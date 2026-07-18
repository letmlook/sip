//! # siprs-ua
//!
//! SIP 用户代理核心实现，提供 SipEngine、GB28181 设备端与平台端支持。
//!
//! SIP UA 层提供用户代理的核心功能，包括：
//! - 呼出控制（UAC）
//! - 呼入控制（UAS）
//! - 呼叫控制（挂断、取消、会话修改）
//! - 注册管理
//! - 事件通知
//! - GB28181 设备端（`Gb28181Device`）
//! - GB28181 平台端（`Gb28181Server`）
//!
//! # 架构
//!
//! ```text
//! Application Layer
//!       ↕ (SipEvent / Gb28181Event / Gb28181ServerEvent)
//!    SipEngine / Gb28181Device / Gb28181Server
//!       ↕
//! ┌────┼────┬────────┐
//! UAC  UAS  Dialog   Registration
//!       ↕
//! ┌────┼────┬────────┐
//! Transport Transaction Dialog Registration
//! ```
//!
//! # 核心类型
//!
//! - [`SipEngine`] — SIP 协议栈主入口，协调所有下层组件
//! - [`SipEvent`] — 向上层应用通知的事件类型
//! - [`Gb28181Device`] — GB28181 设备端完整实现
//! - [`Gb28181Server`] — GB28181 平台端完整实现
//! - [`DeviceRegistry`] — 设备在线状态管理与设备树
//!
//! # 示例
//!
//! ```ignore
//! use siprs_ua::{SipEngine, SipEvent};
//! use siprs_core::config::SipConfig;
//!
//! let config = SipConfig::builder()
//!     .aor("sip:alice@example.com")
//!     .contact("sip:alice@192.168.1.1:5060")
//!     .build()
//!     .unwrap();
//!
//! let mut engine = SipEngine::new(config);
//! engine.start().await.unwrap();
//!
//! // 获取事件接收器
//! let mut event_rx = engine.event_receiver().unwrap();
//!
//! // 发起呼叫
//! let call_id = engine.make_call("sip:bob@example.com", None, None).await.unwrap();
//! ```

pub mod config;
pub mod device_registry;
pub mod engine;
pub mod event;
pub mod gb28181;
pub mod gb28181_server;
pub mod subscription;
pub mod uac;
pub mod uas;

// 保留原有模块
pub mod agent;
pub mod call;
pub mod profile;

// 重导出核心类型
pub use config::UaConfig;
pub use engine::SipEngine;
pub use event::{CallTerminationReason, SipEvent};

// 重导出订阅相关核心类型
pub use subscription::{
    build_catalog_subscribe, build_subscribe_request, build_subscribe_with_config,
    SubscriptionEvent, SubscriptionInfo, SubscriptionManager, SubscriptionState,
};

// 重导出 GB28181 相关核心类型
pub use gb28181::{
    build_keepalive_xml, build_message_sip_request, Gb28181Config, Gb28181Device, Gb28181Event,
};

// 重导出 GB28181 平台端核心类型
pub use gb28181_server::{
    build_server_message_request, Gb28181Server, Gb28181ServerConfig, Gb28181ServerEvent,
};

// 重导出设备注册表核心类型
pub use device_registry::{
    CascadingDirection, CascadingPlatformInfo, DeviceOnlineStatus, DevicePosition, DeviceRegistry,
    DeviceRegistryEvent, DeviceTree, DeviceTreeNode, RegisteredDevice, SubscriptionStateInfo,
};
