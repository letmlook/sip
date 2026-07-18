//! GB28181 平台端（WVP 信令服务器核心）
//!
//! 实现 GB/T 28181 国标平台端的完整流程，包括：
//! - 接收设备 REGISTER（通过 Registrar 验证认证）
//! - 接收设备心跳 MESSAGE Keepalive
//! - 主动查询设备目录/设备信息/录像/设备状态
//! - 主动发起视频点播 INVITE（实时/回放/下载）
//! - 主动发送设备控制 MESSAGE（云台控制/远程启动/录像拖动）
//! - 主动订阅设备目录/报警/移动位置
//! - 接收设备报警/移动位置通知
//!
//! # 架构
//!
//! ```text
//! Application Layer
//!       ↕ (Gb28181ServerEvent)
//! Gb28181Server
//!       ↕
//! ┌────┼────────┬──────────────┐
//! SipEngine  Registrar  DeviceRegistry
//!       ↕           ↕
//! SubscriptionManager  MESSAGE/INVITE/BYE
//! ```
//!
//! # 与 Gb28181Device 的关系
//!
//! - **Gb28181Device** = 设备端，主动注册到平台
//! - **Gb28181Server** = 平台端，接收设备注册，主动查询/控制设备
//!
//! # 示例
//!
//! ```ignore
//! use sip_ua::gb28181_server::{Gb28181ServerConfig, Gb28181Server};
//!
//! let config = Gb28181ServerConfig {
//!     server_id: "34020000002000000001".to_string(),
//!     server_domain: "3402000000".to_string(),
//!     sip_ip: "192.168.1.1".to_string(),
//!     sip_port: 5060,
//!     realm: "3402000000".to_string(),
//!     auth_password: "12345678".to_string(),
//!     heartbeat_timeout: 180,
//!     register_expires: 3600,
//! };
//!
//! let mut server = Gb28181Server::new(config);
//! server.start().await.unwrap();
//!
//! // 获取事件接收器
//! let mut event_rx = server.event_receiver().unwrap();
//!
//! // 处理事件
//! while let Some(event) = event_rx.recv().await {
//!     match event {
//!         Gb28181ServerEvent::DeviceRegistered { device_id } => {
//!             println!("设备注册: {}", device_id);
//!             server.query_catalog(&device_id).await.ok();
//!         }
//!         Gb28181ServerEvent::CatalogResponse { device_id, devices } => {
//!             println!("设备 {} 目录: {} 个子设备", device_id, devices.len());
//!         }
//!         _ => {}
//!     }
//! }
//! ```

use std::sync::Arc;

use sip_core::config::{RegistrationConfig, SipConfig};
use sip_core::{Host, SipVersion, TransportProtocol};
use sip_message::{
    CSeqHeader, CallId, FromToHeader, HeaderCollection, HeaderName, HeaderValue, Method,
    RequestLine, SipRequest, SipUri, Tag, ViaHeader,
};
use sip_registration::registrar::{MemoryRegistrationStore, Registrar};
use sip_sdp::gb28181::{
    build_download_invite_sdp, build_invite_sdp, build_playback_invite_sdp, AudioEncoding,
    MediaParam, StreamType, VideoEncoding,
};
use tokio::sync::{mpsc, Mutex};

use crate::device_registry::{DeviceRegistry, DeviceTree, RegisteredDevice};
use crate::engine::SipEngine;

use crate::subscription::SubscriptionManager;

/// GB28181 Notify 消息（设备端主动通知）
///
/// 对应 XML 根元素 `<Notify>`，用于心跳保活、报警通知、移动位置通知等。
/// 此结构体用于解析设备端发送的 Notify 消息，gb28181_xml 库暂不支持此类型。
#[derive(Debug, Clone, PartialEq)]
struct Notify {
    /// 命令类型
    cmd_type: gb28181_xml::CmdType,
    /// 命令序号
    sn: u32,
    /// 目标设备编码
    device_id: String,
    /// 报警列表
    alarm_list: Vec<gb28181_xml::AlarmInfo>,
    /// 移动位置列表
    mobile_position_list: Vec<gb28181_xml::MobilePositionInfo>,
}

impl Notify {
    /// 从 XML 字符串解析 Notify 消息
    fn from_xml(xml: &str) -> Result<Self, gb28181_xml::XmlError> {
        // 验证根元素
        let inner = extract_between_tags(xml, "Notify").ok_or_else(|| {
            gb28181_xml::XmlError::InvalidFormat("missing <Notify> root element".to_string())
        })?;

        let cmd_type_str = simple_extract_tag_value(&inner, "CmdType")
            .ok_or(gb28181_xml::XmlError::MissingField("CmdType".to_string()))?;
        let cmd_type = cmd_type_str
            .parse::<gb28181_xml::CmdType>()
            .expect("CmdType from XML must be valid");

        let sn_str = simple_extract_tag_value(&inner, "SN")
            .ok_or(gb28181_xml::XmlError::MissingField("SN".to_string()))?;
        let sn: u32 = sn_str
            .parse()
            .map_err(|_| gb28181_xml::XmlError::InvalidNumber(format!("invalid SN: {sn_str}")))?;

        let device_id = simple_extract_tag_value(&inner, "DeviceID")
            .ok_or(gb28181_xml::XmlError::MissingField("DeviceID".to_string()))?;

        // 解析报警列表
        let alarm_list = if let Some(list_inner) = extract_tag_with_attrs_local(&inner, "AlarmList")
        {
            let item_contents = extract_all_tags_local(&list_inner, "Item");
            let mut items = Vec::with_capacity(item_contents.len());
            for item_content in &item_contents {
                if let Ok(item) = gb28181_xml::AlarmInfo::from_xml_item(item_content) {
                    items.push(item);
                }
            }
            items
        } else {
            Vec::new()
        };

        // 解析移动位置列表
        let mobile_position_list =
            if let Some(list_inner) = extract_tag_with_attrs_local(&inner, "MobilePositionList") {
                let item_contents = extract_all_tags_local(&list_inner, "Item");
                let mut items = Vec::with_capacity(item_contents.len());
                for item_content in &item_contents {
                    if let Ok(item) = gb28181_xml::MobilePositionInfo::from_xml_item(item_content) {
                        items.push(item);
                    }
                }
                items
            } else {
                Vec::new()
            };

        Ok(Self {
            cmd_type,
            sn,
            device_id,
            alarm_list,
            mobile_position_list,
        })
    }
}

/// 提取根标签之间的内容
fn extract_between_tags(content: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");

    let start = content.find(&open)?;
    let inner_start = start + open.len();
    let inner_end = content[inner_start..].find(&close)?;
    Some(content[inner_start..inner_start + inner_end].to_string())
}

/// 简单的 XML 标签值提取（本地实现，不依赖 gb28181_xml 内部函数）
fn simple_extract_tag_value(content: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");

    let start = content.find(&open)?;
    let value_start = start + open.len();
    let value_end = content[value_start..].find(&close)?;
    Some(content[value_start..value_start + value_end].to_string())
}

/// 提取包含属性和子元素的标签内容（本地实现）
fn extract_tag_with_attrs_local(content: &str, tag: &str) -> Option<String> {
    let open_start = format!("<{tag}");
    let close = format!("</{tag}>");

    let start = content.find(&open_start)?;
    let tag_content_start = start + open_start.len();
    let gt_pos = content[tag_content_start..].find('>')?;
    let inner_start = tag_content_start + gt_pos + 1;
    let inner_end = content[inner_start..].find(&close)?;
    Some(content[inner_start..inner_start + inner_end].to_string())
}

