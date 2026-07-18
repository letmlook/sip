//! # siprs-dialog
//!
//! SIP 对话层实现，遵循 RFC 3261 Section 12。
//!
//! 按照 RFC 3261 Section 12 实现完整的对话层，包括：
//!
//! - **对话标识** — Call-ID + LocalTag + RemoteTag 三元组
//! - **对话状态** — Early / Confirmed / Terminated
//! - **对话创建** — UAC/UAS 两侧的对话创建规则
//! - **对话维护** — 状态流转、序列号管理、远端目标更新
//! - **路由集** — UAC 侧逆序、UAS 侧正序
//! - **对话内请求** — CSeq 递增、Route 头部、Request-URI
//! - **2xx 重传** — INVITE 2xx 响应重传时重新发送 ACK
//! - **对话终止** — BYE 请求处理
//!
//! # 模块结构
//!
//! - [`types`] — 对话标识、状态、信息和事件类型
//! - [`state`] — 对话创建、维护、路由集和对话内请求构建
//! - [`manager`] — DialogManager 对话管理器
//!
//! # 示例
//!
//! ```ignore
//! use std::sync::Arc;
//! use siprs_dialog::DialogManager;
//! use siprs_core::metrics::SipMetrics;
//!
//! let (dialog_manager, dialog_event_rx) = DialogManager::with_event_channel(
//!     Arc::new(SipMetrics::new())
//! );
//!
//! // 在对话内构建 BYE 请求
//! let bye_request = dialog_manager
//!     .build_in_dialog_request(&dialog_id, Method::Bye)
//!     .await?;
//! ```

pub mod dialog;
pub mod manager;
pub mod session;
pub mod state;
pub mod types;

// 重导出核心类型
pub use types::{DialogEvent, DialogId, DialogInfo, DialogState};

// 重导出状态模块的公共 API
pub use state::{
    build_ack_for_2xx, build_in_dialog_request, build_route_set_uac, build_route_set_uas,
    create_uac_dialog_from_response, create_uas_dialog, is_invite_2xx_retransmit,
    update_dialog_on_response, validate_incoming_cseq, CSeqValidationResult, DialogUpdateResult,
};

// 重导出管理器
pub use manager::DialogManager;
