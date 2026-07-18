//! SIP Registration - Registration layer implementation for the SIP protocol stack
//!
//! 提供 SIP 注册功能的完整实现，包括：
//!
//! - **注册状态类型** - `RegistrationId`、`RegistrationState`、`RegistrationInfo` 等
//! - **摘要认证** - RFC 2617 MD5 Digest Authentication 处理
//! - **注册管理器** - `RegistrationManager` 管理注册生命周期
//!
//! # 模块结构
//!
//! - [`types`] - 注册状态类型定义
//! - [`auth`] - 摘要认证处理
//! - [`manager`] - 注册管理器
//! - [`registrar`] - 注册服务器实现
//!
//! # 示例
//!
//! ```ignore
//! use sip_registration::manager::RegistrationManager;
//! use sip_core::config::RegistrationConfig;
//! use sip_core::metrics::SipMetrics;
//!
//! let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
//! let config = RegistrationConfig::default();
//! let metrics = Arc::new(SipMetrics::new());
//!
//! let manager = RegistrationManager::new(config, None, event_tx, metrics);
//!
//! // 发起注册
//! let (reg_id, request) = manager.register(
//!     "sip:alice@example.com",
//!     "sip:alice@192.168.1.1:5060",
//!     None,
//! ).await.unwrap();
//! ```

pub mod auth;
pub mod manager;
pub mod registrar;
pub mod types;

// 重导出核心类型
pub use types::{
    ContactInfo, RegistrationEvent, RegistrationFailureReason, RegistrationId, RegistrationInfo,
    RegistrationState,
};

pub use auth::{AuthHandler, DigestAuthHandler};

pub use manager::RegistrationManager;

pub use registrar::{BindingInfo, MemoryRegistrationStore, Registrar, RegistrationStore};
