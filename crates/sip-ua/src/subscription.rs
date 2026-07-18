//! SIP SUBSCRIBE/NOTIFY 事件订阅框架
//!
//! 实现 RFC 6665 定义的 SUBSCRIBE/NOTIFY 机制，为 GB28181 等场景提供
//! 事件订阅管理功能。
//!
//! # 核心功能
//!
//! - 发起订阅（SUBSCRIBE 请求构建与发送）
//! - 取消订阅（SUBSCRIBE Expires=0）
//! - 刷新订阅（重新发送 SUBSCRIBE）
//! - 处理 NOTIFY 通知
//! - 订阅过期检测
//! - 订阅事件流
//!
//! # GB28181 场景
//!
//! GB28181 使用 SUBSCRIBE/NOTIFY 机制实现设备目录查询、设备信息查询等
//! 事件通知功能。典型流程：
//!
//! 1. UAC 发送 SUBSCRIBE（Event: Catalog, Accept: Application/MANSCDP+xml）
//! 2. UAS 回复 200 OK
//! 3. UAS 发送 NOTIFY（包含设备目录 XML）
//! 4. UAC 回复 200 OK
//!
//! # 示例
//!
//! ```ignore
//! use sip_ua::subscription::{SubscriptionManager, build_catalog_subscribe};
//!
//! let (manager, event_rx) = SubscriptionManager::new();
//!
//! // 发起目录订阅
//! let subscribe_request = build_catalog_subscribe(
//!     "34020000002000000001",
//!     "192.168.1.1:5060",
//!     3600,
//! );
//! let subscription_id = manager.subscribe(
//!     "34020000002000000001",
//!     "Catalog",
//!     3600,
//!     None,
//! ).await.unwrap();
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use sip_core::config::SipConfig;
use sip_core::{Host, SipVersion};
use sip_message::{
    CSeqHeader, CallId, FromToHeader, HeaderCollection, HeaderName, HeaderValue, Method,
    RequestLine, SipRequest, SipResponse, SipUri, Tag, ViaHeader,
};
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

// ============================================================================
// SubscriptionState - 订阅状态
// ============================================================================

/// 订阅状态
///
/// 对应 RFC 6665 中 Subscription-State 头部的状态值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionState {
    /// 已激活 - 订阅已成功建立并处于活跃状态
    Active,
    /// 等待批准 - 订阅已创建但尚未被授权
    Pending,
    /// 已终止 - 订阅已结束
    Terminated,
}

impl SubscriptionState {
    /// 从 Subscription-State 头部值字符串解析订阅状态
    ///
    /// # 参数
    ///
    /// - `s` - Subscription-State 头部值（如 "active", "pending", "terminated"）
    pub fn from_header_value(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "active" => Self::Active,
            "pending" => Self::Pending,
            "terminated" => Self::Terminated,
            _ => {
                tracing::warn!(
                    "SubscriptionState: unknown subscription-state value '{}', assuming Terminated",
                    s
                );
                Self::Terminated
            }
        }
    }
}

impl std::fmt::Display for SubscriptionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Pending => write!(f, "pending"),
            Self::Terminated => write!(f, "terminated"),
        }
    }
}

// ============================================================================
// SubscriptionInfo - 订阅信息
// ============================================================================

/// 订阅信息
///
/// 记录一次事件订阅的完整状态信息，包括订阅标识、事件类型、
/// 订阅状态、有效期、关联对话等。
#[derive(Debug, Clone)]
pub struct SubscriptionInfo {
    /// 订阅唯一标识
    pub id: String,
    /// Event 头部值，如 "Catalog"、"Alarm"
    pub event: String,
    /// 订阅当前状态
    pub state: SubscriptionState,
    /// 订阅有效期（秒）
    pub expires: u64,
    /// 关联的对话标识（可选）
    pub dialog_id: Option<String>,
    /// 设备标识（GB28181 设备 ID）
    pub device_id: String,
    /// 订阅创建时间
    pub created_at: std::time::Instant,
    /// 最后一次收到 NOTIFY 的时间
    pub last_notify: Option<std::time::Instant>,
}

impl SubscriptionInfo {
    /// 判断订阅是否已过期
    ///
    /// 基于创建时间和有效期计算。
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > Duration::from_secs(self.expires)
    }

    /// 获取订阅剩余有效时间
    ///
    /// 返回剩余秒数，如果已过期返回 0。
    pub fn remaining_secs(&self) -> u64 {
        let elapsed = self.created_at.elapsed().as_secs();
        self.expires.saturating_sub(elapsed)
    }
}

// ============================================================================
// SubscriptionEvent - 订阅事件
// ============================================================================

/// 订阅事件
///
/// 订阅管理器向上层应用通知的事件类型，涵盖订阅生命周期中的
/// 所有关键节点。
#[derive(Debug)]
pub enum SubscriptionEvent {
    /// 订阅已创建
    ///
    /// 当 SUBSCRIBE 请求成功发送后触发。
    SubscriptionCreated {
        /// 订阅标识
        subscription_id: String,
    },

    /// 收到 NOTIFY 通知
    ///
    /// 当收到与订阅匹配的 NOTIFY 请求时触发。
    NotifyReceived {
        /// 订阅标识
        subscription_id: String,
        /// 内容类型（如 "Application/MANSCDP+xml"）
        content_type: String,
        /// 消息体内容
        body: Vec<u8>,
    },

    /// 订阅已终止
    ///
    /// 当收到 Subscription-State: terminated 的 NOTIFY，或主动取消订阅时触发。
    SubscriptionTerminated {
        /// 订阅标识
        subscription_id: String,
        /// 终止原因
        reason: String,
    },

    /// 订阅刷新
    ///
    /// 当重新发送 SUBSCRIBE 刷新订阅时触发。
    SubscriptionRefreshed {
        /// 订阅标识
        subscription_id: String,
    },

    /// 订阅过期
    ///
    /// 当订阅有效期到期时触发。
    SubscriptionExpired {
        /// 订阅标识
        subscription_id: String,
    },
}

// ============================================================================
// SubscriptionManager - 订阅管理器
// ============================================================================

/// SIP 订阅管理器
///
/// 统一管理所有 SUBSCRIBE/NOTIFY 订阅的生命周期，提供：
/// - 订阅创建与查找
/// - 订阅状态更新
/// - NOTIFY 请求处理
/// - 订阅过期检测
/// - 订阅事件分发
///
/// # 线程安全
///
/// `SubscriptionManager` 内部使用 `Arc<Mutex<...>>` 保证线程安全，
/// 可以在多个异步任务之间共享。
///
/// # 示例
///
/// ```ignore
/// use sip_ua::subscription::SubscriptionManager;
///
/// let (manager, event_rx) = SubscriptionManager::new();
///
/// // 发起订阅
/// let sub_id = manager.subscribe("device1", "Catalog", 3600, None).await.unwrap();
///
/// // 处理 NOTIFY
/// manager.handle_notify(&notify_request).await;
///
/// // 检查过期
/// manager.check_expired().await;
/// ```
pub struct SubscriptionManager {
    /// 订阅集合（subscription_id → SubscriptionInfo）
    subscriptions: Arc<Mutex<HashMap<String, SubscriptionInfo>>>,
    /// 订阅事件发送端
    event_tx: mpsc::UnboundedSender<SubscriptionEvent>,
}

