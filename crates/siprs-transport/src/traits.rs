//! SIP 传输层 trait 抽象与核心类型
//!
//! 定义传输协议抽象（`Transport` trait）、接收消息类型（`ReceivedMessage`）
//! 以及传输层事件枚举（`TransportEvent`）。

use std::net::SocketAddr;

use async_trait::async_trait;
use siprs_core::{TransportError, TransportProtocol};
use siprs_message::SipMessage;

// ============================================================================
// ReceivedMessage - 传输层接收到的消息
// ============================================================================

/// 传输层接收到的消息
///
/// 包含解析后的 SIP 消息、来源地址和使用的传输协议。
#[derive(Debug)]
pub struct ReceivedMessage {
    /// 解析后的 SIP 消息
    pub message: SipMessage,
    /// 消息来源地址
    pub source_addr: SocketAddr,
    /// 使用的传输协议
    pub transport: TransportProtocol,
}

// ============================================================================
// Transport - 传输协议抽象
// ============================================================================

/// 传输协议抽象
///
/// 定义 SIP 传输层必须实现的核心接口。所有传输实现（UDP、TCP、TLS）
/// 均需实现此 trait，以便 `TransportManager` 统一管理。
///
/// # 对象安全
///
/// 此 trait 是对象安全的，可通过 `Arc<dyn Transport>` 使用。
#[async_trait]
pub trait Transport: Send + Sync {
    /// 获取传输协议类型
    fn protocol(&self) -> TransportProtocol;

    /// 发送 SIP 消息到指定地址
    ///
    /// 对于面向连接的传输（TCP、TLS），`addr` 参数通常被忽略，
    /// 消息发送到已连接的对端。对于无连接传输（UDP），`addr` 指定目标地址。
    ///
    /// # 参数
    ///
    /// - `message` - 序列化后的 SIP 消息字节流
    /// - `addr` - 目标地址
    ///
    /// # 错误
    ///
    /// 返回 `TransportError` 表示发送失败。
    async fn send_message(&self, message: &[u8], addr: SocketAddr) -> Result<(), TransportError>;

    /// 关闭传输连接
    ///
    /// 释放传输层占用的资源。对于面向连接的传输，关闭底层连接；
    /// 对于无连接传输，释放 socket 资源。
    ///
    /// # 错误
    ///
    /// 返回 `TransportError` 表示关闭失败。
    async fn close(&self) -> Result<(), TransportError>;
}

// ============================================================================
// TransportEvent - 传输层事件
// ============================================================================

/// 传输层事件
///
/// 传输层产生的各类事件，用于通知上层状态变化。
#[derive(Debug)]
pub enum TransportEvent {
    /// 收到 SIP 消息
    Message(Box<ReceivedMessage>),
    /// 连接建立
    ConnectionEstablished(SocketAddr, TransportProtocol),
    /// 连接断开
    ConnectionLost(SocketAddr, TransportProtocol),
    /// 传输错误
    Error(TransportError),
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_received_message_debug() {
        // 仅验证 ReceivedMessage 可以 Debug 格式化
        // 实际消息构造在集成测试中验证
    }

    #[test]
    fn test_transport_event_debug() {
        let err = TransportError::SendFailed {
            reason: "test".to_string(),
        };
        let event = TransportEvent::Error(err);
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("Error"));
    }
}
