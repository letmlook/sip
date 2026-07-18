//! SIP TCP 传输实现
//!
//! 使用 `tokio::net::TcpStream` 和 `Framed` 编解码实现基于 TCP 的 SIP 消息传输。
//! 基于 Content-Length 的消息分帧，支持连接复用。
//!
//! # 架构
//!
//! TCP 连接在 accept 后会被拆分为读写两半：
//! - `TcpReadStream`：由接收循环任务持有，负责读取并解析消息
//! - `TcpWriteStream`：由连接池持有，负责发送消息
//!
//! 这种拆分避免了 `Arc::try_unwrap` 的所有权问题，同时允许读写并发进行。

use std::net::SocketAddr;
use std::sync::Arc;

use bytes::BytesMut;
use futures::{SinkExt, StreamExt};
use sip_core::{TransportError, TransportProtocol};
use tokio::net::{TcpListener as TokioTcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_util::codec::Framed;
use tracing;

use crate::codec::SipCodec;
use crate::traits::{ReceivedMessage, TransportEvent};

// ============================================================================
// TcpReadStream - TCP 读取流
// ============================================================================

/// TCP 读取流
///
/// 持有 `Framed` 的读取半部分，用于接收循环中读取消息。
/// 由 `TcpConnection::into_split()` 产生。
pub struct TcpReadStream {
    stream: futures::stream::SplitStream<Framed<TcpStream, SipCodec>>,
    peer_addr: SocketAddr,
}

impl TcpReadStream {
    /// 启动 TCP 连接的接收循环
    ///
    /// 从 Framed 流中持续读取消息，解析后通过事件通道发送。
    ///
    /// # 参数
    ///
    /// - `event_tx` - 传输事件发送端
    pub async fn receive_loop(mut self, event_tx: mpsc::Sender<TransportEvent>) {
        let peer_addr = self.peer_addr;
        let parser = sip_message::MessageParser::default();

        // 通知连接建立
        let _ = event_tx
            .send(TransportEvent::ConnectionEstablished(
                peer_addr,
                TransportProtocol::Tcp,
            ))
            .await;

        loop {
            match self.stream.next().await {
                Some(Ok(data)) => {
                    // 解析 SIP 消息
                    match parser.parse(&data) {
                        Ok(message) => {
                            let received = ReceivedMessage {
                                message,
                                source_addr: peer_addr,
                                transport: TransportProtocol::Tcp,
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
                                "TCP: failed to parse message from {}: {}",
                                peer_addr,
                                e
                            );
                        }
                    }
                }
                Some(Err(e)) => {
                    tracing::warn!("TCP read error from {}: {}", peer_addr, e);
                    break;
                }
                None => {
                    // 流结束，连接关闭
                    tracing::debug!("TCP connection closed: {}", peer_addr);
                    break;
                }
            }
        }

        // 通知连接断开
        let _ = event_tx
            .send(TransportEvent::ConnectionLost(
                peer_addr,
                TransportProtocol::Tcp,
            ))
            .await;
    }
}

// ============================================================================
// TcpWriteStream - TCP 写入流
// ============================================================================

/// TCP 写入流
///
/// 持有 `Framed` 的写入半部分，用于发送消息。
/// 由 `TcpConnection::into_split()` 产生，存储在连接池中。
pub struct TcpWriteStream {
    sink: futures::stream::SplitSink<Framed<TcpStream, SipCodec>, BytesMut>,
    peer_addr: SocketAddr,
    local_addr: SocketAddr,
}

impl TcpWriteStream {
    /// 获取对端地址
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    /// 获取本地地址
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// 获取传输协议类型
    pub fn protocol(&self) -> TransportProtocol {
        TransportProtocol::Tcp
    }

    /// 发送原始字节消息
    ///
    /// 通过 Framed 编解码器发送消息。
    pub async fn send_raw(&mut self, message: BytesMut) -> Result<(), TransportError> {
        self.sink
            .send(message)
            .await
            .map_err(|e| TransportError::SendFailed {
                reason: format!("TCP send to {} failed: {}", self.peer_addr, e),
            })
    }

    /// 关闭写入流
    ///
    /// 关闭底层的 SplitSink，发送端将不再可用。
    pub async fn close(&mut self) -> Result<(), TransportError> {
        self.sink
            .close()
            .await
            .map_err(|e| TransportError::SendFailed {
                reason: format!("TCP close {} failed: {}", self.peer_addr, e),
            })
    }
}

// ============================================================================
// TcpConnection - TCP 连接（临时，用于创建后立即拆分）
// ============================================================================

/// TCP 连接
///
/// 封装一个已建立的 TCP 连接，使用 `Framed<TcpStream, SipCodec>` 进行
/// 基于 Content-Length 的消息分帧。
///
/// 创建后应立即调用 `into_split()` 拆分为读写两半。
pub struct TcpConnection {
    stream: Framed<TcpStream, SipCodec>,
    peer_addr: SocketAddr,
    local_addr: SocketAddr,
}

impl TcpConnection {
    /// 从已有的 TCP 流创建连接
    ///
    /// # 参数
    ///
    /// - `stream` - 已建立的 TCP 流
    /// - `max_message_size` - 最大消息大小限制
    pub async fn new(stream: TcpStream, max_message_size: usize) -> Result<Self, TransportError> {
        let peer_addr = stream
            .peer_addr()
            .map_err(|e| TransportError::ConnectionFailed {
                addr: "unknown".to_string(),
                reason: format!("failed to get peer address: {}", e),
            })?;
        let local_addr = stream
            .local_addr()
            .map_err(|e| TransportError::ConnectionFailed {
                addr: "unknown".to_string(),
                reason: format!("failed to get local address: {}", e),
            })?;

        let codec = SipCodec::new(max_message_size);
        let framed = Framed::new(stream, codec);

        Ok(Self {
            stream: framed,
            peer_addr,
            local_addr,
        })
    }

    /// 连接到远端地址
    ///
    /// # 参数
    ///
    /// - `addr` - 远端地址
    /// - `max_message_size` - 最大消息大小限制
    pub async fn connect(
        addr: SocketAddr,
        max_message_size: usize,
    ) -> Result<Self, TransportError> {
        let stream =
            TcpStream::connect(addr)
                .await
                .map_err(|e| TransportError::ConnectionFailed {
                    addr: addr.to_string(),
                    reason: e.to_string(),
                })?;
        Self::new(stream, max_message_size).await
    }

    /// 获取对端地址
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    /// 获取本地地址
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// 获取传输协议类型
    pub fn protocol(&self) -> TransportProtocol {
        TransportProtocol::Tcp
    }

    /// 将连接拆分为读写两半
    ///
    /// 拆分后：
    /// - `TcpReadStream` 用于接收循环（读取消息）
    /// - `TcpWriteStream` 用于连接池（发送消息）
    ///
    /// 这种设计允许读写并发进行，避免了 `Arc<Mutex<>>` 的锁竞争问题。
    pub fn into_split(self) -> (TcpReadStream, TcpWriteStream) {
        let (sink, stream) = self.stream.split();
        (
            TcpReadStream {
                stream,
                peer_addr: self.peer_addr,
            },
            TcpWriteStream {
                sink,
                peer_addr: self.peer_addr,
                local_addr: self.local_addr,
            },
        )
    }

    /// 发送原始字节消息（用于客户端主动连接场景）
    ///
    /// 通过 Framed 编解码器发送消息。
    pub async fn send_raw(&mut self, message: BytesMut) -> Result<(), TransportError> {
        self.stream
            .send(message)
            .await
            .map_err(|e| TransportError::SendFailed {
                reason: format!("TCP send to {} failed: {}", self.peer_addr, e),
            })
    }
}

// ============================================================================
// TcpListener - TCP 监听器
// ============================================================================

/// TCP 监听器
///
/// 监听指定地址的 TCP 连接请求，为每个新连接创建 `TcpConnection` 并
/// 拆分为读写两半，分别启动接收循环和注册到连接池。
pub struct TcpListener {
    listener: TokioTcpListener,
    local_addr: SocketAddr,
}

impl TcpListener {
    /// 绑定到指定地址创建 TCP 监听器
    ///
    /// # 参数
    ///
    /// - `addr` - 绑定的本地地址（如 `0.0.0.0:5060`）
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

    /// 启动 TCP 接受连接循环
    ///
    /// 持续接受新的 TCP 连接，为每个连接：
    /// 1. 拆分为 `TcpReadStream` 和 `TcpWriteStream`
    /// 2. 将 `TcpWriteStream` 发送到连接池通道
    /// 3. 为 `TcpReadStream` 启动独立的接收循环任务
    ///
    /// # 参数
    ///
    /// - `event_tx` - 传输事件发送端
    /// - `max_message_size` - 最大消息大小限制
    /// - `connection_tx` - 新连接通知通道（发送写入流到连接池）
    pub async fn accept_loop(
        &self,
        event_tx: mpsc::Sender<TransportEvent>,
        max_message_size: usize,
        connection_tx: mpsc::Sender<Arc<tokio::sync::Mutex<TcpWriteStream>>>,
    ) {
        loop {
            match self.listener.accept().await {
                Ok((stream, peer_addr)) => {
                    tracing::info!("TCP: new connection from {}", peer_addr);

                    match TcpConnection::new(stream, max_message_size).await {
                        Ok(conn) => {
                            // 拆分为读写两半
                            let (read_stream, write_stream) = conn.into_split();

                            // 将写入流注册到连接池
                            let write_stream = Arc::new(tokio::sync::Mutex::new(write_stream));
                            if connection_tx.send(Arc::clone(&write_stream)).await.is_err() {
                                tracing::debug!("TCP accept loop: connection channel closed");
                                break;
                            }

                            // 启动该连接的接收循环（读取流拥有所有权，无需 Arc::try_unwrap）
                            let event_tx_clone = event_tx.clone();
                            tokio::spawn(async move {
                                read_stream.receive_loop(event_tx_clone).await;
                            });
                        }
                        Err(e) => {
                            tracing::error!(
                                "TCP: failed to create connection from {}: {}",
                                peer_addr,
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("TCP accept error: {}", e);
                }
            }
        }
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tcp_listener_bind() {
        let listener = TcpListener::bind("127.0.0.1:0".parse().unwrap()).await;
        assert!(listener.is_ok());
        let listener = listener.unwrap();
        assert_ne!(listener.local_addr().port(), 0);
    }

    #[tokio::test]
    async fn test_tcp_connection_connect() {
        let listener = TcpListener::bind("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let addr = listener.local_addr();

        // 在后台接受连接
        tokio::spawn(async move {
            let (event_tx, _event_rx) = mpsc::channel(100);
            let (conn_tx, _conn_rx) = mpsc::channel(100);
            listener.accept_loop(event_tx, 65535, conn_tx).await;
        });

        // 连接到监听器
        let conn = TcpConnection::connect(addr, 65535).await;
        assert!(conn.is_ok());
    }

    #[tokio::test]
    async fn test_tcp_connection_protocol() {
        let listener = TcpListener::bind("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let addr = listener.local_addr();

        tokio::spawn(async move {
            let (event_tx, _event_rx) = mpsc::channel(100);
            let (conn_tx, _conn_rx) = mpsc::channel(100);
            listener.accept_loop(event_tx, 65535, conn_tx).await;
        });

        let conn = TcpConnection::connect(addr, 65535).await.unwrap();
        assert_eq!(conn.protocol(), TransportProtocol::Tcp);
    }

    #[tokio::test]
    async fn test_tcp_connection_connect_failed() {
        let result = TcpConnection::connect("127.0.0.1:1".parse().unwrap(), 65535).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_tcp_connection_into_split() {
        let listener = TcpListener::bind("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let addr = listener.local_addr();

        tokio::spawn(async move {
            let (event_tx, _event_rx) = mpsc::channel(100);
            let (conn_tx, _conn_rx) = mpsc::channel(100);
            listener.accept_loop(event_tx, 65535, conn_tx).await;
        });

        let conn = TcpConnection::connect(addr, 65535).await.unwrap();
        let (_read_stream, write_stream) = conn.into_split();
        assert_eq!(write_stream.peer_addr(), addr);
        assert_eq!(write_stream.protocol(), TransportProtocol::Tcp);
    }
}
