//! # siprs-registration
//!
//! SIP 注册层实现，支持客户端注册与注册服务器。
//!
//! 提供 SIP 注册功能的完整实现，包括：
//!
//! - **注册状态类型** — `RegistrationId`、`RegistrationState`、`RegistrationInfo` 等
//! - **摘要认证** — RFC 2617 MD5 Digest Authentication，自动处理 401/407 挑战
//! - **注册管理器** — `RegistrationManager` 管理注册生命周期
//! - **注册服务器** — `Registrar` 实现完整注册服务器，支持内存存储和凭据查找
//!
//! # 模块结构
//!
//! - [`types`] — 注册状态类型定义
//! - [`auth`] — 摘要认证处理
//! - [`manager`] — 注册管理器
//! - [`registrar`] — 注册服务器实现
//!
//! # 注册流程
//!
//! 1. 客户端发送 REGISTER（无 Authorization 头部）
//! 2. 服务器返回 401 Unauthorized（含 WWW-Authenticate 头部）
//! 3. 客户端使用凭据计算 MD5 摘要认证响应
//! 4. 客户端重新发送 REGISTER（含 Authorization 头部）
//! 5. 服务器返回 200 OK
//!
//! `RegistrationManager` 内部自动处理步骤 2-4。
//!
//! # 示例
//!
//! ```ignore
//! use siprs_registration::manager::RegistrationManager;
//! use siprs_core::config::RegistrationConfig;
//! use siprs_core::metrics::SipMetrics;
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
