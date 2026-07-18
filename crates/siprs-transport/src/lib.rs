//! # siprs-transport
//!
//! SIP 协议栈传输层实现，支持 UDP、TCP、TLS、WebSocket 四种传输协议。
//!
//! 提供完整的 SIP 传输层功能，包括：
//!
//! - **Transport trait** — 传输协议抽象接口，支持自定义实现
//! - **SipCodec** — TCP 流分帧编解码器（基于 Content-Length）
//! - **UdpTransport** — UDP 传输
//! - **TcpConnection/TcpListener** — TCP 传输
//! - **TlsConnection** — TLS 传输（feature-gated，需要 `tls-rustls` feature）
//! - **WsConnection/WsListener** — WebSocket 传输（feature-gated，需要 `ws` feature，RFC 7118）
//! - **ConnectionPool** — 连接池与连接复用
//! - **DnsResolver/TrustDnsResolver** — DNS 解析（RFC 3263）
//! - **TransportManager** — 传输管理器（统一管理所有传输）
//!
//! # 传输协议选择规则
//!
//! - `sips:bob@example.com` → TLS 传输
//! - `sip+ws:bob@example.com` → WebSocket 传输（RFC 7118）
//! - `sip+wss:bob@example.com` → WebSocket Secure 传输（RFC 7118）
//! - `sip:bob@example.com;transport=tcp` → TCP 传输
//! - `sip:bob@example.com;transport=ws` → WebSocket 传输
//! - `sip:bob@example.com`（无传输参数）→ UDP 传输
//! - UDP 消息超过 MTU 限制 → 自动切换 TCP
//!
//! # Feature Flags
//!
//! | Feature | 默认 | 说明 |
//! |---------|------|------|
//! | `tls-rustls` | 启用 | 基于 rustls 的 TLS 传输 |
//! | `tls-native` | 禁用 | 预留：基于 native-tls 的 TLS 传输 |
//! | `ws` | 启用 | 基于 tokio-tungstenite 的 WebSocket 传输（RFC 7118）|
//! | `wss` | 禁用 | WebSocket Secure 传输（需要 native-tls）|
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
//! ```

pub mod codec;
pub mod connection_pool;
pub mod dns;
pub mod manager;
pub mod tcp;
pub mod traits;
pub mod udp;

pub mod tls;

#[cfg(feature = "ws")]
pub mod ws;

// 重导出核心类型
pub use codec::SipCodec;
pub use connection_pool::ConnectionPool;
pub use dns::{DnsResolver, SystemDnsResolver, TrustDnsResolver};
pub use manager::TransportManager;
pub use tcp::{TcpConnection, TcpListener};
pub use traits::{ReceivedMessage, Transport, TransportEvent};
pub use udp::UdpTransport;

#[cfg(feature = "ws")]
pub use ws::{
    ClientWsWriteStream, ServerWsWriteStream, WsConnection, WsListener, WsReadStream, WsWriteStream,
};

/// SIP 默认最大消息大小
pub const SIP_DEFAULT_MAX_MESSAGE_SIZE: usize = 65535;
