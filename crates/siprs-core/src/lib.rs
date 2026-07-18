//! # siprs-core
//!
//! SIP 协议栈核心类型、错误处理、配置与运行指标。
//!
//! 本 crate 是 SIP 协议栈的基石，为所有上层 crate 提供统一的错误类型、
//! 配置管理、运行指标监控和核心数据类型。
//!
//! # 模块结构
//!
//! - [`config`] — SIP 全局配置（`SipConfig`）、传输配置、事务配置、TLS 配置、注册配置
//! - [`error`] — 统一错误体系（`SipError`），覆盖解析、传输、事务、对话、注册等所有错误
//! - [`metrics`] — 无锁运行指标监控（`SipMetrics`），基于 `AtomicU64`
//! - [`types`] — 核心类型（`TransportProtocol`、`Host`、`StatusCode`、`SipVersion` 等）
//!
//! # 示例
//!
//! ```
//! use siprs_core::config::SipConfig;
//!
//! let config = SipConfig::builder()
//!     .aor("sip:alice@example.com")
//!     .contact("sip:alice@192.168.1.100:5060")
//!     .build()
//!     .unwrap();
//! ```

pub mod config;
pub mod error;
pub mod metrics;
pub mod types;

// 重导出常用类型
pub use error::*;
pub use types::*;