/// 提取 XML 中所有同名标签的内容（本地实现）
fn extract_all_tags_local(content: &str, tag: &str) -> Vec<String> {
    let mut results = Vec::new();
    let open_start = format!("<{tag}");
    let close = format!("</{tag}>");

    let mut search_from = 0;
    while search_from < content.len() {
        let Some(start) = content[search_from..].find(&open_start) else {
            break;
        };
        let abs_start = search_from + start;
        let tag_content_start = abs_start + open_start.len();

        let Some(gt_offset) = content[tag_content_start..].find('>') else {
            break;
        };
        let inner_start = tag_content_start + gt_offset + 1;

        let Some(inner_end) = content[inner_start..].find(&close) else {
            break;
        };

        results.push(content[inner_start..inner_start + inner_end].to_string());
        search_from = inner_start + inner_end + close.len();
    }

    results
}

// ============================================================================
// Gb28181ServerConfig - GB28181 平台端配置
// ============================================================================

/// GB28181 平台端配置
///
/// 包含平台编码、SIP 监听、认证和心跳等完整配置信息。
#[derive(Debug, Clone)]
pub struct Gb28181ServerConfig {
    /// 平台国标编码（20位，如 34020000002000000001）
    pub server_id: String,
    /// 平台域名（通常与编码前10位相同）
    pub server_domain: String,
    /// SIP 监听IP
    pub sip_ip: String,
    /// SIP 监听端口
    pub sip_port: u16,
    /// 认证域
    pub realm: String,
    /// 认证密码（用于生成挑战）
    pub auth_password: String,
    /// 心跳超时（秒）
    pub heartbeat_timeout: u64,
    /// 注册有效期（秒）
    pub register_expires: u64,
}

// ============================================================================
// Gb28181ServerEvent - 平台端事件
// ============================================================================

/// 平台端事件
///
/// 平台端向上层应用通知的事件类型，涵盖设备注册、查询响应、
/// 视频点播、报警等 GB28181 典型交互场景。
#[derive(Debug)]
pub enum Gb28181ServerEvent {
    /// 设备注册成功
    DeviceRegistered { device_id: String },
    /// 设备注销
    DeviceUnregistered { device_id: String },
    /// 设备上线
    DeviceOnline { device_id: String },
    /// 设备离线
    DeviceOffline { device_id: String },
    /// 收到心跳
    KeepaliveReceived { device_id: String },
    /// 收到目录查询响应
    CatalogResponse {
        device_id: String,
        devices: Vec<gb28181_xml::DeviceItem>,
    },
    /// 收到设备信息响应
    DeviceInfoResponse {
        device_id: String,
        name: String,
        manufacturer: String,
        model: String,
    },
    /// 收到录像查询响应
    RecordQueryResponse {
        device_id: String,
        records: Vec<gb28181_xml::RecordItem>,
    },
    /// 收到设备状态响应
    DeviceStatusResponse {
        device_id: String,
        online: bool,
        status: String,
    },
    /// 收到报警通知
    AlarmReceived {
        device_id: String,
        alarms: Vec<gb28181_xml::AlarmInfo>,
    },
    /// 收到移动位置通知
    MobilePositionReceived {
        device_id: String,
        positions: Vec<gb28181_xml::MobilePositionInfo>,
    },
    /// 视频点播 INVITE 已发送
    InviteSent { device_id: String, call_id: String },
    /// 收到 INVITE 200 OK
    InviteOk {
        device_id: String,
        call_id: String,
        sdp: String,
    },
    /// 收到 BYE
    ByeReceived { device_id: String, call_id: String },
    /// 错误
    Error(String),
}

// ============================================================================
// Gb28181Server - GB28181 平台端
// ============================================================================

/// GB28181 平台端（WVP 信令服务器核心）
///
/// 封装 SIP 协议栈和 GB28181 业务逻辑，提供平台端完整功能。
/// 平台端接收设备注册，主动查询/控制设备，发起视频点播。
///
/// # 生命周期
///
/// 1. `new(config)` - 创建平台端实例
/// 2. `start()` - 启动（SIP 引擎 + Registrar + 心跳检测定时器）
/// 3. 事件循环 - 通过 `event_receiver()` 获取事件并处理
/// 4. `stop()` - 停止平台端
///
/// # 线程安全
///
/// 内部状态使用 `Arc<Mutex<...>>` 保护，可在多个异步任务间共享。
pub struct Gb28181Server {
    /// GB28181 平台端配置
    config: Gb28181ServerConfig,
    /// SIP 引擎
    engine: Arc<Mutex<SipEngine>>,
    /// 注册服务器
    registrar: Arc<Registrar>,
    /// 设备注册表
    device_registry: Arc<DeviceRegistry>,
    /// 订阅管理器
    subscription_manager: SubscriptionManager,
    /// 平台端事件发送端
    event_tx: mpsc::UnboundedSender<Gb28181ServerEvent>,
    /// 平台端事件接收端
    event_rx: Option<mpsc::UnboundedReceiver<Gb28181ServerEvent>>,
    /// 心跳检测定时器句柄
    heartbeat_check_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// 命令序号计数器
    sn_counter: Arc<Mutex<u32>>,
    /// 是否已启动
    running: Arc<Mutex<bool>>,
}

