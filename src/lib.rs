//! # sip-rs
//!
//! 基于 Rust 的完整 SIP 协议栈 + GB28181 国标信令服务器。
//!
//! 本 crate 提供完整的 SIP (Session Initiation Protocol) 协议栈实现，
//! 遵循 RFC 3261 规范，并内置 GB28181 国标适配层，可广泛应用于
//! VoIP、视频监控、即时通讯等场景。
//!
//! # Crate 组织
//!
//! | Crate | 说明 |
//! |-------|------|
//! | [`siprs_core`] | 核心类型、错误处理、配置、运行指标 |
//! | [`siprs_message`] | SIP 消息解析与构建 (RFC 3261) |
//! | [`siprs_transport`] | 传输层 — UDP/TCP/TLS (rustls) |
//! | [`siprs_transaction`] | 事务层 — 4 种状态机、Timer A~K |
//! | [`siprs_dialog`] | 对话层 — 对话 ID 管理、状态跟踪 |
//! | [`siprs_registration`] | 注册层 — MD5 摘要认证、Registrar |
//! | [`siprs_ua`] | 用户代理 — SipEngine、GB28181 设备/平台 |
//! | [`siprs_sdp`] | SDP 解析/构建 + GB28181 媒体扩展 |
//! | [`siprs_media`] | 媒体协商、RTP/RTCP 处理、编解码协商 |
//! | [`siprs_gb28181_codec`] | GB28181 20 位国标编码解析/生成 |
//! | [`siprs_gb28181_xml`] | GB28181 XML (MANSCDP) 消息处理 |
//!
//! # 快速开始
//!
//! ```ignore
//! use siprs_ua::{SipEngine, SipEvent};
//! use siprs_core::config::SipConfig;
//!
//! let config = SipConfig::builder()
//!     .aor("sip:alice@example.com")
//!     .contact("sip:alice@192.168.1.100:5060")
//!     .build()?;
//!
//! let mut engine = SipEngine::new(config);
//! engine.start().await?;
//! engine.register().await?;
//! ```

// 重导出所有子 crate，方便统一访问
pub use siprs_core;
pub use siprs_dialog;
pub use siprs_gb28181_codec;
pub use siprs_gb28181_xml;
pub use siprs_media;
pub use siprs_message;
pub use siprs_registration;
pub use siprs_sdp;
pub use siprs_transaction;
pub use siprs_transport;
pub use siprs_ua;