impl SubscriptionManager {
    /// 创建新的订阅管理器
    ///
    /// 返回管理器和事件接收端，便于获取订阅事件流。
    pub fn new() -> (Self, mpsc::UnboundedReceiver<SubscriptionEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let manager = Self {
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        };
        (manager, event_rx)
    }

    /// 使用自定义事件通道创建订阅管理器
    ///
    /// # 参数
    ///
    /// - `event_tx` - 订阅事件发送端
    pub fn with_event_channel(event_tx: mpsc::UnboundedSender<SubscriptionEvent>) -> Self {
        Self {
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
    }

    // ========================================================================
    // 订阅操作
    // ========================================================================

    /// 发起订阅
    ///
    /// 创建订阅记录并生成 SUBSCRIBE 请求。调用者需将请求通过传输层发送。
    ///
    /// # 参数
    ///
    /// - `device_id` - 设备标识（GB28181 设备 ID）
    /// - `event` - 事件类型（如 "Catalog"）
    /// - `expires` - 订阅有效期（秒）
    /// - `dialog_id` - 关联的对话标识（可选）
    ///
    /// # 返回
    ///
    /// 返回订阅标识和构建的 SUBSCRIBE 请求。
    pub async fn subscribe(
        &self,
        device_id: &str,
        event: &str,
        expires: u64,
        dialog_id: Option<String>,
    ) -> Result<(String, SipRequest), String> {
        let subscription_id = Self::generate_subscription_id();

        let info = SubscriptionInfo {
            id: subscription_id.clone(),
            event: event.to_string(),
            state: SubscriptionState::Pending,
            expires,
            dialog_id,
            device_id: device_id.to_string(),
            created_at: std::time::Instant::now(),
            last_notify: None,
        };

        // 存储订阅记录
        self.subscriptions
            .lock()
            .await
            .insert(subscription_id.clone(), info);

        // 构建 SUBSCRIBE 请求
        let request = build_subscribe_request(device_id, event, expires);

        // 发送事件
        let _ = self.event_tx.send(SubscriptionEvent::SubscriptionCreated {
            subscription_id: subscription_id.clone(),
        });

        tracing::info!(
            "SubscriptionManager: created subscription {} for event '{}' (device_id={})",
            subscription_id,
            event,
            device_id
        );

        Ok((subscription_id, request))
    }

    /// 取消订阅
    ///
    /// 发送 SUBSCRIBE (Expires=0) 请求以终止订阅。
    ///
    /// # 参数
    ///
    /// - `subscription_id` - 订阅标识
    ///
    /// # 返回
    ///
    /// 返回构建的 SUBSCRIBE (Expires=0) 请求，如果订阅不存在返回 None。
    pub async fn unsubscribe(&self, subscription_id: &str) -> Option<SipRequest> {
        let mut subscriptions = self.subscriptions.lock().await;
        let info = subscriptions.get_mut(subscription_id)?;

        // 更新状态为 Terminated
        info.state = SubscriptionState::Terminated;

        let device_id = info.device_id.clone();
        let event = info.event.clone();

        // 移除订阅记录
        subscriptions.remove(subscription_id);

        // 发送事件
        let _ = self
            .event_tx
            .send(SubscriptionEvent::SubscriptionTerminated {
                subscription_id: subscription_id.to_string(),
                reason: "unsubscribe".to_string(),
            });

        tracing::info!(
            "SubscriptionManager: unsubscribed {} (event='{}', device_id={})",
            subscription_id,
            event,
            device_id
        );

        // 构建 SUBSCRIBE (Expires=0) 请求
        Some(build_subscribe_request(&device_id, &event, 0))
    }

    /// 刷新订阅
    ///
    /// 重新发送 SUBSCRIBE 请求以延长订阅有效期。
    ///
    /// # 参数
    ///
    /// - `subscription_id` - 订阅标识
    /// - `expires` - 新的订阅有效期（秒）
    ///
    /// # 返回
    ///
    /// 返回构建的 SUBSCRIBE 请求，如果订阅不存在返回 None。
    pub async fn refresh(&self, subscription_id: &str, expires: u64) -> Option<SipRequest> {
        let mut subscriptions = self.subscriptions.lock().await;
        let info = subscriptions.get_mut(subscription_id)?;

        // 更新有效期和创建时间
        info.expires = expires;
        info.created_at = std::time::Instant::now();
        info.state = SubscriptionState::Active;

        let device_id = info.device_id.clone();
        let event = info.event.clone();

        // 发送事件
        let _ = self
            .event_tx
            .send(SubscriptionEvent::SubscriptionRefreshed {
                subscription_id: subscription_id.to_string(),
            });

        tracing::info!(
            "SubscriptionManager: refreshed subscription {} (expires={}s)",
            subscription_id,
            expires
        );

        // 构建 SUBSCRIBE 请求
        Some(build_subscribe_request(&device_id, &event, expires))
    }

    // ========================================================================
    // NOTIFY 处理
    // ========================================================================

    /// 处理收到的 NOTIFY 请求
    ///
    /// 解析 Subscription-State 头部、Content-Type 头部和消息体，
    /// 匹配对应的订阅记录并触发相应事件。
    ///
    /// # 参数
    ///
    /// - `notify` - 收到的 NOTIFY 请求
    ///
    /// # 返回
    ///
    /// 返回匹配的订阅标识，如果未找到匹配的订阅返回 None。
    pub async fn handle_notify(&self, notify: &SipRequest) -> Option<String> {
        // 1. 解析 Event 头部
        let event_value = notify
            .headers
            .get(&HeaderName::Extension("Event".to_string()))
            .map(|v| v.to_string())
            .or_else(|| {
                // 尝试从 Raw 头部中查找
                notify
                    .headers
                    .iter()
                    .find(|(name, _)| {
                        matches!(name, HeaderName::Extension(s) if s.eq_ignore_ascii_case("Event"))
                    })
                    .map(|(_, v)| v.to_string())
            });

        // 2. 解析 Subscription-State 头部
        let subscription_state_str = notify
            .headers
            .iter()
            .find(|(name, _)| {
                matches!(name, HeaderName::Extension(s) if s.eq_ignore_ascii_case("Subscription-State"))
            })
            .map(|(_, v)| v.to_string());

        let subscription_state = subscription_state_str
            .as_deref()
            .map(|s| {
                // Subscription-State 头部值可能包含参数，如 "active;expires=3600"
                let state_part = s.split(';').next().unwrap_or(s).trim();
                SubscriptionState::from_header_value(state_part)
            })
            .unwrap_or(SubscriptionState::Active);

        // 3. 解析 Content-Type 头部
        let content_type = notify
            .headers
            .get(&HeaderName::ContentType)
            .and_then(|v| {
                if let HeaderValue::ContentType(ct) = v {
                    Some(ct.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        // 4. 提取消息体
        let body = notify
            .body
            .as_ref()
            .map(|b| b.content.clone())
            .unwrap_or_default();

        // 5. 匹配订阅记录
        let mut subscriptions = self.subscriptions.lock().await;

        // 优先通过 Event 头部匹配
        let matched_subscription = if let Some(ref event) = event_value {
            subscriptions
                .iter_mut()
                .find(|(_, info)| {
                    info.event.eq_ignore_ascii_case(event)
                        && info.state != SubscriptionState::Terminated
                })
                .map(|(id, info)| (id.clone(), info.clone()))
        } else {
            None
        };

        // 如果 Event 头部匹配失败，尝试通过 Call-ID 匹配
        let matched_subscription = matched_subscription.or_else(|| {
            let call_id = notify
                .headers
                .get(&HeaderName::CallId)
                .and_then(|v| v.as_call_id())
                .map(|cid| cid.0.clone());

            if let Some(cid) = call_id {
                subscriptions
                    .iter_mut()
                    .find(|(_, info)| {
                        info.dialog_id
                            .as_ref()
                            .is_some_and(|did| did.contains(&cid))
                            && info.state != SubscriptionState::Terminated
                    })
                    .map(|(id, info)| (id.clone(), info.clone()))
            } else {
                None
            }
        });

        let (subscription_id, _) = matched_subscription?;

        // 6. 更新订阅状态
        if let Some(info) = subscriptions.get_mut(&subscription_id) {
            info.last_notify = Some(std::time::Instant::now());

            match subscription_state {
                SubscriptionState::Active => {
                    info.state = SubscriptionState::Active;
                }
                SubscriptionState::Pending => {
                    info.state = SubscriptionState::Pending;
                }
                SubscriptionState::Terminated => {
                    info.state = SubscriptionState::Terminated;
                    subscriptions.remove(&subscription_id);

                    let _ = self
                        .event_tx
                        .send(SubscriptionEvent::SubscriptionTerminated {
                            subscription_id: subscription_id.clone(),
                            reason: "terminated by NOTIFY".to_string(),
                        });

                    tracing::info!(
                        "SubscriptionManager: subscription {} terminated by NOTIFY",
                        subscription_id
                    );

                    // 如果是终止状态，仍然触发 NotifyReceived 事件（可能包含最终数据）
                    if !body.is_empty() {
                        let _ = self.event_tx.send(SubscriptionEvent::NotifyReceived {
                            subscription_id: subscription_id.clone(),
                            content_type,
                            body,
                        });
                    }

                    return Some(subscription_id);
                }
            }
        }

        // 7. 触发 NotifyReceived 事件
        let _ = self.event_tx.send(SubscriptionEvent::NotifyReceived {
            subscription_id: subscription_id.clone(),
            content_type,
            body,
        });

        tracing::debug!(
            "SubscriptionManager: handled NOTIFY for subscription {} (state={})",
            subscription_id,
            subscription_state
        );

        Some(subscription_id)
    }

    // ========================================================================
    // SUBSCRIBE 响应处理
    // ========================================================================

    /// 处理 SUBSCRIBE 请求的响应
    ///
    /// 根据响应状态码更新订阅状态：
    /// - 2xx → 订阅状态变为 Active
    /// - 423 → 间隔太短，需要调整 Expires
    /// - 其他 4xx-6xx → 订阅失败，终止订阅
    ///
    /// # 参数
    ///
    /// - `subscription_id` - 订阅标识
    /// - `response` - 收到的响应
    pub async fn handle_subscribe_response(&self, subscription_id: &str, response: &SipResponse) {
        let status_code = response.status_line.status_code;
        let mut subscriptions = self.subscriptions.lock().await;

        if let Some(info) = subscriptions.get_mut(subscription_id) {
            if status_code.is_success() {
                // 2xx 成功响应 → 订阅激活
                info.state = SubscriptionState::Active;

                // 从响应中提取 Expires 头部（如果存在）
                if let Some(HeaderValue::Expires(expires)) =
                    response.headers.get(&HeaderName::Expires)
                {
                    info.expires = *expires as u64;
                }

                // 提取 dialog_id
                let call_id = response
                    .headers
                    .get(&HeaderName::CallId)
                    .and_then(|v| v.as_call_id())
                    .map(|cid| cid.0.clone());
                let from_tag = response
                    .headers
                    .get(&HeaderName::From)
                    .and_then(|v| v.as_from_to())
                    .and_then(|ft| ft.tag.as_ref())
                    .map(|t| t.0.clone());
                let to_tag = response
                    .headers
                    .get(&HeaderName::To)
                    .and_then(|v| v.as_from_to())
                    .and_then(|ft| ft.tag.as_ref())
                    .map(|t| t.0.clone());

                if let (Some(cid), Some(ft), Some(tt)) = (call_id, from_tag, to_tag) {
                    info.dialog_id = Some(format!("{}:{}:{}", cid, ft, tt));
                }

                tracing::info!(
                    "SubscriptionManager: subscription {} activated (expires={}s)",
                    subscription_id,
                    info.expires
                );
            } else if status_code.0 == 423 {
                // 423 Interval Too Brief → 需要调整 Expires
                let min_expires = response
                    .headers
                    .get(&HeaderName::MinExpires)
                    .and_then(|v| {
                        if let HeaderValue::Raw(s) = v {
                            s.parse::<u64>().ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(info.expires);

                tracing::warn!(
                    "SubscriptionManager: subscription {} interval too brief, min_expires={}",
                    subscription_id,
                    min_expires
                );

                info.expires = min_expires;
            } else {
                // 其他错误 → 终止订阅
                info.state = SubscriptionState::Terminated;
                subscriptions.remove(subscription_id);

                let _ = self
                    .event_tx
                    .send(SubscriptionEvent::SubscriptionTerminated {
                        subscription_id: subscription_id.to_string(),
                        reason: format!("SUBSCRIBE failed with {}", status_code.0),
                    });

                tracing::warn!(
                    "SubscriptionManager: subscription {} failed with {}",
                    subscription_id,
                    status_code.0
                );
            }
        }
    }

    // ========================================================================
    // 过期检测
    // ========================================================================

    /// 检查过期订阅
    ///
    /// 遍历所有订阅，将已过期的订阅标记为终止并触发
    /// `SubscriptionExpired` 事件。
    ///
    /// 建议在定时任务中周期性调用此方法。
    pub async fn check_expired(&self) {
        let mut subscriptions = self.subscriptions.lock().await;
        let now = std::time::Instant::now();

        let expired_ids: Vec<String> = subscriptions
            .iter()
            .filter(|(_, info)| {
                info.state != SubscriptionState::Terminated
                    && now.duration_since(info.created_at) > Duration::from_secs(info.expires)
            })
            .map(|(id, _)| id.clone())
            .collect();

        for id in expired_ids {
            subscriptions.remove(&id);

            let _ = self.event_tx.send(SubscriptionEvent::SubscriptionExpired {
                subscription_id: id.clone(),
            });

            tracing::info!("SubscriptionManager: subscription {} expired", id);
        }
    }

    // ========================================================================
    // 查询操作
    // ========================================================================

    /// 获取订阅信息
    ///
    /// # 参数
    ///
    /// - `subscription_id` - 订阅标识
    pub async fn get_subscription(&self, subscription_id: &str) -> Option<SubscriptionInfo> {
        self.subscriptions
            .lock()
            .await
            .get(subscription_id)
            .cloned()
    }

    /// 获取所有活跃订阅的标识列表
    pub async fn active_subscription_ids(&self) -> Vec<String> {
        self.subscriptions
            .lock()
            .await
            .iter()
            .filter(|(_, info)| info.state != SubscriptionState::Terminated)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// 获取订阅数量
    pub async fn subscription_count(&self) -> usize {
        self.subscriptions.lock().await.len()
    }

    /// 按 Event 类型查找订阅
    ///
    /// # 参数
    ///
    /// - `event` - 事件类型（如 "Catalog"）
    pub async fn find_by_event(&self, event: &str) -> Vec<SubscriptionInfo> {
        self.subscriptions
            .lock()
            .await
            .iter()
            .filter(|(_, info)| info.event.eq_ignore_ascii_case(event))
            .map(|(_, info)| info.clone())
            .collect()
    }

    /// 按设备 ID 查找订阅
    ///
    /// # 参数
    ///
    /// - `device_id` - 设备标识
    pub async fn find_by_device_id(&self, device_id: &str) -> Vec<SubscriptionInfo> {
        self.subscriptions
            .lock()
            .await
            .iter()
            .filter(|(_, info)| info.device_id == device_id)
            .map(|(_, info)| info.clone())
            .collect()
    }

    // ========================================================================
    // 辅助方法
    // ========================================================================

    /// 生成唯一的订阅标识
    fn generate_subscription_id() -> String {
        format!("sub-{}", Uuid::new_v4().simple())
    }
}

// ============================================================================
// SUBSCRIBE 请求构建
// ============================================================================

/// 构建目录订阅请求
///
/// 为 GB28181 设备目录查询构建 SUBSCRIBE 请求，包含：
/// - Method: SUBSCRIBE
/// - Request-URI: sip:device_id@server
/// - Event: Catalog
/// - Accept: Application/MANSCDP+xml
/// - Expires: 指定值
///
/// # 参数
///
/// - `device_id` - 设备标识（GB28181 设备 ID）
/// - `server_addr` - 服务器地址（如 "192.168.1.1:5060"）
/// - `expires` - 订阅有效期（秒）
///
/// # 返回
///
/// 返回构建的 SUBSCRIBE 请求。
pub fn build_catalog_subscribe(device_id: &str, server_addr: &str, expires: u64) -> SipRequest {
    build_subscribe_request_with_server(device_id, server_addr, "Catalog", expires)
}

/// 构建通用 SUBSCRIBE 请求
///
/// 构建包含 Event 头部和 Expires 头部的 SUBSCRIBE 请求。
/// 适用于 GB28181 和通用 SIP 事件订阅场景。
///
/// # 参数
///
/// - `device_id` - 设备标识
/// - `event` - 事件类型（如 "Catalog"）
/// - `expires` - 订阅有效期（秒）
///
/// # 返回
///
/// 返回构建的 SUBSCRIBE 请求。
pub fn build_subscribe_request(device_id: &str, event: &str, expires: u64) -> SipRequest {
    // 默认使用 device_id 作为服务器地址
    let server_addr = device_id.to_string();
    build_subscribe_request_with_server(device_id, &server_addr, event, expires)
}

/// 构建带服务器地址的 SUBSCRIBE 请求
///
/// # 参数
///
/// - `device_id` - 设备标识
/// - `server_addr` - 服务器地址
/// - `event` - 事件类型
/// - `expires` - 订阅有效期（秒）
fn build_subscribe_request_with_server(
    device_id: &str,
    server_addr: &str,
    event: &str,
    expires: u64,
) -> SipRequest {
    // 构建 Request-URI: sip:device_id@server
    let request_uri = build_device_uri(device_id, server_addr);

    // 构建 From URI
    let from_uri = request_uri.clone();
    let from_tag = Tag::new();

    // 构建 To URI（无 Tag）
    let to_uri = request_uri.clone();

    // 生成 Call-ID
    let call_id = CallId::new();

    // 构建 Via 头部
    let via_host = extract_host(server_addr);
    let via = ViaHeader::new(sip_core::TransportProtocol::Udp, via_host, Some(5060));

    // 构建 From 头部
    let from_header = FromToHeader::new(from_uri).with_tag(from_tag);

    // 构建 To 头部（无 Tag）
    let to_header = FromToHeader::new(to_uri);

    // 构建 CSeq 头部
    let cseq = CSeqHeader::new(1, Method::Subscribe);

    // 组装头部
    let mut headers = HeaderCollection::new();
    headers.insert(HeaderName::Via, HeaderValue::Via(via));
    headers.insert(HeaderName::From, HeaderValue::FromTo(from_header));
    headers.insert(HeaderName::To, HeaderValue::FromTo(to_header));
    headers.insert(HeaderName::CallId, HeaderValue::CallId(call_id));
    headers.insert(HeaderName::CSeq, HeaderValue::CSeq(cseq));
    headers.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

    // Event 头部
    headers.insert(
        HeaderName::Extension("Event".to_string()),
        HeaderValue::Raw(event.to_string()),
    );

    // Accept 头部（GB28181 场景使用 Application/MANSCDP+xml）
    let accept = match event.to_lowercase().as_str() {
        "catalog" | "alarm" | "deviceinfo" | "devicestatus" | "mobileposition" => {
            "Application/MANSCDP+xml"
        }
        _ => "application/json",
    };
    headers.insert(
        HeaderName::Extension("Accept".to_string()),
        HeaderValue::Raw(accept.to_string()),
    );

    // Expires 头部
    headers.insert(HeaderName::Expires, HeaderValue::Expires(expires as u32));

    SipRequest {
        request_line: RequestLine {
            method: Method::Subscribe,
            request_uri,
            version: SipVersion,
        },
        headers,
        body: None,
    }
}

/// 使用 SipConfig 构建 SUBSCRIBE 请求
///
/// 使用完整的 SIP 配置构建请求，包含正确的 From、Contact、Via 头部。
///
/// # 参数
///
/// - `config` - SIP 配置
/// - `device_id` - 设备标识
/// - `event` - 事件类型
/// - `expires` - 订阅有效期（秒）
///
/// # 返回
///
/// 返回构建的 SUBSCRIBE 请求和生成的 Call-ID。
pub fn build_subscribe_with_config(
    config: &SipConfig,
    device_id: &str,
    event: &str,
    expires: u64,
) -> Result<(SipRequest, CallId), String> {
    // 构建 Request-URI
    let request_uri = build_device_uri(device_id, &config.aor);

    // 解析 From URI
    let from_uri = SipUri::parse(&config.aor).map_err(|e| format!("invalid AOR URI: {}", e))?;

    // 解析 To URI
    let to_uri = request_uri.clone();

    // 解析 Contact URI
    let contact_uri =
        SipUri::parse(&config.contact).map_err(|e| format!("invalid contact URI: {}", e))?;

    // 生成 Call-ID 和 From Tag
    let call_id = CallId::new();
    let from_tag = Tag::new();

    // 构建 Via 头部
    let via_host = extract_host_from_contact(&config.contact);
    let via = ViaHeader::new(config.transport, via_host, Some(config.sip_port));

    // 构建 From 头部
    let from_header = FromToHeader::new(from_uri).with_tag(from_tag);

    // 构建 To 头部（无 Tag）
    let to_header = FromToHeader::new(to_uri);

    // 构建 Contact 头部
    let contact_header = sip_message::ContactHeader::new(contact_uri);

    // 构建 CSeq 头部
    let cseq = CSeqHeader::new(1, Method::Subscribe);

    // 组装头部
    let mut headers = HeaderCollection::new();
    headers.insert(HeaderName::Via, HeaderValue::Via(via));
    headers.insert(HeaderName::From, HeaderValue::FromTo(from_header));
    headers.insert(HeaderName::To, HeaderValue::FromTo(to_header));
    headers.insert(HeaderName::CallId, HeaderValue::CallId(call_id.clone()));
    headers.insert(HeaderName::CSeq, HeaderValue::CSeq(cseq));
    headers.insert(HeaderName::Contact, HeaderValue::Contact(contact_header));
    headers.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

    // Event 头部
    headers.insert(
        HeaderName::Extension("Event".to_string()),
        HeaderValue::Raw(event.to_string()),
    );

    // Accept 头部
    let accept = match event.to_lowercase().as_str() {
        "catalog" | "alarm" | "deviceinfo" | "devicestatus" | "mobileposition" => {
            "Application/MANSCDP+xml"
        }
        _ => "application/json",
    };
    headers.insert(
        HeaderName::Extension("Accept".to_string()),
        HeaderValue::Raw(accept.to_string()),
    );

    // Expires 头部
    headers.insert(HeaderName::Expires, HeaderValue::Expires(expires as u32));

    let request = SipRequest {
        request_line: RequestLine {
            method: Method::Subscribe,
            request_uri,
            version: SipVersion,
        },
        headers,
        body: None,
    };

    Ok((request, call_id))
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 构建设备 SIP URI
///
/// 格式：`sip:device_id@server`
fn build_device_uri(device_id: &str, server_addr: &str) -> SipUri {
    // 尝试解析 server_addr，如果包含端口则保留
    let host: Host = server_addr
        .parse()
        .unwrap_or_else(|_| Host::Domain(server_addr.to_string()));

    SipUri {
        scheme: sip_message::UriScheme::Sip,
        user_info: Some(sip_message::UserInfo {
            user: device_id.to_string(),
            password: None,
        }),
        host,
        port: None,
        params: sip_message::UriParams::new(),
        headers: sip_message::UriHeaders::new(),
    }
}

/// 从服务器地址提取 Host
fn extract_host(server_addr: &str) -> Host {
    // 移除端口部分
    let host_str = if server_addr.starts_with('[') {
        // IPv6 地址
        server_addr.to_string()
    } else if let Some(colon_pos) = server_addr.rfind(':') {
        server_addr[..colon_pos].to_string()
    } else {
        server_addr.to_string()
    };

    host_str
        .parse()
        .unwrap_or_else(|_| Host::Domain(server_addr.to_string()))
}

/// 从 Contact URI 字符串提取 Host 用于 Via 头部
fn extract_host_from_contact(contact: &str) -> Host {
    SipUri::parse(contact)
        .map(|uri| uri.host.clone())
        .unwrap_or_else(|_| Host::Domain("localhost".to_string()))
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use sip_message::Body;

    // ---- SubscriptionState 测试 ----

    #[test]
    fn test_subscription_state_from_header_value() {
        assert_eq!(
            SubscriptionState::from_header_value("active"),
            SubscriptionState::Active
        );
        assert_eq!(
            SubscriptionState::from_header_value("pending"),
            SubscriptionState::Pending
        );
        assert_eq!(
            SubscriptionState::from_header_value("terminated"),
            SubscriptionState::Terminated
        );
        // 大小写不敏感
        assert_eq!(
            SubscriptionState::from_header_value("Active"),
            SubscriptionState::Active
        );
        assert_eq!(
            SubscriptionState::from_header_value("TERMINATED"),
            SubscriptionState::Terminated
        );
        // 未知值默认为 Terminated
        assert_eq!(
            SubscriptionState::from_header_value("unknown"),
            SubscriptionState::Terminated
        );
    }

    #[test]
    fn test_subscription_state_display() {
        assert_eq!(SubscriptionState::Active.to_string(), "active");
        assert_eq!(SubscriptionState::Pending.to_string(), "pending");
        assert_eq!(SubscriptionState::Terminated.to_string(), "terminated");
    }

    // ---- SubscriptionInfo 测试 ----

    #[test]
    fn test_subscription_info_is_expired() {
        let info = SubscriptionInfo {
            id: "test-sub".to_string(),
            event: "Catalog".to_string(),
            state: SubscriptionState::Active,
            expires: 3600,
            dialog_id: None,
            device_id: "34020000002000000001".to_string(),
            created_at: std::time::Instant::now(),
            last_notify: None,
        };
        // 刚创建的订阅不应过期
        assert!(!info.is_expired());

        // 创建一个已过期的订阅
        let expired_info = SubscriptionInfo {
            id: "test-sub-expired".to_string(),
            event: "Catalog".to_string(),
            state: SubscriptionState::Active,
            expires: 0, // 0 秒有效期
            dialog_id: None,
            device_id: "34020000002000000001".to_string(),
            created_at: std::time::Instant::now() - Duration::from_secs(1),
            last_notify: None,
        };
        assert!(expired_info.is_expired());
    }

    #[test]
    fn test_subscription_info_remaining_secs() {
        let info = SubscriptionInfo {
            id: "test-sub".to_string(),
            event: "Catalog".to_string(),
            state: SubscriptionState::Active,
            expires: 3600,
            dialog_id: None,
            device_id: "34020000002000000001".to_string(),
            created_at: std::time::Instant::now(),
            last_notify: None,
        };
        // 刚创建的订阅应剩余接近 3600 秒
        let remaining = info.remaining_secs();
        assert!(remaining <= 3600);
        assert!(remaining > 3590);
    }

    // ---- SubscriptionManager 测试 ----

    #[tokio::test]
    async fn test_subscribe() {
        let (manager, _event_rx) = SubscriptionManager::new();

        let (sub_id, request) = manager
            .subscribe("34020000002000000001", "Catalog", 3600, None)
            .await
            .unwrap();

        // 验证订阅 ID 非空
        assert!(!sub_id.is_empty());
        assert!(sub_id.starts_with("sub-"));

        // 验证请求方法
        assert_eq!(request.request_line.method, Method::Subscribe);

        // 验证 Event 头部
        let event_header = request
            .headers
            .iter()
            .find(|(name, _)| {
                matches!(name, HeaderName::Extension(s) if s.eq_ignore_ascii_case("Event"))
            })
            .map(|(_, v)| v.to_string());
        assert_eq!(event_header.as_deref(), Some("Catalog"));

        // 验证 Accept 头部
        let accept_header = request
            .headers
            .iter()
            .find(|(name, _)| {
                matches!(name, HeaderName::Extension(s) if s.eq_ignore_ascii_case("Accept"))
            })
            .map(|(_, v)| v.to_string());
        assert_eq!(accept_header.as_deref(), Some("Application/MANSCDP+xml"));

        // 验证 Expires 头部
        let expires = request.headers.get(&HeaderName::Expires).and_then(|v| {
            if let HeaderValue::Expires(e) = v {
                Some(*e)
            } else {
                None
            }
        });
        assert_eq!(expires, Some(3600));

        // 验证订阅记录已创建
        let info = manager.get_subscription(&sub_id).await.unwrap();
        assert_eq!(info.event, "Catalog");
        assert_eq!(info.state, SubscriptionState::Pending);
        assert_eq!(info.expires, 3600);
        assert_eq!(info.device_id, "34020000002000000001");
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let (manager, _event_rx) = SubscriptionManager::new();

        let (sub_id, _) = manager
            .subscribe("34020000002000000001", "Catalog", 3600, None)
            .await
            .unwrap();

        // 取消订阅
        let unsubscribe_request = manager.unsubscribe(&sub_id).await;
        assert!(unsubscribe_request.is_some());

        let request = unsubscribe_request.unwrap();
        assert_eq!(request.request_line.method, Method::Subscribe);

        // 验证 Expires=0
        let expires = request.headers.get(&HeaderName::Expires).and_then(|v| {
            if let HeaderValue::Expires(e) = v {
                Some(*e)
            } else {
                None
            }
        });
        assert_eq!(expires, Some(0));

        // 验证订阅记录已删除
        let info = manager.get_subscription(&sub_id).await;
        assert!(info.is_none());
    }

    #[tokio::test]
    async fn test_unsubscribe_nonexistent() {
        let (manager, _event_rx) = SubscriptionManager::new();

        let result = manager.unsubscribe("nonexistent").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_refresh() {
        let (manager, _event_rx) = SubscriptionManager::new();

        let (sub_id, _) = manager
            .subscribe("34020000002000000001", "Catalog", 3600, None)
            .await
            .unwrap();

        // 刷新订阅
        let refresh_request = manager.refresh(&sub_id, 7200).await;
        assert!(refresh_request.is_some());

        let request = refresh_request.unwrap();
        assert_eq!(request.request_line.method, Method::Subscribe);

        // 验证新的 Expires 值
        let expires = request.headers.get(&HeaderName::Expires).and_then(|v| {
            if let HeaderValue::Expires(e) = v {
                Some(*e)
            } else {
                None
            }
        });
        assert_eq!(expires, Some(7200));

        // 验证订阅信息已更新
        let info = manager.get_subscription(&sub_id).await.unwrap();
        assert_eq!(info.expires, 7200);
        assert_eq!(info.state, SubscriptionState::Active);
    }

    #[tokio::test]
    async fn test_refresh_nonexistent() {
        let (manager, _event_rx) = SubscriptionManager::new();

        let result = manager.refresh("nonexistent", 7200).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_handle_subscribe_response_200() {
        let (manager, _event_rx) = SubscriptionManager::new();

        let (sub_id, subscribe_request) = manager
            .subscribe("34020000002000000001", "Catalog", 3600, None)
            .await
            .unwrap();

        // 构建 200 OK 响应
        let response = build_200_ok_response(&subscribe_request, 3600);

        // 处理响应
        manager.handle_subscribe_response(&sub_id, &response).await;

        // 验证订阅状态变为 Active
        let info = manager.get_subscription(&sub_id).await.unwrap();
        assert_eq!(info.state, SubscriptionState::Active);
    }

    #[tokio::test]
    async fn test_handle_subscribe_response_423() {
        let (manager, _event_rx) = SubscriptionManager::new();

        let (sub_id, subscribe_request) = manager
            .subscribe("34020000002000000001", "Catalog", 60, None)
            .await
            .unwrap();

        // 构建 423 响应
        let mut response = sip_message::SipResponse {
            status_line: sip_message::StatusLine {
                version: SipVersion,
                status_code: sip_core::StatusCode(423),
                reason_phrase: "Interval Too Brief".to_string(),
            },
            headers: HeaderCollection::new(),
            body: None,
        };

        // 复制必要头部
        if let Some(via) = subscribe_request.headers.get(&HeaderName::Via) {
            response.headers.insert(HeaderName::Via, via.clone());
        }
        if let Some(from) = subscribe_request.headers.get(&HeaderName::From) {
            response.headers.insert(HeaderName::From, from.clone());
        }
        if let Some(to) = subscribe_request.headers.get(&HeaderName::To) {
            response.headers.insert(HeaderName::To, to.clone());
        }
        if let Some(call_id) = subscribe_request.headers.get(&HeaderName::CallId) {
            response.headers.insert(HeaderName::CallId, call_id.clone());
        }
        if let Some(cseq) = subscribe_request.headers.get(&HeaderName::CSeq) {
            response.headers.insert(HeaderName::CSeq, cseq.clone());
        }

        // Min-Expires 头部
        response
            .headers
            .insert(HeaderName::MinExpires, HeaderValue::Raw("3600".to_string()));

        // 处理响应
        manager.handle_subscribe_response(&sub_id, &response).await;

        // 验证 Expires 已更新
        let info = manager.get_subscription(&sub_id).await.unwrap();
        assert_eq!(info.expires, 3600);
    }

    #[tokio::test]
    async fn test_handle_subscribe_response_error() {
        let (manager, _event_rx) = SubscriptionManager::new();

        let (sub_id, _subscribe_request) = manager
            .subscribe("34020000002000000001", "Catalog", 3600, None)
            .await
            .unwrap();

        // 构建 404 响应
        let response = sip_message::SipResponse {
            status_line: sip_message::StatusLine {
                version: SipVersion,
                status_code: sip_core::StatusCode(404),
                reason_phrase: "Not Found".to_string(),
            },
            headers: HeaderCollection::new(),
            body: None,
        };

        // 处理响应
        manager.handle_subscribe_response(&sub_id, &response).await;

        // 验证订阅已终止
        let info = manager.get_subscription(&sub_id).await;
        assert!(info.is_none());
    }

    #[tokio::test]
    async fn test_handle_notify() {
        let (manager, _event_rx) = SubscriptionManager::new();

        let (sub_id, _) = manager
            .subscribe("34020000002000000001", "Catalog", 3600, None)
            .await
            .unwrap();

        // 先将订阅设为 Active
        {
            let mut subscriptions = manager.subscriptions.lock().await;
            if let Some(info) = subscriptions.get_mut(&sub_id) {
                info.state = SubscriptionState::Active;
            }
        }

        // 构建 NOTIFY 请求
        let notify = build_notify_request(
            "Catalog",
            "active",
            "Application/MANSCDP+xml",
            b"<Response/>".to_vec(),
        );

        // 处理 NOTIFY
        let matched_id = manager.handle_notify(&notify).await;
        assert!(matched_id.is_some());
        assert_eq!(matched_id.unwrap(), sub_id);

        // 验证 last_notify 已更新
        let info = manager.get_subscription(&sub_id).await.unwrap();
        assert!(info.last_notify.is_some());
    }

    #[tokio::test]
    async fn test_handle_notify_terminated() {
        let (manager, _event_rx) = SubscriptionManager::new();

        let (sub_id, _) = manager
            .subscribe("34020000002000000001", "Catalog", 3600, None)
            .await
            .unwrap();

        // 先将订阅设为 Active
        {
            let mut subscriptions = manager.subscriptions.lock().await;
            if let Some(info) = subscriptions.get_mut(&sub_id) {
                info.state = SubscriptionState::Active;
            }
        }

        // 构建 Subscription-State: terminated 的 NOTIFY
        let notify = build_notify_request(
            "Catalog",
            "terminated",
            "Application/MANSCDP+xml",
            Vec::new(),
        );

        // 处理 NOTIFY
        let matched_id = manager.handle_notify(&notify).await;
        assert!(matched_id.is_some());

        // 验证订阅已终止
        let info = manager.get_subscription(&sub_id).await;
        assert!(info.is_none());
    }

    #[tokio::test]
    async fn test_check_expired() {
        let (manager, _event_rx) = SubscriptionManager::new();

        // 创建一个已过期的订阅
        let sub_id = {
            let mut subscriptions = manager.subscriptions.lock().await;
            let id = format!("sub-expired-test");
            let info = SubscriptionInfo {
                id: id.clone(),
                event: "Catalog".to_string(),
                state: SubscriptionState::Active,
                expires: 1, // 1 秒有效期
                dialog_id: None,
                device_id: "34020000002000000001".to_string(),
                created_at: std::time::Instant::now() - Duration::from_secs(5),
                last_notify: None,
            };
            subscriptions.insert(id.clone(), info);
            id
        };

        // 检查过期
        manager.check_expired().await;

        // 验证订阅已移除
        let info = manager.get_subscription(&sub_id).await;
        assert!(info.is_none());
    }

    #[tokio::test]
    async fn test_check_expired_not_yet() {
        let (manager, _event_rx) = SubscriptionManager::new();

        let (sub_id, _) = manager
            .subscribe("34020000002000000001", "Catalog", 3600, None)
            .await
            .unwrap();

        // 检查过期（不应有过期订阅）
        manager.check_expired().await;

        // 验证订阅仍存在
        let info = manager.get_subscription(&sub_id).await;
        assert!(info.is_some());
    }

    #[tokio::test]
    async fn test_subscription_events() {
        let (manager, mut event_rx) = SubscriptionManager::new();

        // 发起订阅
        let (sub_id, _) = manager
            .subscribe("34020000002000000001", "Catalog", 3600, None)
            .await
            .unwrap();

        // 验证 SubscriptionCreated 事件
        let event = event_rx.try_recv().unwrap();
        match event {
            SubscriptionEvent::SubscriptionCreated { subscription_id } => {
                assert_eq!(subscription_id, sub_id);
            }
            _ => panic!("Expected SubscriptionCreated event"),
        }

        // 取消订阅
        let _ = manager.unsubscribe(&sub_id).await;

        // 验证 SubscriptionTerminated 事件
        let event = event_rx.try_recv().unwrap();
        match event {
            SubscriptionEvent::SubscriptionTerminated {
                subscription_id,
                reason,
            } => {
                assert_eq!(subscription_id, sub_id);
                assert_eq!(reason, "unsubscribe");
            }
            _ => panic!("Expected SubscriptionTerminated event"),
        }
    }

    #[tokio::test]
    async fn test_find_by_event() {
        let (manager, _event_rx) = SubscriptionManager::new();

        let (_sub_id1, _) = manager
            .subscribe("device1", "Catalog", 3600, None)
            .await
            .unwrap();
        let (sub_id2, _) = manager
            .subscribe("device2", "Alarm", 3600, None)
            .await
            .unwrap();
        let (_sub_id3, _) = manager
            .subscribe("device3", "Catalog", 3600, None)
            .await
            .unwrap();

        // 按 Catalog 查找
        let catalog_subs = manager.find_by_event("Catalog").await;
        assert_eq!(catalog_subs.len(), 2);

        // 按 Alarm 查找
        let alarm_subs = manager.find_by_event("Alarm").await;
        assert_eq!(alarm_subs.len(), 1);
        assert_eq!(alarm_subs[0].id, sub_id2);
    }

    #[tokio::test]
    async fn test_find_by_device_id() {
        let (manager, _event_rx) = SubscriptionManager::new();

        let (sub_id1, _) = manager
            .subscribe("device1", "Catalog", 3600, None)
            .await
            .unwrap();
        let (sub_id2, _) = manager
            .subscribe("device2", "Catalog", 3600, None)
            .await
            .unwrap();

        // 按 device1 查找
        let device1_subs = manager.find_by_device_id("device1").await;
        assert_eq!(device1_subs.len(), 1);
        assert_eq!(device1_subs[0].id, sub_id1);

        // 按 device2 查找
        let device2_subs = manager.find_by_device_id("device2").await;
        assert_eq!(device2_subs.len(), 1);
        assert_eq!(device2_subs[0].id, sub_id2);
    }

    #[tokio::test]
    async fn test_active_subscription_ids() {
        let (manager, _event_rx) = SubscriptionManager::new();

        let (sub_id1, _) = manager
            .subscribe("device1", "Catalog", 3600, None)
            .await
            .unwrap();
        let (sub_id2, _) = manager
            .subscribe("device2", "Alarm", 3600, None)
            .await
            .unwrap();

        let active_ids = manager.active_subscription_ids().await;
        assert_eq!(active_ids.len(), 2);

        // 取消一个订阅
        let _ = manager.unsubscribe(&sub_id1).await;

        let active_ids = manager.active_subscription_ids().await;
        assert_eq!(active_ids.len(), 1);
        assert_eq!(active_ids[0], sub_id2);
    }

    #[tokio::test]
    async fn test_subscription_count() {
        let (manager, _event_rx) = SubscriptionManager::new();

        assert_eq!(manager.subscription_count().await, 0);

        let (sub_id1, _) = manager
            .subscribe("device1", "Catalog", 3600, None)
            .await
            .unwrap();
        assert_eq!(manager.subscription_count().await, 1);

        let _ = manager.unsubscribe(&sub_id1).await;
        assert_eq!(manager.subscription_count().await, 0);
    }

    // ---- SUBSCRIBE 请求构建测试 ----

    #[test]
    fn test_build_catalog_subscribe() {
        let request = build_catalog_subscribe("34020000002000000001", "192.168.1.1:5060", 3600);

        // 验证方法
        assert_eq!(request.request_line.method, Method::Subscribe);

        // 验证 Request-URI
        let uri = &request.request_line.request_uri;
        assert_eq!(uri.scheme, sip_message::UriScheme::Sip);
        assert_eq!(uri.user_info.as_ref().unwrap().user, "34020000002000000001");

        // 验证 Event 头部
        let event_header = request
            .headers
            .iter()
            .find(|(name, _)| {
                matches!(name, HeaderName::Extension(s) if s.eq_ignore_ascii_case("Event"))
            })
            .map(|(_, v)| v.to_string());
        assert_eq!(event_header.as_deref(), Some("Catalog"));

        // 验证 Accept 头部
        let accept_header = request
            .headers
            .iter()
            .find(|(name, _)| {
                matches!(name, HeaderName::Extension(s) if s.eq_ignore_ascii_case("Accept"))
            })
            .map(|(_, v)| v.to_string());
        assert_eq!(accept_header.as_deref(), Some("Application/MANSCDP+xml"));

        // 验证 Expires 头部
        let expires = request.headers.get(&HeaderName::Expires).and_then(|v| {
            if let HeaderValue::Expires(e) = v {
                Some(*e)
            } else {
                None
            }
        });
        assert_eq!(expires, Some(3600));

        // 验证必要头部
        assert!(request.headers.get(&HeaderName::Via).is_some());
        assert!(request.headers.get(&HeaderName::From).is_some());
        assert!(request.headers.get(&HeaderName::To).is_some());
        assert!(request.headers.get(&HeaderName::CallId).is_some());
        assert!(request.headers.get(&HeaderName::CSeq).is_some());
        assert!(request.headers.get(&HeaderName::MaxForwards).is_some());
    }

    #[test]
    fn test_build_subscribe_request_alarm() {
        let request = build_subscribe_request("34020000002000000001", "Alarm", 3600);

        // 验证 Event 头部
        let event_header = request
            .headers
            .iter()
            .find(|(name, _)| {
                matches!(name, HeaderName::Extension(s) if s.eq_ignore_ascii_case("Event"))
            })
            .map(|(_, v)| v.to_string());
        assert_eq!(event_header.as_deref(), Some("Alarm"));

        // 验证 Accept 头部（Alarm 也使用 Application/MANSCDP+xml）
        let accept_header = request
            .headers
            .iter()
            .find(|(name, _)| {
                matches!(name, HeaderName::Extension(s) if s.eq_ignore_ascii_case("Accept"))
            })
            .map(|(_, v)| v.to_string());
        assert_eq!(accept_header.as_deref(), Some("Application/MANSCDP+xml"));
    }

    #[test]
    fn test_build_subscribe_with_config() {
        let config = SipConfig::builder()
            .aor("sip:34020000002000000001@192.168.1.1")
            .contact("sip:34020000002000000001@192.168.1.1:5060")
            .sip_port(5060)
            .build()
            .unwrap();

        let (request, call_id) =
            build_subscribe_with_config(&config, "34020000002000000001", "Catalog", 3600).unwrap();

        // 验证方法
        assert_eq!(request.request_line.method, Method::Subscribe);

        // 验证 Call-ID 非空
        assert!(!call_id.0.is_empty());

        // 验证 Event 头部
        let event_header = request
            .headers
            .iter()
            .find(|(name, _)| {
                matches!(name, HeaderName::Extension(s) if s.eq_ignore_ascii_case("Event"))
            })
            .map(|(_, v)| v.to_string());
        assert_eq!(event_header.as_deref(), Some("Catalog"));

        // 验证 Accept 头部
        let accept_header = request
            .headers
            .iter()
            .find(|(name, _)| {
                matches!(name, HeaderName::Extension(s) if s.eq_ignore_ascii_case("Accept"))
            })
            .map(|(_, v)| v.to_string());
        assert_eq!(accept_header.as_deref(), Some("Application/MANSCDP+xml"));

        // 验证 Expires 头部
        let expires = request.headers.get(&HeaderName::Expires).and_then(|v| {
            if let HeaderValue::Expires(e) = v {
                Some(*e)
            } else {
                None
            }
        });
        assert_eq!(expires, Some(3600));

        // 验证 Contact 头部
        assert!(request.headers.get(&HeaderName::Contact).is_some());
    }

    // ---- 辅助函数测试 ----

    #[test]
    fn test_build_device_uri() {
        let uri = build_device_uri("34020000002000000001", "192.168.1.1");
        assert_eq!(uri.scheme, sip_message::UriScheme::Sip);
        assert_eq!(uri.user_info.as_ref().unwrap().user, "34020000002000000001");
        assert_eq!(uri.host, Host::IPv4("192.168.1.1".parse().unwrap()));
    }

    #[test]
    fn test_extract_host() {
        let host = extract_host("192.168.1.1:5060");
        assert_eq!(host, Host::IPv4("192.168.1.1".parse().unwrap()));

        let host_no_port = extract_host("example.com");
        assert_eq!(host_no_port, Host::Domain("example.com".to_string()));
    }

    // ---- 测试辅助函数 ----

    /// 构建 200 OK 响应
    fn build_200_ok_response(request: &SipRequest, expires: u32) -> SipResponse {
        let mut headers = HeaderCollection::new();

        // 复制 Via
        if let Some(via) = request.headers.get(&HeaderName::Via) {
            headers.insert(HeaderName::Via, via.clone());
        }

        // 复制 From
        if let Some(from) = request.headers.get(&HeaderName::From) {
            headers.insert(HeaderName::From, from.clone());
        }

        // To 头部添加 Tag
        if let Some(to_value) = request.headers.get(&HeaderName::To) {
            if let Some(from_to) = to_value.as_from_to() {
                let mut from_to = from_to.clone();
                from_to = from_to.with_tag(Tag::new());
                headers.insert(HeaderName::To, HeaderValue::FromTo(from_to));
            } else {
                headers.insert(HeaderName::To, to_value.clone());
            }
        }

        // 复制 Call-ID
        if let Some(call_id) = request.headers.get(&HeaderName::CallId) {
            headers.insert(HeaderName::CallId, call_id.clone());
        }

        // 复制 CSeq
        if let Some(cseq) = request.headers.get(&HeaderName::CSeq) {
            headers.insert(HeaderName::CSeq, cseq.clone());
        }

        // Expires 头部
        headers.insert(HeaderName::Expires, HeaderValue::Expires(expires));

        SipResponse {
            status_line: sip_message::StatusLine {
                version: SipVersion,
                status_code: sip_core::StatusCode(200),
                reason_phrase: "OK".to_string(),
            },
            headers,
            body: None,
        }
    }

    /// 构建 NOTIFY 请求
    fn build_notify_request(
        event: &str,
        subscription_state: &str,
        content_type: &str,
        body: Vec<u8>,
    ) -> SipRequest {
        let request_uri = build_device_uri("34020000002000000001", "192.168.1.1");
        let from_uri = request_uri.clone();
        let to_uri = request_uri.clone();

        let call_id = CallId::new();
        let from_tag = Tag::new();

        let via = ViaHeader::new(
            sip_core::TransportProtocol::Udp,
            Host::Domain("192.168.1.1".to_string()),
            Some(5060),
        );

        let from_header = FromToHeader::new(from_uri).with_tag(from_tag);
        let to_header = FromToHeader::new(to_uri);
        let cseq = CSeqHeader::new(1, Method::Notify);

        let mut headers = HeaderCollection::new();
        headers.insert(HeaderName::Via, HeaderValue::Via(via));
        headers.insert(HeaderName::From, HeaderValue::FromTo(from_header));
        headers.insert(HeaderName::To, HeaderValue::FromTo(to_header));
        headers.insert(HeaderName::CallId, HeaderValue::CallId(call_id));
        headers.insert(HeaderName::CSeq, HeaderValue::CSeq(cseq));
        headers.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

        // Event 头部
        headers.insert(
            HeaderName::Extension("Event".to_string()),
            HeaderValue::Raw(event.to_string()),
        );

        // Subscription-State 头部
        headers.insert(
            HeaderName::Extension("Subscription-State".to_string()),
            HeaderValue::Raw(subscription_state.to_string()),
        );

        // Content-Type 和 Body
        let sip_body = if !body.is_empty() {
            let b = Body::new(content_type, body);
            headers.insert(
                HeaderName::ContentType,
                HeaderValue::ContentType(b.content_type.clone()),
            );
            headers.insert(
                HeaderName::ContentLength,
                HeaderValue::ContentLength(b.len()),
            );
            Some(b)
        } else {
            None
        };

        SipRequest {
            request_line: RequestLine {
                method: Method::Notify,
                request_uri,
                version: SipVersion,
            },
            headers,
            body: sip_body,
        }
    }
}
