//! 自定义传输层示例
//!
//! 演示如何实现自定义的 Transport trait，
//! 并使用自定义传输层创建 SipEngine。
//!
//! # 运行说明
//!
//! 此示例展示了 Transport trait 的实现方式。
//! 在实际部署中，自定义传输层可以用于：
//! - WebSocket 传输
//! - 内存传输（用于测试）
//! - 自定义协议封装

use std::net::SocketAddr;

use async_trait::async_trait;
use sip_core::config::SipConfig;
use sip_core::{TransportError, TransportProtocol};
use sip_transport::traits::Transport;

// ============================================================================
// 自定义传输层实现 - 内存传输（用于测试）
// ============================================================================

/// 内存传输
///
/// 一个简单的内存传输实现，不实际发送数据，
/// 用于测试和演示 Transport trait 的实现方式。
pub struct InMemoryTransport {
    /// 传输协议类型
    protocol: TransportProtocol,
    /// 发送计数
    send_count: std::sync::atomic::AtomicU64,
}

impl InMemoryTransport {
    /// 创建新的内存传输
    pub fn new(protocol: TransportProtocol) -> Self {
        Self {
            protocol,
            send_count: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// 获取发送计数
    pub fn send_count(&self) -> u64 {
        self.send_count.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[async_trait]
impl Transport for InMemoryTransport {
    /// 获取传输协议类型
    fn protocol(&self) -> TransportProtocol {
        self.protocol
    }

    /// 发送 SIP 消息到指定地址
    ///
    /// 在内存传输中，仅记录发送次数，不实际发送数据。
    async fn send_message(&self, message: &[u8], addr: SocketAddr) -> Result<(), TransportError> {
        // 在实际实现中，这里会将消息发送到网络
        // 此示例仅记录发送操作
        self.send_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        println!(
            "[InMemoryTransport] 发送消息: {} 字节 -> {} (协议: {})",
            message.len(),
            addr,
            self.protocol
        );

        // 如果需要，可以在此处解析并打印消息内容
        if let Ok(text) = std::str::from_utf8(message) {
            // 仅打印第一行（请求行或状态行）
            if let Some(first_line) = text.lines().next() {
                println!("    第一行: {}", first_line);
            }
        }

        Ok(())
    }

    /// 关闭传输连接
    ///
    /// 在内存传输中，无需关闭任何连接。
    async fn close(&self) -> Result<(), TransportError> {
        println!("[InMemoryTransport] 传输已关闭");
        Ok(())
    }
}

// ============================================================================
// 主函数
// ============================================================================

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::fmt::init();

    println!("=== SIP 自定义传输层示例 ===\n");

    // ========================================================================
    // 1. 创建自定义传输实例
    // ========================================================================

    let transport = InMemoryTransport::new(TransportProtocol::Udp);
    println!("[1] 自定义传输已创建（协议: {}）", transport.protocol());

    // ========================================================================
    // 2. 使用自定义传输发送消息
    // ========================================================================

    let addr: SocketAddr = "192.168.1.1:5060".parse().unwrap();

    // 构建一个简单的 INVITE 消息
    let invite_message = b"INVITE sip:bob@example.com SIP/2.0\r\n\
                           Via: SIP/2.0/UDP 192.168.1.100:5060;branch=z9hG4bK-test123\r\n\
                           From: <sip:alice@example.com>;tag=abc123\r\n\
                           To: <sip:bob@example.com>\r\n\
                           Call-ID: test-call@example.com\r\n\
                           CSeq: 1 INVITE\r\n\
                           Contact: <sip:alice@192.168.1.100:5060>\r\n\
                           Content-Length: 0\r\n\
                           \r\n";

    match transport.send_message(invite_message, addr).await {
        Ok(()) => println!("[2] 消息发送成功"),
        Err(e) => println!("[2] 消息发送失败: {}", e),
    }

    // 发送第二个消息
    let bye_message = b"BYE sip:bob@example.com SIP/2.0\r\n\
                        Via: SIP/2.0/UDP 192.168.1.100:5060;branch=z9hG4bK-test456\r\n\
                        From: <sip:alice@example.com>;tag=abc123\r\n\
                        To: <sip:bob@example.com>;tag=xyz789\r\n\
                        Call-ID: test-call@example.com\r\n\
                        CSeq: 2 BYE\r\n\
                        Content-Length: 0\r\n\
                        \r\n";

    match transport.send_message(bye_message, addr).await {
        Ok(()) => println!("[3] BYE 消息发送成功"),
        Err(e) => println!("[3] BYE 消息发送失败: {}", e),
    }

    // ========================================================================
    // 3. 查看发送统计
    // ========================================================================

    println!("\n[4] 发送统计: 共发送 {} 条消息", transport.send_count());

    // ========================================================================
    // 4. 关闭传输
    // ========================================================================

    match transport.close().await {
        Ok(()) => println!("[5] 传输已关闭"),
        Err(e) => println!("[5] 关闭失败: {}", e),
    }

    // ========================================================================
    // 补充说明：如何在 SipEngine 中使用自定义传输
    // ========================================================================

    println!("\n=== 如何在 SipEngine 中使用自定义传输 ===\n");

    // 创建 SipConfig（与标准流程相同）
    let _config = SipConfig::builder()
        .aor("sip:alice@example.com")
        .contact("sip:alice@192.168.1.100:5060")
        .build()
        .expect("配置构建失败");

    println!("SipConfig 已创建");
    println!();
    println!("注意：当前 SipEngine 内部使用 TransportManager 管理传输层。");
    println!("要使用自定义传输，有以下方式：");
    println!("1. 实现 Transport trait（如本示例所示）");
    println!("2. 通过 TransportManager 注册自定义传输实现");
    println!("3. 在测试中使用 InMemoryTransport 替代真实网络传输");
    println!();
    println!("Transport trait 的核心方法：");
    println!("  - protocol() -> TransportProtocol     // 返回传输协议类型");
    println!("  - send_message(&[u8], SocketAddr)     // 发送消息到指定地址");
    println!("  - close()                             // 关闭传输连接");
}