impl Gb28181Server {
    /// 创建 GB28181 平台端
    ///
    /// 根据配置构建 SIP 引擎、Registrar、设备注册表和订阅管理器。
    ///
    /// # 参数
    ///
    /// - `config` - GB28181 平台端配置
    pub fn new(config: Gb28181ServerConfig) -> Self {
        // 构建 SIP 配置
        let sip_config = Self::build_sip_config(&config);

        // 创建 SIP 引擎
        let engine = SipEngine::new(sip_config);

        // 创建 Registrar（注册服务器）
        let store = Arc::new(MemoryRegistrationStore::new());
        let auth_handler = Arc::new(sip_registration::auth::DigestAuthHandler::new());
        let registrar = Registrar::new(store, auth_handler, config.realm.clone(), true)
            .with_credential_lookup(Arc::new({
                let password = config.auth_password.clone();
                move |_username| Some(password.clone())
            }));

        // 创建设备注册表
        let device_registry = Arc::new(DeviceRegistry::new(config.heartbeat_timeout));

        // 创建订阅管理器
        let (subscription_manager, _sub_event_rx) = SubscriptionManager::new();

        // 创建事件通道
        let (event_tx, event_rx) = mpsc::unbounded_channel::<Gb28181ServerEvent>();

        Self {
            config,
            engine: Arc::new(Mutex::new(engine)),
            registrar: Arc::new(registrar),
            device_registry,
            subscription_manager,
            event_tx,
            event_rx: Some(event_rx),
            heartbeat_check_handle: Arc::new(Mutex::new(None)),
            sn_counter: Arc::new(Mutex::new(1)),
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// 启动平台端
    ///
    /// 启动 SIP 引擎和心跳检测定时器。
    pub async fn start(&self) -> Result<(), String> {
        let mut running = self.running.lock().await;
        if *running {
            tracing::warn!("Gb28181Server: already running");
            return Ok(());
        }

        // 启动 SIP 引擎
        self.engine
            .lock()
            .await
            .start()
            .await
            .map_err(|e| format!("failed to start SIP engine: {}", e))?;

        // 启动心跳检测定时器
        self.start_heartbeat_check_timer().await;

        *running = true;

        tracing::info!(
            "Gb28181Server: started (server_id={}, sip={}:{})",
            self.config.server_id,
            self.config.sip_ip,
            self.config.sip_port
        );

        Ok(())
    }

    /// 停止平台端
    ///
    /// 停止心跳检测定时器和 SIP 引擎。
    pub async fn stop(&self) -> Result<(), String> {
        let mut running = self.running.lock().await;
        if !*running {
            return Ok(());
        }

        // 停止心跳检测定时器
        if let Some(handle) = self.heartbeat_check_handle.lock().await.take() {
            handle.abort();
        }

        // 停止 SIP 引擎
        self.engine.lock().await.stop().await;

        *running = false;

        tracing::info!("Gb28181Server: stopped");

        Ok(())
    }

    // ========================================================================
    // 设备管理（被动接收）
    // ========================================================================

    /// 处理设备 REGISTER 请求
    ///
    /// 通过 Registrar 验证认证，注册到 DeviceRegistry。
    ///
    /// # 参数
    ///
    /// - `request` - REGISTER 请求
    ///
    /// # 返回
    ///
    /// 返回应发送给设备的 SIP 响应。
    pub async fn handle_register(&self, request: &SipRequest) -> sip_message::SipResponse {
        // 通过 Registrar 处理注册
        let response = self.registrar.handle_register(request, None).await;

        // 检查注册是否成功（200 OK）
        if response.status_line.status_code.is_success() {
            // 提取 AOR（从 To 头部）
            let aor = request
                .headers
                .get(&HeaderName::To)
                .and_then(|v| v.as_from_to())
                .map(|ft| ft.uri.to_string())
                .unwrap_or_default();

            // 提取 Contact URI
            let contact = request
                .headers
                .get(&HeaderName::Contact)
                .and_then(|v| v.as_contact())
                .map(|c| c.uri.to_string())
                .unwrap_or_default();

            // 提取 Call-ID
            let call_id = request
                .headers
                .get(&HeaderName::CallId)
                .and_then(|v| v.as_call_id())
                .map(|c| c.0.clone())
                .unwrap_or_default();

            // 提取 Expires
            let expires = self.config.register_expires;

            // 从 AOR 中提取 device_id
            let device_id = extract_device_id_from_aor(&aor);

            // 注册到 DeviceRegistry
            self.device_registry
                .register_device(
                    &device_id,
                    &contact,
                    &format!("{}:{}", self.config.sip_ip, self.config.sip_port),
                    expires,
                    &call_id,
                )
                .await;

            // 发送事件
            let _ = self.event_tx.send(Gb28181ServerEvent::DeviceRegistered {
                device_id: device_id.clone(),
            });
            let _ = self
                .event_tx
                .send(Gb28181ServerEvent::DeviceOnline { device_id });

            tracing::info!("Gb28181Server: device registered ({})", aor);
        }

        response
    }

    /// 处理设备心跳 MESSAGE
    ///
    /// 解析 Keepalive XML，更新 DeviceRegistry 心跳时间。
    ///
    /// # 参数
    ///
    /// - `request` - MESSAGE 请求
    pub async fn handle_keepalive(&self, request: &SipRequest) -> Result<(), String> {
        // 提取消息体
        let body = request
            .body
            .as_ref()
            .map(|b| String::from_utf8_lossy(&b.content).to_string())
            .unwrap_or_default();

        // 从 From 头部提取 device_id
        let device_id = extract_device_id_from_request(request);

        // 更新心跳时间
        let updated = self.device_registry.update_keepalive(&device_id).await;

        if updated {
            let _ = self.event_tx.send(Gb28181ServerEvent::KeepaliveReceived {
                device_id: device_id.clone(),
            });
            tracing::debug!("Gb28181Server: keepalive from {}", device_id);
        } else {
            tracing::warn!("Gb28181Server: keepalive from unknown device {}", device_id);
        }

        let _ = body; // 消息体已解析，避免未使用警告
        Ok(())
    }

    /// 处理设备报警 MESSAGE
    ///
    /// 解析报警 XML，触发 AlarmReceived 事件。
    ///
    /// # 参数
    ///
    /// - `request` - MESSAGE 请求
    pub async fn handle_alarm(&self, request: &SipRequest) -> Result<(), String> {
        let body = request
            .body
            .as_ref()
            .map(|b| String::from_utf8_lossy(&b.content).to_string())
            .unwrap_or_default();

        let device_id = extract_device_id_from_request(request);

        // 尝试解析报警 XML（可能是 Notify 或 Response）
        let alarms = if let Ok(notify) = Notify::from_xml(&body) {
            if notify.cmd_type == gb28181_xml::CmdType::Alarm {
                notify.alarm_list
            } else {
                Vec::new()
            }
        } else if let Ok(msg) = gb28181_xml::parse_xml(&body) {
            match msg {
                gb28181_xml::Message::Response(resp)
                    if resp.cmd_type == gb28181_xml::CmdType::Alarm =>
                {
                    resp.alarm_list
                }
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };

        let _ = self.event_tx.send(Gb28181ServerEvent::AlarmReceived {
            device_id: device_id.clone(),
            alarms: alarms.clone(),
        });

        tracing::info!(
            "Gb28181Server: alarm from {} (count={})",
            device_id,
            alarms.len()
        );

        Ok(())
    }

    /// 处理设备移动位置 MESSAGE
    ///
    /// 解析移动位置 XML，触发 MobilePositionReceived 事件。
    ///
    /// # 参数
    ///
    /// - `request` - MESSAGE 请求
    pub async fn handle_mobile_position(&self, request: &SipRequest) -> Result<(), String> {
        let body = request
            .body
            .as_ref()
            .map(|b| String::from_utf8_lossy(&b.content).to_string())
            .unwrap_or_default();

        let device_id = extract_device_id_from_request(request);

        // 尝试解析移动位置 XML（可能是 Notify 或 Response）
        let positions = if let Ok(notify) = Notify::from_xml(&body) {
            if notify.cmd_type == gb28181_xml::CmdType::MobilePositionNotify {
                notify.mobile_position_list
            } else {
                Vec::new()
            }
        } else if let Ok(msg) = gb28181_xml::parse_xml(&body) {
            match msg {
                gb28181_xml::Message::Response(resp)
                    if resp.cmd_type == gb28181_xml::CmdType::MobilePositionNotify =>
                {
                    resp.mobile_position_list
                }
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };

        let _ = self
            .event_tx
            .send(Gb28181ServerEvent::MobilePositionReceived {
                device_id: device_id.clone(),
                positions: positions.clone(),
            });

        tracing::info!(
            "Gb28181Server: mobile position from {} (count={})",
            device_id,
            positions.len()
        );

        Ok(())
    }

    /// 处理 MESSAGE 响应（目录/设备信息/录像/设备状态）
    ///
    /// 解析 XML 消息体，根据 CmdType 路由到不同的事件。
    ///
    /// # 参数
    ///
    /// - `request` - MESSAGE 请求
    pub async fn handle_message_response(&self, request: &SipRequest) -> Result<(), String> {
        let body = request
            .body
            .as_ref()
            .map(|b| String::from_utf8_lossy(&b.content).to_string())
            .unwrap_or_default();

        let device_id = extract_device_id_from_request(request);

        // 解析 XML
        let msg =
            gb28181_xml::parse_xml(&body).map_err(|e| format!("failed to parse XML: {}", e))?;

        match msg {
            gb28181_xml::Message::Response(response) => {
                match response.cmd_type {
                    gb28181_xml::CmdType::Catalog => {
                        // 更新设备目录
                        self.device_registry
                            .update_catalog(&device_id, response.device_list.clone())
                            .await;

                        let _ = self.event_tx.send(Gb28181ServerEvent::CatalogResponse {
                            device_id: device_id.clone(),
                            devices: response.device_list,
                        });
                    }
                    gb28181_xml::CmdType::DeviceInfo => {
                        // 提取设备信息
                        let name = response
                            .device_list
                            .first()
                            .and_then(|item| item.name.clone())
                            .unwrap_or_default();
                        let manufacturer = response
                            .device_list
                            .first()
                            .and_then(|item| item.manufacturer.clone())
                            .unwrap_or_default();
                        let model = response
                            .device_list
                            .first()
                            .and_then(|item| item.model.clone())
                            .unwrap_or_default();

                        // 更新设备信息
                        self.device_registry
                            .update_device_info(
                                &device_id,
                                Some(name.clone()),
                                Some(manufacturer.clone()),
                                Some(model.clone()),
                                None,
                                None,
                            )
                            .await;

                        let _ = self.event_tx.send(Gb28181ServerEvent::DeviceInfoResponse {
                            device_id: device_id.clone(),
                            name,
                            manufacturer,
                            model,
                        });
                    }
                    gb28181_xml::CmdType::RecordQuery => {
                        let _ = self.event_tx.send(Gb28181ServerEvent::RecordQueryResponse {
                            device_id: device_id.clone(),
                            records: response.record_list,
                        });
                    }
                    gb28181_xml::CmdType::DeviceStatus => {
                        if let Some(status) = response.device_status {
                            let _ = self
                                .event_tx
                                .send(Gb28181ServerEvent::DeviceStatusResponse {
                                    device_id: device_id.clone(),
                                    online: status.online,
                                    status: status.status,
                                });
                        }
                    }
                    gb28181_xml::CmdType::Alarm => {
                        let _ = self.event_tx.send(Gb28181ServerEvent::AlarmReceived {
                            device_id: device_id.clone(),
                            alarms: response.alarm_list,
                        });
                    }
                    gb28181_xml::CmdType::MobilePositionNotify => {
                        let _ = self
                            .event_tx
                            .send(Gb28181ServerEvent::MobilePositionReceived {
                                device_id: device_id.clone(),
                                positions: response.mobile_position_list,
                            });
                    }
                    _ => {
                        tracing::debug!(
                            "Gb28181Server: unhandled response CmdType: {:?}",
                            response.cmd_type
                        );
                    }
                }
            }

            gb28181_xml::Message::Control(_) => {
                tracing::debug!("Gb28181Server: received Control message (unexpected)");
            }
            gb28181_xml::Message::Query(_) => {
                tracing::debug!("Gb28181Server: received Query message (unexpected)");
            }
        }

        // 如果 parse_xml 失败，尝试解析 Notify
        if gb28181_xml::parse_xml(&body).is_err() {
            if let Ok(notify) = Notify::from_xml(&body) {
                match notify.cmd_type {
                    gb28181_xml::CmdType::Keepalive => {
                        // 心跳已在 handle_keepalive 中处理
                    }
                    gb28181_xml::CmdType::Alarm => {
                        let _ = self.event_tx.send(Gb28181ServerEvent::AlarmReceived {
                            device_id: device_id.clone(),
                            alarms: notify.alarm_list,
                        });
                    }
                    gb28181_xml::CmdType::MobilePositionNotify => {
                        let _ = self
                            .event_tx
                            .send(Gb28181ServerEvent::MobilePositionReceived {
                                device_id: device_id.clone(),
                                positions: notify.mobile_position_list,
                            });
                    }
                    _ => {
                        tracing::debug!(
                            "Gb28181Server: unhandled notify CmdType: {:?}",
                            notify.cmd_type
                        );
                    }
                }
            }
        }

        Ok(())
    }

    // ========================================================================
    // 主动查询（平台→设备）
    // ========================================================================

    /// 向设备发送目录查询 MESSAGE
    ///
    /// 构建 Catalog Query XML 并通过 MESSAGE 发送。
    ///
    /// # 参数
    ///
    /// - `device_id` - 目标设备编码
    pub async fn query_catalog(&self, device_id: &str) -> Result<(), String> {
        let sn = self.next_sn().await;
        let device_id_obj = gb28181_codec::DeviceId::parse(device_id)
            .map_err(|e| format!("invalid device_id: {}", e))?;

        let query = gb28181_xml::Query::catalog(sn, device_id_obj);
        let xml = query.to_xml();

        self.send_message_to_device(device_id, &xml).await?;

        tracing::info!(
            "Gb28181Server: sent catalog query to {} (sn={})",
            device_id,
            sn
        );

        Ok(())
    }

    /// 向设备发送设备信息查询 MESSAGE
    ///
    /// 构建 DeviceInfo Query XML 并通过 MESSAGE 发送。
    ///
    /// # 参数
    ///
    /// - `device_id` - 目标设备编码
    pub async fn query_device_info(&self, device_id: &str) -> Result<(), String> {
        let sn = self.next_sn().await;
        let device_id_obj = gb28181_codec::DeviceId::parse(device_id)
            .map_err(|e| format!("invalid device_id: {}", e))?;

        let query = gb28181_xml::Query::device_info(sn, device_id_obj);
        let xml = query.to_xml();

        self.send_message_to_device(device_id, &xml).await?;

        tracing::info!(
            "Gb28181Server: sent device info query to {} (sn={})",
            device_id,
            sn
        );

        Ok(())
    }

    /// 向设备发送录像查询 MESSAGE
    ///
    /// 构建 RecordQuery XML 并通过 MESSAGE 发送。
    ///
    /// # 参数
    ///
    /// - `device_id` - 目标设备编码
    /// - `start` - 开始时间（ISO 8601 格式，如 `2024-01-01T00:00:00`）
    /// - `end` - 结束时间（ISO 8601 格式）
    pub async fn query_record(
        &self,
        device_id: &str,
        start: &str,
        end: &str,
    ) -> Result<(), String> {
        let sn = self.next_sn().await;
        let device_id_obj = gb28181_codec::DeviceId::parse(device_id)
            .map_err(|e| format!("invalid device_id: {}", e))?;

        let query = gb28181_xml::Query::record_query(sn, device_id_obj, start, end);
        let xml = query.to_xml();

        self.send_message_to_device(device_id, &xml).await?;

        tracing::info!(
            "Gb28181Server: sent record query to {} (sn={}, start={}, end={})",
            device_id,
            sn,
            start,
            end
        );

        Ok(())
    }

    /// 向设备发送设备状态查询 MESSAGE
    ///
    /// 构建 DeviceStatus Query XML 并通过 MESSAGE 发送。
    ///
    /// # 参数
    ///
    /// - `device_id` - 目标设备编码
    pub async fn query_device_status(&self, device_id: &str) -> Result<(), String> {
        let sn = self.next_sn().await;
        let device_id_obj = gb28181_codec::DeviceId::parse(device_id)
            .map_err(|e| format!("invalid device_id: {}", e))?;

        let query = gb28181_xml::Query::device_status(sn, device_id_obj);
        let xml = query.to_xml();

        self.send_message_to_device(device_id, &xml).await?;

        tracing::info!(
            "Gb28181Server: sent device status query to {} (sn={})",
            device_id,
            sn
        );

        Ok(())
    }

    // ========================================================================
    // 视频点播（平台→设备）
    // ========================================================================

    /// 发起实时视频点播 INVITE
    ///
    /// 构建 GB28181 INVITE 请求（含 SDP），发起实时视频点播。
    ///
    /// # 参数
    ///
    /// - `device_id` - 目标设备编码
    pub async fn invite_live(&self, device_id: &str) -> Result<String, String> {
        let media_param = MediaParam {
            video_encoding: VideoEncoding::PS,
            audio_encoding: AudioEncoding::G711A,
            stream_type: StreamType::Live,
        };

        let sdp = build_invite_sdp(
            device_id,
            &self.config.sip_ip,
            self.config.sip_port,
            &media_param,
        );

        let sdp_str = sdp.to_sdp_string();
        let body = sdp_str.into_bytes();

        let target = self.get_device_target(device_id).await;
        let call_id = self
            .engine
            .lock()
            .await
            .make_call(&target, Some(body), Some("application/sdp"))
            .await?;

        let _ = self.event_tx.send(Gb28181ServerEvent::InviteSent {
            device_id: device_id.to_string(),
            call_id: call_id.clone(),
        });

        tracing::info!(
            "Gb28181Server: sent live invite to {} (call_id={})",
            device_id,
            call_id
        );

        Ok(call_id)
    }

    /// 发起历史回放 INVITE
    ///
    /// 构建 GB28181 INVITE 请求（含时间范围 SDP），发起历史回放。
    ///
    /// # 参数
    ///
    /// - `device_id` - 目标设备编码
    /// - `start` - 开始时间（ISO 8601 格式）
    /// - `end` - 结束时间（ISO 8601 格式）
    pub async fn invite_playback(
        &self,
        device_id: &str,
        start: &str,
        end: &str,
    ) -> Result<String, String> {
        let media_param = MediaParam {
            video_encoding: VideoEncoding::PS,
            audio_encoding: AudioEncoding::G711A,
            stream_type: StreamType::History,
        };

        let sdp = build_playback_invite_sdp(
            device_id,
            &self.config.sip_ip,
            self.config.sip_port,
            &media_param,
            start,
            end,
        );

        let sdp_str = sdp.to_sdp_string();
        let body = sdp_str.into_bytes();

        let target = self.get_device_target(device_id).await;
        let call_id = self
            .engine
            .lock()
            .await
            .make_call(&target, Some(body), Some("application/sdp"))
            .await?;

        let _ = self.event_tx.send(Gb28181ServerEvent::InviteSent {
            device_id: device_id.to_string(),
            call_id: call_id.clone(),
        });

        tracing::info!(
            "Gb28181Server: sent playback invite to {} (call_id={}, start={}, end={})",
            device_id,
            call_id,
            start,
            end
        );

        Ok(call_id)
    }

    /// 发起录像下载 INVITE
    ///
    /// 构建 GB28181 INVITE 请求（含下载速度 SDP），发起录像下载。
    ///
    /// # 参数
    ///
    /// - `device_id` - 目标设备编码
    /// - `start` - 开始时间（ISO 8601 格式）
    /// - `end` - 结束时间（ISO 8601 格式）
    /// - `speed` - 下载倍速（1/2/4）
    pub async fn invite_download(
        &self,
        device_id: &str,
        start: &str,
        end: &str,
        speed: u32,
    ) -> Result<String, String> {
        let media_param = MediaParam {
            video_encoding: VideoEncoding::PS,
            audio_encoding: AudioEncoding::G711A,
            stream_type: StreamType::Download,
        };

        let sdp = build_download_invite_sdp(
            device_id,
            &self.config.sip_ip,
            self.config.sip_port,
            &media_param,
            start,
            end,
            Some(speed),
        );

        let sdp_str = sdp.to_sdp_string();
        let body = sdp_str.into_bytes();

        let target = self.get_device_target(device_id).await;
        let call_id = self
            .engine
            .lock()
            .await
            .make_call(&target, Some(body), Some("application/sdp"))
            .await?;

        let _ = self.event_tx.send(Gb28181ServerEvent::InviteSent {
            device_id: device_id.to_string(),
            call_id: call_id.clone(),
        });

        tracing::info!(
            "Gb28181Server: sent download invite to {} (call_id={}, speed={})",
            device_id,
            call_id,
            speed
        );

        Ok(call_id)
    }

    /// 挂断通话
    ///
    /// 发送 BYE 请求。
    ///
    /// # 参数
    ///
    /// - `call_id` - 呼叫标识
    pub async fn hang_up(&self, call_id: &str) -> Result<(), String> {
        self.engine.lock().await.hang_up(call_id).await?;

        tracing::info!("Gb28181Server: hung up (call_id={})", call_id);

        Ok(())
    }

    // ========================================================================
    // 设备控制（平台→设备）
    // ========================================================================

    /// 云台控制
    ///
    /// 构建 PTZ Control XML 并通过 MESSAGE 发送。
    ///
    /// # 参数
    ///
    /// - `device_id` - 目标设备编码
    /// - `direction` - 云台方向
    /// - `speed` - 云台速度（0-255）
    pub async fn ptz_control(
        &self,
        device_id: &str,
        direction: gb28181_xml::PtzDirection,
        speed: u8,
    ) -> Result<(), String> {
        let sn = self.next_sn().await;
        let device_id_obj = gb28181_codec::DeviceId::parse(device_id)
            .map_err(|e| format!("invalid device_id: {}", e))?;

        let ptz_speed = gb28181_xml::PtzSpeed::new(speed);
        let control = gb28181_xml::Control::ptz_with_direction(
            sn,
            device_id_obj,
            direction,
            ptz_speed,
            ptz_speed,
        );
        let xml = control.to_xml();

        self.send_message_to_device(device_id, &xml).await?;

        tracing::info!(
            "Gb28181Server: sent PTZ control to {} (sn={})",
            device_id,
            sn
        );

        Ok(())
    }

    /// 远程启动
    ///
    /// 构建 TeleBoot Control XML 并通过 MESSAGE 发送。
    ///
    /// # 参数
    ///
    /// - `device_id` - 目标设备编码
    pub async fn remote_start(&self, device_id: &str) -> Result<(), String> {
        let sn = self.next_sn().await;
        let device_id_obj = gb28181_codec::DeviceId::parse(device_id)
            .map_err(|e| format!("invalid device_id: {}", e))?;

        let control = gb28181_xml::Control::remote_start(sn, device_id_obj);
        let xml = control.to_xml();

        self.send_message_to_device(device_id, &xml).await?;

        tracing::info!(
            "Gb28181Server: sent remote start to {} (sn={})",
            device_id,
            sn
        );

        Ok(())
    }

    /// 录像拖动（历史回放定位）
    ///
    /// 构建 PlayTime Control XML 并通过 MESSAGE 发送。
    ///
    /// # 参数
    ///
    /// - `device_id` - 目标设备编码
    /// - `play_time` - 回放定位时间（ISO 8601 格式）
    pub async fn drag_playback(&self, device_id: &str, play_time: &str) -> Result<(), String> {
        let sn = self.next_sn().await;
        let device_id_obj = gb28181_codec::DeviceId::parse(device_id)
            .map_err(|e| format!("invalid device_id: {}", e))?;

        let control = gb28181_xml::Control::drag_playback(sn, device_id_obj, play_time);
        let xml = control.to_xml();

        self.send_message_to_device(device_id, &xml).await?;

        tracing::info!(
            "Gb28181Server: sent drag playback to {} (sn={}, play_time={})",
            device_id,
            sn,
            play_time
        );

        Ok(())
    }

    // ========================================================================
    // 订阅（平台→设备）
    // ========================================================================

    /// 订阅设备目录变更
    ///
    /// 发送 SUBSCRIBE Catalog 请求。
    ///
    /// # 参数
    ///
    /// - `device_id` - 目标设备编码
    ///
    /// # 返回
    ///
    /// 返回订阅标识。
    pub async fn subscribe_catalog(&self, device_id: &str) -> Result<String, String> {
        let (subscription_id, request) = self
            .subscription_manager
            .subscribe(device_id, "Catalog", 3600, None)
            .await
            .map_err(|e| format!("failed to create subscription: {}", e))?;

        // 通过传输层发送 SUBSCRIBE
        let target = self.get_device_target(device_id).await;
        self.engine
            .lock()
            .await
            .send_request(&request, &target)
            .await
            .map_err(|e| format!("failed to send SUBSCRIBE: {}", e))?;

        tracing::info!(
            "Gb28181Server: subscribed catalog for {} (sub_id={})",
            device_id,
            subscription_id
        );

        Ok(subscription_id)
    }

    /// 订阅设备报警
    ///
    /// 发送 SUBSCRIBE Alarm 请求。
    ///
    /// # 参数
    ///
    /// - `device_id` - 目标设备编码
    ///
    /// # 返回
    ///
    /// 返回订阅标识。
    pub async fn subscribe_alarm(&self, device_id: &str) -> Result<String, String> {
        let (subscription_id, request) = self
            .subscription_manager
            .subscribe(device_id, "Alarm", 3600, None)
            .await
            .map_err(|e| format!("failed to create subscription: {}", e))?;

        let target = self.get_device_target(device_id).await;
        self.engine
            .lock()
            .await
            .send_request(&request, &target)
            .await
            .map_err(|e| format!("failed to send SUBSCRIBE: {}", e))?;

        tracing::info!(
            "Gb28181Server: subscribed alarm for {} (sub_id={})",
            device_id,
            subscription_id
        );

        Ok(subscription_id)
    }

    /// 订阅移动位置
    ///
    /// 发送 SUBSCRIBE MobilePosition 请求。
    ///
    /// # 参数
    ///
    /// - `device_id` - 目标设备编码
    ///
    /// # 返回
    ///
    /// 返回订阅标识。
    pub async fn subscribe_mobile_position(&self, device_id: &str) -> Result<String, String> {
        let (subscription_id, request) = self
            .subscription_manager
            .subscribe(device_id, "MobilePosition", 3600, None)
            .await
            .map_err(|e| format!("failed to create subscription: {}", e))?;

        let target = self.get_device_target(device_id).await;
        self.engine
            .lock()
            .await
            .send_request(&request, &target)
            .await
            .map_err(|e| format!("failed to send SUBSCRIBE: {}", e))?;

        tracing::info!(
            "Gb28181Server: subscribed mobile position for {} (sub_id={})",
            device_id,
            subscription_id
        );

        Ok(subscription_id)
    }

    /// 取消订阅
    ///
    /// 发送 SUBSCRIBE (Expires=0) 请求。
    ///
    /// # 参数
    ///
    /// - `subscription_id` - 订阅标识
    pub async fn unsubscribe(&self, subscription_id: &str) -> Result<(), String> {
        let request = self
            .subscription_manager
            .unsubscribe(subscription_id)
            .await
            .ok_or_else(|| format!("subscription {} not found", subscription_id))?;

        // 通过传输层发送 SUBSCRIBE (Expires=0)
        // 从订阅信息中获取 device_id
        let sub_info = self
            .subscription_manager
            .get_subscription(subscription_id)
            .await;

        if let Some(info) = sub_info {
            let target = self.get_device_target(&info.device_id).await;
            self.engine
                .lock()
                .await
                .send_request(&request, &target)
                .await
                .map_err(|e| format!("failed to send SUBSCRIBE (unsubscribe): {}", e))?;
        }

        tracing::info!("Gb28181Server: unsubscribed {}", subscription_id);

        Ok(())
    }

    // ========================================================================
    // 查询
    // ========================================================================

    /// 获取设备信息
    ///
    /// # 参数
    ///
    /// - `device_id` - 设备国标编码
    pub async fn get_device(&self, device_id: &str) -> Option<RegisteredDevice> {
        self.device_registry.get_device(device_id).await
    }

    /// 列出所有设备
    pub async fn list_devices(&self) -> Vec<RegisteredDevice> {
        self.device_registry.list_devices().await
    }

    /// 列出在线设备
    pub async fn list_online_devices(&self) -> Vec<RegisteredDevice> {
        self.device_registry.list_online_devices().await
    }

    /// 获取设备树
    pub async fn get_device_tree(&self) -> DeviceTree {
        self.device_registry
            .get_device_tree(&self.config.server_id)
            .await
    }

    /// 获取事件接收器
    ///
    /// 返回平台端事件接收端。此方法只能调用一次。
    pub fn event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<Gb28181ServerEvent>> {
        self.event_rx.take()
    }

    /// 获取配置引用
    pub fn config(&self) -> &Gb28181ServerConfig {
        &self.config
    }

    /// 获取设备注册表引用
    pub fn device_registry(&self) -> &Arc<DeviceRegistry> {
        &self.device_registry
    }

    /// 获取订阅管理器引用
    pub fn subscription_manager(&self) -> &SubscriptionManager {
        &self.subscription_manager
    }

    /// 获取 Registrar 引用
    pub fn registrar(&self) -> &Arc<Registrar> {
        &self.registrar
    }

    // ========================================================================
    // 内部方法
    // ========================================================================

    /// 构建 SIP 配置
    ///
    /// 根据 GB28181 平台端配置生成 SipConfig。
    fn build_sip_config(config: &Gb28181ServerConfig) -> SipConfig {
        let aor = format!("sip:{}@{}", config.server_id, config.server_domain);
        let contact = format!(
            "sip:{}@{}:{}",
            config.server_id, config.sip_ip, config.sip_port
        );

        SipConfig::builder()
            .aor(&aor)
            .contact(&contact)
            .sip_port(config.sip_port)
            .transport(TransportProtocol::Udp)
            .registration_config(RegistrationConfig {
                registrar_server: None,
                default_expires: config.register_expires,
                ..RegistrationConfig::default()
            })
            .build()
            .expect("invalid GB28181 Server SIP config")
    }

    /// 启动心跳检测定时器
    ///
    /// 周期性检查设备心跳超时。
    async fn start_heartbeat_check_timer(&self) {
        let interval = self.config.heartbeat_timeout;
        let device_registry = Arc::clone(&self.device_registry);

        let handle = tokio::spawn(async move {
            let mut interval_timer =
                tokio::time::interval(std::time::Duration::from_secs(interval));

            loop {
                interval_timer.tick().await;

                // 检查心跳超时
                let offline_count = device_registry.check_heartbeat().await;

                if offline_count > 0 {
                    tracing::info!(
                        "Gb28181Server: heartbeat check, {} device(s) offline",
                        offline_count
                    );
                }
            }
        });

        *self.heartbeat_check_handle.lock().await = Some(handle);
    }

    /// 获取下一个命令序号
    async fn next_sn(&self) -> u32 {
        let mut counter = self.sn_counter.lock().await;
        *counter += 1;
        *counter
    }

    /// 向设备发送 MESSAGE
    ///
    /// 构建 MESSAGE 请求并通过传输层发送。
    ///
    /// # 参数
    ///
    /// - `device_id` - 目标设备编码
    /// - `xml_body` - XML 消息体
    async fn send_message_to_device(&self, device_id: &str, xml_body: &str) -> Result<(), String> {
        let request = build_server_message_request(&self.config.server_id, device_id, xml_body);
        let target = self.get_device_target(device_id).await;

        self.engine
            .lock()
            .await
            .send_request(&request, &target)
            .await
            .map_err(|e| format!("failed to send MESSAGE: {}", e))?;

        Ok(())
    }

    /// 获取设备的目标地址
    ///
    /// 从 DeviceRegistry 中获取设备的联系地址，如果不存在则使用默认格式。
    async fn get_device_target(&self, device_id: &str) -> String {
        if let Some(device) = self.device_registry.get_device(device_id).await {
            return device.contact;
        }
        // 默认格式
        format!("sip:{}@{}", device_id, self.config.server_domain)
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 从 AOR 中提取设备 ID
///
/// AOR 格式为 `sip:device_id@domain`，提取 device_id 部分。
fn extract_device_id_from_aor(aor: &str) -> String {
    // 去掉 sip: 前缀
    let without_scheme = aor.strip_prefix("sip:").unwrap_or(aor);
    // 取 @ 前面的部分
    without_scheme
        .split('@')
        .next()
        .unwrap_or(without_scheme)
        .to_string()
}

/// 从 SIP 请求中提取设备 ID
///
/// 优先从 From 头部提取，其次从消息体中的 DeviceID 标签提取。
fn extract_device_id_from_request(request: &SipRequest) -> String {
    // 优先从 From 头部提取
    if let Some(from) = request
        .headers
        .get(&HeaderName::From)
        .and_then(|v| v.as_from_to())
    {
        let uri_str = from.uri.to_string();
        let device_id = extract_device_id_from_aor(&uri_str);
        if !device_id.is_empty() && device_id != "unknown" {
            return device_id;
        }
    }

    // 从消息体中提取 DeviceID
    if let Some(body) = &request.body {
        let body_str = String::from_utf8_lossy(&body.content);
        if let Some(device_id) = simple_extract_tag_value(&body_str, "DeviceID") {
            return device_id;
        }
    }

    String::new()
}

/// 构建平台端 MESSAGE SIP 请求
///
/// 生成 GB28181 MESSAGE 请求，From 为平台编码，To 为设备编码。
///
/// # 参数
///
/// - `server_id` - 平台国标编码
/// - `device_id` - 目标设备编码
/// - `xml_body` - XML 消息体
pub fn build_server_message_request(
    server_id: &str,
    device_id: &str,
    xml_body: &str,
) -> SipRequest {
    // Request-URI: sip:device_id@server_domain
    let request_uri = SipUri::parse(&format!("sip:{}", device_id)).unwrap_or_else(|_| {
        SipUri::parse("sip:unknown@unknown").expect("hardcoded fallback URI must be valid")
    });

    // From URI: sip:server_id
    let from_uri = SipUri::parse(&format!("sip:{}", server_id)).unwrap_or_else(|_| {
        SipUri::parse("sip:unknown@unknown").expect("hardcoded fallback URI must be valid")
    });
    let from_tag = Tag::new();

    // To URI: sip:device_id
    let to_uri = request_uri.clone();

    // Call-ID
    let call_id = CallId::new();

    // Via
    let via = ViaHeader::new(
        TransportProtocol::Udp,
        Host::Domain("0.0.0.0".to_string()),
        Some(5060),
    );

    // From
    let from_header = FromToHeader::new(from_uri).with_tag(from_tag);

    // To
    let to_header = FromToHeader::new(to_uri);

    // CSeq
    let cseq = CSeqHeader::new(1, Method::Message);

    // 组装头部
    let mut headers = HeaderCollection::new();
    headers.insert(HeaderName::Via, HeaderValue::Via(via));
    headers.insert(HeaderName::From, HeaderValue::FromTo(from_header));
    headers.insert(HeaderName::To, HeaderValue::FromTo(to_header));
    headers.insert(HeaderName::CallId, HeaderValue::CallId(call_id));
    headers.insert(HeaderName::CSeq, HeaderValue::CSeq(cseq));
    headers.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));
    headers.insert(
        HeaderName::ContentType,
        HeaderValue::ContentType("Application/MANSCDP+xml".to_string()),
    );

    // 消息体
    let body = sip_message::Body::new("Application/MANSCDP+xml", xml_body.as_bytes().to_vec());

    SipRequest {
        request_line: RequestLine {
            method: Method::Message,
            request_uri,
            version: SipVersion,
        },
        headers,
        body: Some(body),
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_config() -> Gb28181ServerConfig {
        Gb28181ServerConfig {
            server_id: "34020000002000000001".to_string(),
            server_domain: "3402000000".to_string(),
            sip_ip: "192.168.1.1".to_string(),
            sip_port: 5060,
            realm: "3402000000".to_string(),
            auth_password: "12345678".to_string(),
            heartbeat_timeout: 180,
            register_expires: 3600,
        }
    }

    #[test]
    fn test_server_config_creation() {
        let config = make_test_config();
        assert_eq!(config.server_id, "34020000002000000001");
        assert_eq!(config.server_domain, "3402000000");
        assert_eq!(config.sip_ip, "192.168.1.1");
        assert_eq!(config.sip_port, 5060);
        assert_eq!(config.realm, "3402000000");
        assert_eq!(config.auth_password, "12345678");
        assert_eq!(config.heartbeat_timeout, 180);
        assert_eq!(config.register_expires, 3600);
    }

    #[test]
    fn test_build_sip_config() {
        let config = make_test_config();
        let sip_config = Gb28181Server::build_sip_config(&config);

        assert_eq!(sip_config.aor, "sip:34020000002000000001@3402000000");
        assert_eq!(
            sip_config.contact,
            "sip:34020000002000000001@192.168.1.1:5060"
        );
        assert_eq!(sip_config.sip_port, 5060);
        assert_eq!(sip_config.transport, TransportProtocol::Udp);
        assert_eq!(sip_config.registration_config.default_expires, 3600);
    }

    #[test]
    fn test_server_new() {
        let config = make_test_config();
        let server = Gb28181Server::new(config);

        assert_eq!(server.config().server_id, "34020000002000000001");
        assert!(server.event_rx.is_some());
    }

    #[test]
    fn test_event_receiver_once() {
        let config = make_test_config();
        let mut server = Gb28181Server::new(config);

        // 第一次调用应返回 Some
        let rx = server.event_receiver();
        assert!(rx.is_some());

        // 第二次调用应返回 None
        let rx2 = server.event_receiver();
        assert!(rx2.is_none());
    }

    #[test]
    fn test_extract_device_id_from_aor() {
        assert_eq!(
            extract_device_id_from_aor("sip:34020000001320000001@3402000000"),
            "34020000001320000001"
        );
        assert_eq!(
            extract_device_id_from_aor("sip:34020000001320000001@192.168.1.1:5060"),
            "34020000001320000001"
        );
        assert_eq!(
            extract_device_id_from_aor("34020000001320000001"),
            "34020000001320000001"
        );
    }

    #[test]
    fn test_build_server_message_request() {
        let request = build_server_message_request(
            "34020000002000000001",
            "34020000001320000001",
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Query><CmdType>Catalog</CmdType><SN>1</SN><DeviceID>34020000001320000001</DeviceID></Query>",
        );

        assert_eq!(request.request_line.method, Method::Message);
        assert!(request.body.is_some());

        let body = request.body.as_ref().unwrap();
        assert_eq!(body.content_type, "Application/MANSCDP+xml");

        // 验证 Content-Type 头部
        let ct = request.headers.get(&HeaderName::ContentType);
        assert!(ct.is_some());
    }

    #[test]
    fn test_server_event_debug() {
        let event = Gb28181ServerEvent::DeviceRegistered {
            device_id: "34020000001320000001".to_string(),
        };
        assert!(format!("{:?}", event).contains("DeviceRegistered"));

        let event = Gb28181ServerEvent::DeviceUnregistered {
            device_id: "34020000001320000001".to_string(),
        };
        assert!(format!("{:?}", event).contains("DeviceUnregistered"));

        let event = Gb28181ServerEvent::DeviceOnline {
            device_id: "34020000001320000001".to_string(),
        };
        assert!(format!("{:?}", event).contains("DeviceOnline"));

        let event = Gb28181ServerEvent::DeviceOffline {
            device_id: "34020000001320000001".to_string(),
        };
        assert!(format!("{:?}", event).contains("DeviceOffline"));

        let event = Gb28181ServerEvent::KeepaliveReceived {
            device_id: "34020000001320000001".to_string(),
        };
        assert!(format!("{:?}", event).contains("KeepaliveReceived"));

        let event = Gb28181ServerEvent::CatalogResponse {
            device_id: "34020000001320000001".to_string(),
            devices: vec![],
        };
        assert!(format!("{:?}", event).contains("CatalogResponse"));

        let event = Gb28181ServerEvent::DeviceInfoResponse {
            device_id: "34020000001320000001".to_string(),
            name: "Camera1".to_string(),
            manufacturer: "Hikvision".to_string(),
            model: "DS-2CD2143".to_string(),
        };
        assert!(format!("{:?}", event).contains("DeviceInfoResponse"));

        let event = Gb28181ServerEvent::RecordQueryResponse {
            device_id: "34020000001320000001".to_string(),
            records: vec![],
        };
        assert!(format!("{:?}", event).contains("RecordQueryResponse"));

        let event = Gb28181ServerEvent::DeviceStatusResponse {
            device_id: "34020000001320000001".to_string(),
            online: true,
            status: "OK".to_string(),
        };
        assert!(format!("{:?}", event).contains("DeviceStatusResponse"));

        let event = Gb28181ServerEvent::AlarmReceived {
            device_id: "34020000001320000001".to_string(),
            alarms: vec![],
        };
        assert!(format!("{:?}", event).contains("AlarmReceived"));

        let event = Gb28181ServerEvent::MobilePositionReceived {
            device_id: "34020000001320000001".to_string(),
            positions: vec![],
        };
        assert!(format!("{:?}", event).contains("MobilePositionReceived"));

        let event = Gb28181ServerEvent::InviteSent {
            device_id: "34020000001320000001".to_string(),
            call_id: "call-123".to_string(),
        };
        assert!(format!("{:?}", event).contains("InviteSent"));

        let event = Gb28181ServerEvent::InviteOk {
            device_id: "34020000001320000001".to_string(),
            call_id: "call-123".to_string(),
            sdp: "v=0".to_string(),
        };
        assert!(format!("{:?}", event).contains("InviteOk"));

        let event = Gb28181ServerEvent::ByeReceived {
            device_id: "34020000001320000001".to_string(),
            call_id: "call-123".to_string(),
        };
        assert!(format!("{:?}", event).contains("ByeReceived"));

        let event = Gb28181ServerEvent::Error("test error".to_string());
        assert!(format!("{:?}", event).contains("Error"));
    }

    #[tokio::test]
    async fn test_server_not_running_initially() {
        let config = make_test_config();
        let server = Gb28181Server::new(config);

        assert!(!*server.running.lock().await);
    }

    #[tokio::test]
    async fn test_query_catalog_without_engine() {
        let config = make_test_config();
        let server = Gb28181Server::new(config);

        // 由于未启动 SIP 引擎，实际发送会失败
        let result = server.query_catalog("34020000001320000001").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_query_device_info_without_engine() {
        let config = make_test_config();
        let server = Gb28181Server::new(config);

        let result = server.query_device_info("34020000001320000001").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_query_record_without_engine() {
        let config = make_test_config();
        let server = Gb28181Server::new(config);

        let result = server
            .query_record(
                "34020000001320000001",
                "2024-01-01T00:00:00",
                "2024-01-31T23:59:59",
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_query_device_status_without_engine() {
        let config = make_test_config();
        let server = Gb28181Server::new(config);

        let result = server.query_device_status("34020000001320000001").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_invite_live_without_engine() {
        let config = make_test_config();
        let server = Gb28181Server::new(config);

        let result = server.invite_live("34020000001320000001").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_invite_playback_without_engine() {
        let config = make_test_config();
        let server = Gb28181Server::new(config);

        let result = server
            .invite_playback(
                "34020000001320000001",
                "2024-01-01T00:00:00",
                "2024-01-31T23:59:59",
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_invite_download_without_engine() {
        let config = make_test_config();
        let server = Gb28181Server::new(config);

        let result = server
            .invite_download(
                "34020000001320000001",
                "2024-01-01T00:00:00",
                "2024-01-31T23:59:59",
                4,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_ptz_control_without_engine() {
        let config = make_test_config();
        let server = Gb28181Server::new(config);

        let result = server
            .ptz_control("34020000001320000001", gb28181_xml::PtzDirection::Up, 31)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_remote_start_without_engine() {
        let config = make_test_config();
        let server = Gb28181Server::new(config);

        let result = server.remote_start("34020000001320000001").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_drag_playback_without_engine() {
        let config = make_test_config();
        let server = Gb28181Server::new(config);

        let result = server
            .drag_playback("34020000001320000001", "2024-01-15T10:30:00")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_subscribe_catalog_without_engine() {
        let config = make_test_config();
        let server = Gb28181Server::new(config);

        let result = server.subscribe_catalog("34020000001320000001").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_device_registry_operations() {
        let config = make_test_config();
        let server = Gb28181Server::new(config);

        // 初始时没有设备
        let devices = server.list_devices().await;
        assert!(devices.is_empty());

        let online_devices = server.list_online_devices().await;
        assert!(online_devices.is_empty());

        let device = server.get_device("34020000001320000001").await;
        assert!(device.is_none());
    }

    #[tokio::test]
    async fn test_device_tree() {
        let config = make_test_config();
        let server = Gb28181Server::new(config);

        let tree = server.get_device_tree().await;
        assert_eq!(tree.root.device_id, "34020000002000000001");
        assert!(tree.root.children.is_empty());
    }

    #[test]
    fn test_build_server_message_request_catalog_query() {
        let device_id = "34020000001320000001";
        let device_id_obj = gb28181_codec::DeviceId::parse(device_id).unwrap();
        let query = gb28181_xml::Query::catalog(1, device_id_obj);
        let xml = query.to_xml();

        let request = build_server_message_request("34020000002000000001", device_id, &xml);

        assert_eq!(request.request_line.method, Method::Message);
        assert!(request.body.is_some());

        let body_str = String::from_utf8_lossy(&request.body.as_ref().unwrap().content);
        assert!(body_str.contains("<CmdType>Catalog</CmdType>"));
        assert!(body_str.contains("<SN>1</SN>"));
        assert!(body_str.contains(&format!("<DeviceID>{}</DeviceID>", device_id)));
    }

    #[test]
    fn test_build_server_message_request_ptz_control() {
        let device_id = "34020000001320000001";
        let device_id_obj = gb28181_codec::DeviceId::parse(device_id).unwrap();
        let control = gb28181_xml::Control::ptz(3, device_id_obj, "A5201F1F00000085");
        let xml = control.to_xml();

        let request = build_server_message_request("34020000002000000001", device_id, &xml);

        assert_eq!(request.request_line.method, Method::Message);
        let body_str = String::from_utf8_lossy(&request.body.as_ref().unwrap().content);
        assert!(body_str.contains("<CmdType>DeviceControl</CmdType>"));
        assert!(body_str.contains("<PTZCmd>A5201F1F00000085</PTZCmd>"));
    }

    #[test]
    fn test_build_server_message_request_remote_start() {
        let device_id = "34020000001320000001";
        let device_id_obj = gb28181_codec::DeviceId::parse(device_id).unwrap();
        let control = gb28181_xml::Control::remote_start(10, device_id_obj);
        let xml = control.to_xml();

        let request = build_server_message_request("34020000002000000001", device_id, &xml);

        let body_str = String::from_utf8_lossy(&request.body.as_ref().unwrap().content);
        assert!(body_str.contains("<CmdType>DeviceControl</CmdType>"));
        assert!(body_str.contains("<TeleBoot>Boot</TeleBoot>"));
    }

    #[test]
    fn test_build_server_message_request_record_query() {
        let device_id = "34020000001320000001";
        let device_id_obj = gb28181_codec::DeviceId::parse(device_id).unwrap();
        let query = gb28181_xml::Query::record_query(
            10,
            device_id_obj,
            "2024-01-01T00:00:00",
            "2024-01-31T23:59:59",
        );
        let xml = query.to_xml();

        let request = build_server_message_request("34020000002000000001", device_id, &xml);

        let body_str = String::from_utf8_lossy(&request.body.as_ref().unwrap().content);
        assert!(body_str.contains("<CmdType>RecordQuery</CmdType>"));
        assert!(body_str.contains("<StartTime>2024-01-01T00:00:00</StartTime>"));
        assert!(body_str.contains("<EndTime>2024-01-31T23:59:59</EndTime>"));
    }

    #[test]
    fn test_build_server_message_request_device_status() {
        let device_id = "34020000001320000001";
        let device_id_obj = gb28181_codec::DeviceId::parse(device_id).unwrap();
        let query = gb28181_xml::Query::device_status(5, device_id_obj);
        let xml = query.to_xml();

        let request = build_server_message_request("34020000002000000001", device_id, &xml);

        let body_str = String::from_utf8_lossy(&request.body.as_ref().unwrap().content);
        assert!(body_str.contains("<CmdType>DeviceStatus</CmdType>"));
    }
}
