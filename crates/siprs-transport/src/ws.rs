//! SIP WebSocket 传输实现（RFC 7118）
//!
//! 使用 `tokio-tungstenite` 实现 SIP over WebSocket 传输。
//! 支持 `ws://` 和 `wss://` 协议，使用文本帧传输 SIP 消息。
//!
//! # 架构
//!
//! WebSocket 连接与 TCP 类似，在 accept 后会被拆分为读写两半：
//! - `WsReadStream`：由接收循环任务持有，负责读取并解析消息
//! - `WsWriteStream`：由连接池持有，负责发送消息
//!
//! # RFC 7118 关键约束
//!
//! - SIP 消息必须使用文本帧（opcode 0x1）传输，不使用二进制帧
//! - WebSocket 子协议应为 `sip`（Sec-WebSocket-Protocol: sip）
//! - 消息分帧由 WebSocket 协议本身处理，无需额外的 Content-Length 分帧

use std::net::SocketAddr;
use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use siprs_core::{TransportError, TransportProtocol};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener as TokioTcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        handshake::server::{Request, Response},
        protocol::Message,
    },
    WebSocketStream,
};
use tracing;

use crate::traits::{ReceivedMessage, TransportEvent};

/// 客户端 WebSocket 写入流类型别名（用于连接池）
pub type ClientWsWriteStream =
    WsWriteStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// 服务端 WebSocket 写入流类型别名
pub type ServerWsWriteStream = WsWriteStream<tokio::net::TcpStream>;

// ============================================================================
// WsReadStream - WebSocket 读取流
// ============================================================================

/// WebSocket 读取流（泛型版本）
///
/// 持有 WebSocket 流的读取半部分，用于接收循环中读取消息。
/// 由 `WsConnection::into_split()` 产生。
pub struct WsReadStream<S> {
    stream: futures::stream::SplitStream<WebSocketStream<S>>,
    peer_addr: SocketAddr,
}

impl<S: AsyncRead + AsyncWrite + Unpin> WsReadStream<S> {
    /// 启动 WebSocket 连接的接收循环
    ///
    /// 从 WebSocket 流中持续读取文本消息，解析后通过事件通道发送。
    /// 忽略非文本帧（二进制帧、Ping/Pong 等）。
    ///
    /// # 参数
    ///
    /// - `event_tx` - 传输事件发送端
    pub async fn receive_loop(mut self, event_tx: mpsc::Sender<TransportEvent>) {
        let peer_addr = self.peer_addr;
        let parser = siprs_message::MessageParser::default();

        // 通知连接建立
        let _ = event_tx
            .send(TransportEvent::ConnectionEstablished(
                peer_addr,
                TransportProtocol::Ws,
            ))
            .await;

        loop {
            match self.stream.next().await {
                Some(Ok(msg)) => {
                    match msg {
                        Message::Text(text) => {
                            // RFC 7118: SIP 消息使用文本帧传输
                            match parser.parse(text.as_bytes()) {
                                Ok(message) => {
                                    let received = ReceivedMessage {
                                        message,
                                        source_addr: peer_addr,
                                        transport: TransportProtocol::Ws,
                                    };
                                    if event_tx
                                        .send(TransportEvent::Message(Box::new(received)))
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "WS: failed to parse message from {}: {}",
                                        peer_addr,
                                        e
                                    );
                                }
                            }
                        }
                        Message::Binary(_) => {
                            // RFC 7118: 不应使用二进制帧传输 SIP 消息
                            tracing::warn!(
                                "WS: received binary frame from {}, ignoring (RFC 7118 requires text frames)",
                                peer_addr
                            );
                        }
                        Message::Ping(data) => {
                            // Ping/Pong 由 tungstenite 自动处理，此处仅记录日志
                            tracing::trace!(
                                "WS: received ping from {} ({} bytes)",
                                peer_addr,
                                data.len()
                            );
                        }
                        Message::Pong(data) => {
                            tracing::trace!(
                                "WS: received pong from {} ({} bytes)",
                                peer_addr,
                                data.len()
                            );
                        }
                        Message::Close(frame) => {
                            tracing::debug!(
                                "WS: received close frame from {}: {:?}",
                                peer_addr,
                                frame
                            );
                            break;
                        }
                        Message::Frame(_) => {
                            // 原始帧，忽略
                        }
                    }
                }
                Some(Err(e)) => {
                    tracing::warn!("WS read error from {}: {}", peer_addr, e);
                    break;
                }
                None => {
                    tracing::debug!("WS connection closed: {}", peer_addr);
                    break;
                }
            }
        }

