//! SIP Transport - SIP 协议栈传输层实现
//!
//! 提供 UDP、TCP、TLS 三种传输协议的完整实现，包括：
//!
//! - **Transport trait** - 传输协议抽象接口
//! - **SipCodec** - TCP 流分帧编解码器（基于 Content-Length）
//! - **UdpTransport** - UDP 传输
//! - **TcpConnection/TcpListener** - TCP 传输
//! - **TlsConnection** - TLS 传输（feature-gated，需要 `tls-rustls` feature）
//! - **ConnectionPool** - 连接池与连接复用
//! - **DnsResolver/TrustDnsResolver** - DNS 解析（RFC 3263）
//! - **TransportManager** - 传输管理器（统一管理所有传输）
//!
//! # 传输协议选择规则
//!
//! - `sips:bob@example.com` → TLS 传输
//! - `sip:bob@example.com;transport=tcp` → TCP 传输
//! - `sip:bob@example.com`（无传输参数）→ UDP 传输
//! - UDP 消息超过 MTU 限制 → 自动切换 TCP
//!
//! # 示例
//!
//! ```ignore
//! use siprs_transport::TransportManager;
//! use siprs_core::config::{TransportConfig, TlsConfig};
//! use siprs_core::metrics::SipMetrics;
//!
//! let mut manager = TransportManager::new(
//!     TransportConfig::default(),
//!     TlsConfig::default(),
//!     std::sync::Arc::new(SipMetrics::new()),
//! );
//!
//! // 启动传输层
//! manager.start("0.0.0.0:5060".parse().unwrap()).await?;
//!
//! // 发送消息
//! manager.send(&message, "sip:bob@example.com;transport=tcp").await?;
//!
//! // 接收消息
//! let mut stream = manager.message_stream().unwrap();
//! while let Some(event) = stream.recv().await {
//!     // 处理事件
//! }
//! ```

pub mod codec;
pub mod connection_pool;
pub mod dns;
pub mod manager;
pub mod tcp;
pub mod traits;
pub mod udp;

pub mod tls;

// 重导出核心类型
pub use codec::SipCodec;
pub use connection_pool::ConnectionPool;
pub use dns::{DnsResolver, SystemDnsResolver, TrustDnsResolver};
pub use manager::TransportManager;
pub use tcp::{TcpConnection, TcpListener};
pub use traits::{ReceivedMessage, Transport, TransportEvent};
pub use udp::UdpTransport;

/// SIP 默认最大消息大小
pub const SIP_DEFAULT_MAX_MESSAGE_SIZE: usize = 65535;
