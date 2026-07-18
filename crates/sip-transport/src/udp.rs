//! SIP UDP 传输实现
//!
//! 使用 `tokio::net::UdpSocket` 实现基于 UDP 的 SIP 消息传输。
//! 单条 UDP 数据报包含完整 SIP 消息，无分帧需求。

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use sip_core::{TransportError, TransportProtocol};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing;

use crate::traits::{ReceivedMessage, Transport, TransportEvent};

// ============================================================================
// UdpTransport - UDP 传输
// ============================================================================

/// UDP 传输
///
/// 使用单个 `UdpSocket` 收发 SIP 消息。UDP 是无连接传输，
/// 每条消息独立发送，不需要分帧处理。
///
/// # 限制
///
/// - 单条 UDP 数据报大小受 MTU 限制（默认 1300 字节）
/// - 超过 MTU 限制的消息应自动切换到 TCP 传输
pub struct UdpTransport {
    socket: Arc<UdpSocket>,
    local_addr: SocketAddr,
}

impl UdpTransport {
    /// 绑定到指定地址创建 UDP 传输
    ///
    /// # 参数
    ///
    /// - `addr` - 绑定的本地地址（如 `0.0.0.0:5060`）
    ///
    /// # 错误
    ///
    /// 返回 `TransportError::BindFailed` 表示绑定失败。
    pub async fn bind(addr: SocketAddr) -> Result<Self, TransportError> {
        let socket = UdpSocket::bind(addr)
            .await
            .map_err(|e| TransportError::BindFailed {
                addr: addr.to_string(),
                reason: e.to_string(),
            })?;
        let local_addr = socket
            .local_addr()
            .map_err(|e| TransportError::BindFailed {
                addr: addr.to_string(),
                reason: e.to_string(),
            })?;
        Ok(Self {
            socket: Arc::new(socket),
            local_addr,
        })
    }

    /// 获取本地绑定地址
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// 启动 UDP 消息接收循环
    ///
    /// 从 UDP socket 持续读取数据报，解析为 SIP 消息后通过
    /// `TransportEvent::Message` 事件发送到事件通道。
    ///
    /// # 参数
    ///
    /// - `event_tx` - 传输事件发送端
    /// - `max_message_size` - 最大消息大小限制
    pub async fn receive_loop(
        self: &Arc<Self>,
        event_tx: mpsc::Sender<TransportEvent>,
        max_message_size: usize,
    ) {
        let mut buf = vec![0u8; max_message_size];
        let parser = sip_message::MessageParser::new(max_message_size);

        loop {
            match self.socket.recv_from(&mut buf).await {
                Ok((len, source_addr)) => {
                    let data = &buf[..len];

                    // 解析 SIP 消息
                    match parser.parse(data) {
                        Ok(message) => {
                            let received = ReceivedMessage {
                                message,
                                source_addr,
                                transport: TransportProtocol::Udp,
                            };
                            if event_tx
                                .send(TransportEvent::Message(Box::new(received)))
                                .await
                                .is_err()
                            {
                                tracing::debug!("UDP receive loop: event channel closed");
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "UDP: failed to parse message from {}: {}",
                                source_addr,
                                e
                            );
                            if event_tx
                                .send(TransportEvent::Error(TransportError::ReceiveFailed {
                                    reason: format!("parse error from {}: {}", source_addr, e),
                                }))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("UDP recv_from error: {}", e);
                    if event_tx
                        .send(TransportEvent::Error(TransportError::ReceiveFailed {
                            reason: e.to_string(),
                        }))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    }
}

#[async_trait]
impl Transport for UdpTransport {
    fn protocol(&self) -> TransportProtocol {
        TransportProtocol::Udp
    }

    async fn send_message(&self, message: &[u8], addr: SocketAddr) -> Result<(), TransportError> {
        self.socket
            .send_to(message, addr)
            .await
            .map_err(|e| TransportError::SendFailed {
                reason: format!("UDP send_to {} failed: {}", addr, e),
            })?;
        Ok(())
    }

    async fn close(&self) -> Result<(), TransportError> {
        // UDP socket 不需要显式关闭，离开作用域时自动释放
        Ok(())
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_udp_transport_bind() {
        let transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap()).await;
        assert!(transport.is_ok());
        let transport = transport.unwrap();
        assert_ne!(transport.local_addr().port(), 0);
    }

    #[tokio::test]
    async fn test_udp_transport_protocol() {
        let transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        assert_eq!(transport.protocol(), TransportProtocol::Udp);
    }

    #[tokio::test]
    async fn test_udp_transport_send_receive() {
        let transport1 = UdpTransport::bind("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let transport2 = UdpTransport::bind("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();

        let msg = b"INVITE sip:bob@example.com SIP/2.0\r\nContent-Length: 0\r\n\r\n";
        let result = transport1.send_message(msg, transport2.local_addr()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_udp_transport_close() {
        let transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let result = transport.close().await;
        assert!(result.is_ok());
    }
}