        // 通知连接断开
        let _ = event_tx
            .send(TransportEvent::ConnectionLost(
                peer_addr,
                TransportProtocol::Ws,
            ))
            .await;
    }
}

// ============================================================================
// WsWriteStream - WebSocket 写入流
// ============================================================================

/// WebSocket 写入流（泛型版本）
///
/// 持有 WebSocket 流的写入半部分，用于发送消息。
/// 由 `WsConnection::into_split()` 产生，存储在连接池中。
pub struct WsWriteStream<S> {
    sink: futures::stream::SplitSink<WebSocketStream<S>, Message>,
    peer_addr: SocketAddr,
}

impl<S: AsyncRead + AsyncWrite + Unpin> WsWriteStream<S> {
    /// 获取对端地址
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    /// 获取传输协议类型
    pub fn protocol(&self) -> TransportProtocol {
        TransportProtocol::Ws
    }

    /// 发送 SIP 消息（文本帧）
    ///
    /// 将原始字节作为文本帧发送。RFC 7118 要求 SIP 消息使用文本帧传输。
    ///
    /// # 参数
    ///
    /// - `message` - 序列化后的 SIP 消息字节
    ///
    /// # 错误
    ///
    /// - 如果消息不是有效的 UTF-8，返回 `TransportError::SendFailed`
    /// - 如果 WebSocket 发送失败，返回 `TransportError::SendFailed`
    pub async fn send_raw(&mut self, message: &[u8]) -> Result<(), TransportError> {
        // 将字节转换为文本（SIP 消息必须是有效的 UTF-8）
        let text = std::str::from_utf8(message).map_err(|e| TransportError::SendFailed {
            reason: format!(
                "WS send to {} failed: SIP message is not valid UTF-8: {}",
                self.peer_addr, e
            ),
        })?;

        self.sink
            .send(Message::Text(text.into()))
            .await
            .map_err(|e| TransportError::SendFailed {
                reason: format!("WS send to {} failed: {}", self.peer_addr, e),
            })
    }

    /// 关闭写入流
    ///
    /// 发送 WebSocket Close 帧并关闭底层连接。
    pub async fn close(&mut self) -> Result<(), TransportError> {
        self.sink
            .close()
            .await
            .map_err(|e| TransportError::SendFailed {
                reason: format!("WS close {} failed: {}", self.peer_addr, e),
            })
    }
}

// ============================================================================
// WsConnection - WebSocket 连接（泛型版本）
// ============================================================================

/// WebSocket 连接
///
/// 封装一个已建立的 WebSocket 连接，使用 `WebSocketStream` 进行
/// 基于 RFC 7118 的 SIP 消息传输。
///
/// 创建后应立即调用 `into_split()` 拆分为读写两半。
pub struct WsConnection<S> {
    stream: WebSocketStream<S>,
    peer_addr: SocketAddr,
}

// ---- 客户端连接（MaybeTlsStream<TcpStream>）----

impl WsConnection<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    /// 连接到远端 WebSocket 服务器
    ///
    /// # 参数
    ///
    /// - `uri` - WebSocket 服务器 URI（如 `ws://example.com:5060` 或 `wss://example.com:5061`）
    ///
    /// # 错误
    ///
    /// 返回 `TransportError` 表示连接失败。
    pub async fn connect(uri: &str) -> Result<Self, TransportError> {
        let (stream, _response) = tokio_tungstenite::connect_async(uri).await.map_err(|e| {
            TransportError::ConnectionFailed {
                addr: uri.to_string(),
                reason: format!("WebSocket connect failed: {}", e),
            }
        })?;

        let peer_addr = extract_peer_addr_from_uri(uri);

        Ok(Self { stream, peer_addr })
    }
}

// ---- 服务端连接（TcpStream）----

