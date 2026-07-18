//! SIP Transaction - SIP 协议栈事务层实现
//!
//! 按照 RFC 3261 Section 17 实现完整的事务层，包括：
//!
//! - **INVITE 客户端事务** - RFC 3261 Section 17.1.1
//! - **非 INVITE 客户端事务** - RFC 3261 Section 17.1.2
//! - **INVITE 服务端事务** - RFC 3261 Section 17.2.1
//! - **非 INVITE 服务端事务** - RFC 3261 Section 17.2.2
//! - **定时器管理** - Timer A~K
//! - **事务匹配** - 基于 Branch ID + Method
//! - **ACK 处理** - 事务层内 ACK 和 TU 层 ACK
//!
//! # 模块结构
//!
//! - [`event`] - 事务事件、定时器事件、状态枚举
//! - [`timer`] - 定时器管理器
//! - [`invite_client`] - INVITE 客户端事务状态机
//! - [`non_invite_client`] - 非 INVITE 客户端事务状态机
//! - [`invite_server`] - INVITE 服务端事务状态机
//! - [`non_invite_server`] - 非 INVITE 服务端事务状态机
//! - [`table`] - 事务匹配与 ACK 处理
//! - [`manager`] - 事务管理器

pub mod event;
pub mod invite_client;
pub mod invite_server;
pub mod manager;
pub mod non_invite_client;
pub mod non_invite_server;
pub mod table;
pub mod timer;

// 重导出核心类型
pub use event::{
    InviteClientState, InviteServerState, NonInviteClientState, NonInviteServerState,
    TerminationReason, TimerEvent, TransactionAction, TransactionEvent, TransactionId,
    TransactionKey,
};

pub use manager::TransactionManager;

pub use table::{ClientTransaction, ServerTransaction, TransactionTable};

pub use timer::TimerManager;