impl WsConnection<tokio::net::TcpStream> {
    /// 从已接受的 WebSocket 流创建服务端连接
    ///
    /// # 参数
    ///
    /// - `stream` - 已建立的 WebSocket 流
    /// - `peer_addr` - 对端地址
    pub fn from_server_stream(
        stream: WebSocketStream<tokio::net::TcpStream>,
        peer_addr: SocketAddr,
    ) -> Self {
        Self { stream, peer_addr }
    }
}

// ---- 通用方法 ----

impl<S: AsyncRead + AsyncWrite + Unpin> WsConnection<S> {
    /// 获取对端地址
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    /// 获取传输协议类型
    pub fn protocol(&self) -> TransportProtocol {
        TransportProtocol::Ws
    }

    /// 将连接拆分为读写两半
    ///
    /// 拆分后：
    /// - `WsReadStream` 用于接收循环（读取消息）
    /// - `WsWriteStream` 用于连接池（发送消息）
    pub fn into_split(self) -> (WsReadStream<S>, WsWriteStream<S>) {
        let (sink, stream) = self.stream.split();
        (
            WsReadStream {
                stream,
                peer_addr: self.peer_addr,
            },
            WsWriteStream {
                sink,
                peer_addr: self.peer_addr,
            },
        )
    }

    /// 发送 SIP 消息（文本帧）
    ///
    /// 通过 WebSocket 文本帧发送消息。
    pub async fn send_raw(&mut self, message: &[u8]) -> Result<(), TransportError> {
        let text = std::str::from_utf8(message).map_err(|e| TransportError::SendFailed {
            reason: format!(
                "WS send to {} failed: SIP message is not valid UTF-8: {}",
                self.peer_addr, e
            ),
        })?;

        self.stream
            .send(Message::Text(text.into()))
            .await
            .map_err(|e| TransportError::SendFailed {
                reason: format!("WS send to {} failed: {}", self.peer_addr, e),
            })
    }
}

// ============================================================================
// WsListener - WebSocket 服务端监听器
// ============================================================================

/// WebSocket 服务端监听器
///
/// 监听指定地址的 TCP 连接请求，将每个连接升级为 WebSocket 连接，
/// 并为每个连接创建 `WsConnection`，拆分为读写两半，
/// 分别启动接收循环和注册到连接池。
pub struct WsListener {
    listener: TokioTcpListener,
    local_addr: SocketAddr,
}

impl WsListener {
    /// 绑定到指定地址创建 WebSocket 监听器
    ///
    /// # 参数
    ///
    /// - `addr` - 绑定的本地地址（如 `0.0.0.0:80`）
    ///
    /// # 错误
    ///
    /// 返回 `TransportError::BindFailed` 表示绑定失败。
    pub async fn bind(addr: SocketAddr) -> Result<Self, TransportError> {
        let listener =
            TokioTcpListener::bind(addr)
                .await
                .map_err(|e| TransportError::BindFailed {
                    addr: addr.to_string(),
                    reason: e.to_string(),
                })?;
        let local_addr = listener
            .local_addr()
            .map_err(|e| TransportError::BindFailed {
                addr: addr.to_string(),
                reason: e.to_string(),
            })?;
        Ok(Self {
            listener,
            local_addr,
        })
    }

    /// 获取本地绑定地址
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// 启动 WebSocket 接受连接循环
    ///
    /// 持续接受新的 TCP 连接，升级为 WebSocket，为每个连接：
    /// 1. 拆分为 `WsReadStream` 和 `WsWriteStream`
    /// 2. 将 `WsWriteStream` 发送到连接池通道
    /// 3. 为 `WsReadStream` 启动独立的接收循环任务
    ///
    /// # 参数
    ///
    /// - `event_tx` - 传输事件发送端
    /// - `connection_tx` - 新连接通知通道（发送写入流到连接池）
    #[allow(clippy::result_large_err)]
    pub async fn accept_loop(
        &self,
        event_tx: mpsc::Sender<TransportEvent>,
        connection_tx: mpsc::Sender<Arc<tokio::sync::Mutex<WsWriteStream<tokio::net::TcpStream>>>>,
    ) {
        loop {
            match self.listener.accept().await {
                Ok((stream, peer_addr)) => {
                    tracing::info!(
                        "WS: new TCP connection from {}, upgrading to WebSocket",
                        peer_addr
                    );

                    // 升级为 WebSocket 连接，设置子协议为 "sip"
                    let ws_stream =
                        match accept_hdr_async(stream, |req: &Request, mut resp: Response| {
                            // 检查客户端是否请求了 "sip" 子协议
                            if let Some(protocols) = req.headers().get("sec-websocket-protocol") {
                                if let Ok(protocols_str) = protocols.to_str() {
                                    if protocols_str.split(',').any(|p| p.trim() == "sip") {
                                        resp.headers_mut().insert(
                                            "sec-websocket-protocol",
                                            "sip".parse().unwrap(),
                                        );
                                    }
                                }
                            }
                            Ok(resp)
                        })
                        .await
                        {
                            Ok(ws) => ws,
                            Err(e) => {
                                tracing::error!(
                                    "WS: failed to upgrade connection from {}: {}",
                                    peer_addr,
                                    e
                                );
                                continue;
                            }
                        };

                    tracing::info!("WS: WebSocket connection established with {}", peer_addr);

                    // 创建 WsConnection 并拆分
                    let conn = WsConnection::from_server_stream(ws_stream, peer_addr);
                    let (read_stream, write_stream) = conn.into_split();

                    // 将写入流注册到连接池
                    let write_stream = Arc::new(tokio::sync::Mutex::new(write_stream));
                    if connection_tx.send(Arc::clone(&write_stream)).await.is_err() {
                        tracing::debug!("WS accept loop: connection channel closed");
                        break;
                    }

                    // 启动该连接的接收循环
                    let event_tx_clone = event_tx.clone();
                    tokio::spawn(async move {
                        read_stream.receive_loop(event_tx_clone).await;
                    });
                }
                Err(e) => {
                    tracing::error!("WS accept error: {}", e);
                }
            }
        }
    }
}

// ============================================================================
// 内部辅助函数
// ============================================================================

/// 从 URI 中解析对端地址
///
/// 从 `ws://host:port` 或 `wss://host:port` 格式的 URI 中解析地址。
fn extract_peer_addr_from_uri(uri: &str) -> SocketAddr {
    // 去掉 scheme 前缀
    let stripped = uri
        .strip_prefix("ws://")
        .or_else(|| uri.strip_prefix("wss://"))
        .unwrap_or(uri);

    // 去掉路径部分
    let host_port = stripped.split('/').next().unwrap_or(stripped);

    host_port
        .parse()
        .unwrap_or(SocketAddr::from(([127, 0, 0, 1], 0)))
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ws_listener_bind() {
        let listener = WsListener::bind("127.0.0.1:0".parse().unwrap()).await;
        assert!(listener.is_ok());
        let listener = listener.unwrap();
        assert_ne!(listener.local_addr().port(), 0);
    }

    #[tokio::test]
    async fn test_ws_connection_connect_failed() {
        // 连接到一个不存在的地址应该失败
        let result = WsConnection::connect("ws://127.0.0.1:1").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_ws_write_stream_protocol() {
        // 仅验证协议类型
        assert_eq!(TransportProtocol::Ws.default_port(), 80);
        assert!(TransportProtocol::Ws.is_reliable());
        assert!(!TransportProtocol::Ws.is_secure());
    }

    #[test]
    fn test_ws_protocol_properties() {
        assert_eq!(TransportProtocol::Wss.default_port(), 5061);
        assert!(TransportProtocol::Wss.is_reliable());
        assert!(TransportProtocol::Wss.is_secure());
    }

    #[test]
    fn test_extract_peer_addr_from_uri() {
        let addr = extract_peer_addr_from_uri("ws://192.168.1.1:8080");
        assert_eq!(addr, "192.168.1.1:8080".parse::<SocketAddr>().unwrap());

        // 域名无法解析为 SocketAddr，应返回默认值
        let addr = extract_peer_addr_from_uri("wss://example.com:443/path");
        assert_eq!(addr, SocketAddr::from(([127, 0, 0, 1], 0)));
    }
}
