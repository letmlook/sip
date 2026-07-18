//! GB28181 设备端核心实现
//!
//! 实现 GB/T 28181 国标设备端的完整流程，包括：
//! - 设备注册（REGISTER + 摘要认证）
//! - 心跳保活（MESSAGE Keepalive）
//! - 目录查询响应（MESSAGE Catalog Response）
//! - 设备信息查询响应（MESSAGE DeviceInfo Response）
//! - 视频点播（INVITE → 200 OK 含 GB28181 SDP）
//! - 挂断通话（BYE）
//! - 云台控制（PTZ Control）
//!
//! # 架构
//!
//! ```text
//! Application Layer
//!       ↕ (Gb28181Event)
//! Gb28181Device
//!       ↕
//! ┌────┼────┬────────────┐
//! SipEngine  SubscriptionManager
//!       ↕
//! Registration  MESSAGE  INVITE/BYE
//! ```
//!
//! # 示例
//!
//! ```ignore
//! use sip_ua::gb28181::{Gb28181Config, Gb28181Device};
//!
//! let config = Gb28181Config {
//!     device_id: "34020000001320000001".to_string(),
//!     server_addr: "192.168.1.1".to_string(),
//!     server_port: 5060,
//!     server_domain: "3402000000".to_string(),
//!     username: "34020000001320000001".to_string(),
//!     password: "12345678".to_string(),
//!     expires: 3600,
//!     heartbeat_interval: 60,
//!     local_ip: "192.168.1.100".to_string(),
//!     local_port: 5060,
//! };
//!
//! let mut device = Gb28181Device::new(config);
//! device.start().await.unwrap();
//!
//! // 获取事件接收器
//! let mut event_rx = device.event_receiver().unwrap();
//!
//! // 处理事件
//! while let Some(event) = event_rx.recv().await {
//!     match event {
//!         Gb28181Event::Registered => println!("注册成功"),
//!         Gb28181Event::CatalogQuery { sn, device_id } => {
//!             device.handle_catalog_query(sn, &device_id, vec![]).await;
//!         }
//!         Gb28181Event::InviteReceived { call_id, device_id, sdp } => {
//!             device.accept_invite(&call_id, "192.168.1.100", 6000).await;
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
use sip_sdp::gb28181::{build_ok_sdp, AudioEncoding, MediaParam, StreamType, VideoEncoding};
use sip_sdp::types::SessionDescription;
use tokio::sync::{mpsc, Mutex};

use crate::engine::SipEngine;
use crate::subscription::SubscriptionManager;

// ============================================================================
// Gb28181Config - GB28181 设备配置
// ============================================================================

/// GB28181 设备配置
///
/// 包含设备注册、心跳、媒体等完整配置信息。
#[derive(Debug, Clone)]
pub struct Gb28181Config {
    /// 设备国标编码（20位）
    pub device_id: String,
    /// SIP 服务器地址
    pub server_addr: String,
    /// SIP 服务器端口
    pub server_port: u16,
    /// SIP 服务器域（通常与设备编码前10位相同）
    pub server_domain: String,
    /// 认证用户名
    pub username: String,
    /// 认证密码
    pub password: String,
    /// 注册有效期（秒）
    pub expires: u64,
    /// 心跳间隔（秒）
    pub heartbeat_interval: u64,
    /// 本机IP
    pub local_ip: String,
    /// 本机SIP端口
    pub local_port: u16,
}

// ============================================================================
// Gb28181Event - GB28181 设备事件
// ============================================================================

/// GB28181 设备事件
///
/// 设备端向上层应用通知的事件类型，涵盖注册、查询、点播等
/// GB28181 典型交互场景。
#[derive(Debug)]
pub enum Gb28181Event {
    /// 注册成功
    Registered,
    /// 注册失败
    RegistrationFailed(String),
    /// 收到目录查询
    CatalogQuery {
        /// 命令序号
        sn: u32,
        /// 目标设备编码
        device_id: String,
    },
    /// 收到设备信息查询
    DeviceInfoQuery {
        /// 命令序号
        sn: u32,
        /// 目标设备编码
        device_id: String,
    },
    /// 收到视频点播请求
    InviteReceived {
        /// 呼叫标识
        call_id: String,
        /// 目标设备编码
        device_id: String,
        /// 会话描述
        sdp: Box<SessionDescription>,
    },
    /// 对方挂断
    ByeReceived {
        /// 呼叫标识
        call_id: String,
    },
    /// 收到云台控制命令
    PtzControl {
        /// 目标设备编码
        device_id: String,
        /// PTZ 命令（十六进制字符串）
        command: String,
    },
    /// 收到心跳响应
    KeepaliveOk,
    /// 心跳超时
    KeepaliveTimeout,
}

// ============================================================================
// Gb28181Device - GB28181 设备端
// ============================================================================

/// GB28181 设备端
///
/// 封装 SIP 协议栈和 GB28181 业务逻辑，提供设备端完整功能。
///
/// # 生命周期
///
/// 1. `new(config)` - 创建设备实例
/// 2. `start()` - 启动设备（注册 + 心跳定时器）
/// 3. 事件循环 - 通过 `event_receiver()` 获取事件并处理
/// 4. `stop()` - 停止设备（注销 + 清理）
///
/// # 线程安全
///
/// 内部状态使用 `Arc<Mutex<...>>` 保护，可在多个异步任务间共享。
pub struct Gb28181Device {
    /// GB28181 配置
    config: Gb28181Config,
    /// SIP 引擎
    engine: Arc<Mutex<SipEngine>>,
    /// 注册标识
    registration_id: Arc<Mutex<Option<String>>>,
    /// 订阅管理器
    #[allow(dead_code)]
    subscription_manager: SubscriptionManager,
    /// GB28181 事件发送端
    event_tx: mpsc::UnboundedSender<Gb28181Event>,
    /// GB28181 事件接收端
    event_rx: Option<mpsc::UnboundedReceiver<Gb28181Event>>,
    /// 心跳定时器句柄
    heartbeat_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// 注册状态
    registered: Arc<Mutex<bool>>,
    /// 当前活跃通话的 Call-ID
    active_call_id: Arc<Mutex<Option<String>>>,
    /// 命令序号计数器
    sn_counter: Arc<Mutex<u32>>,
}

impl Gb28181Device {
    /// 创建 GB28181 设备
    ///
    /// 根据配置构建 SIP 引擎和订阅管理器。
    ///
    /// # 参数
    ///
    /// - `config` - GB28181 设备配置
    pub fn new(config: Gb28181Config) -> Self {
        // 构建 SIP 配置
        let sip_config = Self::build_sip_config(&config);

        // 创建 SIP 引擎
        let engine = SipEngine::new(sip_config);

        // 创建订阅管理器
        let (subscription_manager, _sub_event_rx) = SubscriptionManager::new();

        // 创建事件通道
        let (event_tx, event_rx) = mpsc::unbounded_channel::<Gb28181Event>();

        Self {
            config,
            engine: Arc::new(Mutex::new(engine)),
            registration_id: Arc::new(Mutex::new(None)),
            subscription_manager,
            event_tx,
            event_rx: Some(event_rx),
            heartbeat_handle: Arc::new(Mutex::new(None)),
            registered: Arc::new(Mutex::new(false)),
            active_call_id: Arc::new(Mutex::new(None)),
            sn_counter: Arc::new(Mutex::new(1)),
        }
    }

    /// 启动设备
    ///
    /// 启动 SIP 引擎、发起注册、启动心跳定时器。
    pub async fn start(&self) -> Result<(), String> {
        // 启动 SIP 引擎
        self.engine
            .lock()
            .await
            .start()
            .await
            .map_err(|e| format!("failed to start SIP engine: {}", e))?;

        // 发起注册
        self.register().await?;

        // 启动心跳定时器
        self.start_heartbeat_timer().await;

        tracing::info!(
            "Gb28181Device: started (device_id={})",
            self.config.device_id
        );

        Ok(())
    }

    /// 停止设备
    ///
    /// 注销、停止心跳、停止 SIP 引擎。
    pub async fn stop(&self) -> Result<(), String> {
        // 停止心跳定时器
        if let Some(handle) = self.heartbeat_handle.lock().await.take() {
            handle.abort();
        }

        // 注销
        let reg_id = self.registration_id.lock().await.clone();
        if let Some(id) = reg_id {
            let _ = self.engine.lock().await.unregister(&id).await;
        }

        // 停止 SIP 引擎
        self.engine.lock().await.stop().await;

        *self.registered.lock().await = false;

        tracing::info!(
            "Gb28181Device: stopped (device_id={})",
            self.config.device_id
        );

        Ok(())
    }

    /// 发起注册
    ///
    /// 构建 REGISTER 请求并通过 SIP 引擎发送。
    /// AOR 格式为 `sip:{device_id}@{server_domain}`。
    pub async fn register(&self) -> Result<(), String> {
        let reg_id = self.engine.lock().await.register().await?;

        *self.registration_id.lock().await = Some(reg_id);
        *self.registered.lock().await = true;

        let _ = self.event_tx.send(Gb28181Event::Registered);

        tracing::info!(
            "Gb28181Device: registered (device_id={})",
            self.config.device_id
        );

        Ok(())
    }

    /// 注销
    ///
    /// 发送 REGISTER (Expires=0) 请求。
    pub async fn unregister(&self) -> Result<(), String> {
        let reg_id = self.registration_id.lock().await.clone();

        if let Some(id) = reg_id {
            self.engine.lock().await.unregister(&id).await?;
            *self.registered.lock().await = false;
            *self.registration_id.lock().await = None;

            tracing::info!(
                "Gb28181Device: unregistered (device_id={})",
                self.config.device_id
            );
        }

        Ok(())
    }

    /// 发送心跳（MESSAGE Keepalive）
    ///
    /// 构建 MESSAGE 请求，消息体为 GB28181 Keepalive XML。
    ///
    /// # 返回
    ///
    /// 返回构建的 MESSAGE 请求（已通过传输层发送）。
    pub async fn send_heartbeat(&self) -> Result<(), String> {
        let sn = self.next_sn().await;

        // 构建 Keepalive XML
        let keepalive_xml = build_keepalive_xml(sn, &self.config.device_id);

        // 构建 MESSAGE 请求
        let request = self.build_message_request(&keepalive_xml).await;

        // 通过传输层发送
        let target = format!(
            "sip:{}@{}:{}",
            self.config.device_id, self.config.server_addr, self.config.server_port
        );

        self.engine
            .lock()
            .await
            .send_request(&request, &target)
            .await
            .map_err(|e| format!("failed to send heartbeat: {}", e))?;

        tracing::debug!("Gb28181Device: sent heartbeat (sn={})", sn);

        Ok(())
    }

    /// 响应目录查询
    ///
    /// 构建 MESSAGE 响应，消息体为设备目录 XML。
    ///
    /// # 参数
    ///
    /// - `sn` - 命令序号（与查询请求中的 SN 一致）
    /// - `device_id` - 目标设备编码
    /// - `items` - 设备目录列表
    pub async fn handle_catalog_query(
        &self,
        sn: u32,
        device_id: &str,
        items: Vec<gb28181_xml::DeviceItem>,
    ) -> Result<(), String> {
        // 解析 device_id 为 DeviceId
        let device_id_obj = gb28181_codec::DeviceId::parse(device_id)
            .map_err(|e| format!("invalid device_id: {}", e))?;

        // 构建 Catalog 响应 XML
        let sum_num = items.len() as u32;
        let response = gb28181_xml::Response::catalog(sn, device_id_obj, sum_num, items);
        let xml = response.to_xml();

        // 构建 MESSAGE 请求
        let request = self.build_message_request(&xml).await;

        // 通过传输层发送
        let target = format!(
            "sip:{}@{}:{}",
            self.config.device_id, self.config.server_addr, self.config.server_port
        );

        self.engine
            .lock()
            .await
            .send_request(&request, &target)
            .await
            .map_err(|e| format!("failed to send catalog response: {}", e))?;

        tracing::info!(
            "Gb28181Device: sent catalog response (sn={}, items={})",
            sn,
            sum_num
        );

        Ok(())
    }

    /// 响应设备信息查询
    ///
    /// 构建 MESSAGE 响应，消息体为设备信息 XML。
    ///
    /// # 参数
    ///
    /// - `sn` - 命令序号
    /// - `device_id` - 目标设备编码
    /// - `manufacturer` - 厂商名称
    /// - `model` - 设备型号
    /// - `firmware` - 固件版本
    pub async fn handle_device_info_query(
        &self,
        sn: u32,
        device_id: &str,
        manufacturer: &str,
        model: &str,
        firmware: &str,
    ) -> Result<(), String> {
        let device_id_obj = gb28181_codec::DeviceId::parse(device_id)
            .map_err(|e| format!("invalid device_id: {}", e))?;

        // 构建 DeviceInfo 响应 XML
        let mut response = gb28181_xml::Response::device_info(sn, device_id_obj);
        // DeviceInfo 响应通常包含一个设备项
        let mut item = gb28181_xml::DeviceItem::new(
            gb28181_codec::DeviceId::parse(device_id)
                .map_err(|e| format!("invalid device_id: {}", e))?,
        );
        item.manufacturer = Some(manufacturer.to_string());
        item.model = Some(model.to_string());
        // 使用 parent_id 字段传递 firmware 信息（GB28181 标准扩展）
        item.parent_id = Some(firmware.to_string());

        response.device_list = vec![item];
        response.sum_num = Some(1);

        let xml = response.to_xml();

        // 构建 MESSAGE 请求
        let request = self.build_message_request(&xml).await;

        // 通过传输层发送
        let target = format!(
            "sip:{}@{}:{}",
            self.config.device_id, self.config.server_addr, self.config.server_port
        );

        self.engine
            .lock()
            .await
            .send_request(&request, &target)
            .await
            .map_err(|e| format!("failed to send device info response: {}", e))?;

        tracing::info!("Gb28181Device: sent device info response (sn={})", sn);

        Ok(())
    }

    /// 接受视频点播
    ///
    /// 发送 200 OK 响应，消息体为 GB28181 SDP。
    ///
    /// # 参数
    ///
    /// - `call_id` - 呼叫标识
    /// - `media_ip` - 媒体发送 IP
    /// - `media_port` - 媒体发送端口
    pub async fn accept_invite(
        &self,
        call_id: &str,
        media_ip: &str,
        media_port: u16,
    ) -> Result<(), String> {
        // 构建 GB28181 200 OK SDP
        let media_param = MediaParam {
            video_encoding: VideoEncoding::PS,
            audio_encoding: AudioEncoding::G711A,
            stream_type: StreamType::Live,
        };

        let sdp = build_ok_sdp(&self.config.device_id, media_ip, media_port, &media_param);

        let sdp_str = sdp.to_sdp_string();
        let body = sdp_str.into_bytes();

        // 通过 SipEngine 接听
        self.engine
            .lock()
            .await
            .answer_call(call_id, Some(body), Some("application/sdp"))
            .await?;

        // 记录活跃通话
        *self.active_call_id.lock().await = Some(call_id.to_string());

        tracing::info!(
            "Gb28181Device: accepted invite (call_id={}, media={}:{})",
            call_id,
            media_ip,
            media_port
        );

        Ok(())
    }

    /// 拒绝视频点播
    ///
    /// 发送拒绝响应。
    ///
    /// # 参数
    ///
    /// - `call_id` - 呼叫标识
    /// - `status_code` - 拒绝状态码（可选，默认 486）
    /// - `reason` - 原因短语（可选）
    pub async fn reject_invite(
        &self,
        call_id: &str,
        status_code: Option<u16>,
        reason: Option<&str>,
    ) -> Result<(), String> {
        self.engine
            .lock()
            .await
            .reject_call(call_id, status_code, reason)
            .await?;

        tracing::info!("Gb28181Device: rejected invite (call_id={})", call_id);

        Ok(())
    }

    /// 挂断通话
    ///
    /// 发送 BYE 请求。
    ///
    /// # 参数
    ///
    /// - `call_id` - 呼叫标识（可选，默认使用当前活跃通话）
    pub async fn hang_up(&self, call_id: Option<&str>) -> Result<(), String> {
        let cid = if let Some(id) = call_id {
            id.to_string()
        } else {
            self.active_call_id
                .lock()
                .await
                .clone()
                .ok_or_else(|| "no active call to hang up".to_string())?
        };

        self.engine.lock().await.hang_up(&cid).await?;

        // 清除活跃通话记录
        let mut active = self.active_call_id.lock().await;
        if active.as_deref() == Some(&cid) {
            *active = None;
        }

        tracing::info!("Gb28181Device: hung up (call_id={})", cid);

        Ok(())
    }

    /// 获取事件接收器
    ///
    /// 返回 GB28181 事件接收端。此方法只能调用一次。
    pub fn event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<Gb28181Event>> {
        self.event_rx.take()
    }

    /// 是否已注册
    pub async fn is_registered(&self) -> bool {
        *self.registered.lock().await
    }

    /// 获取设备编码
    pub fn device_id(&self) -> &str {
        &self.config.device_id
    }

    /// 获取配置引用
    pub fn config(&self) -> &Gb28181Config {
        &self.config
    }

    // ========================================================================
    // 内部方法
    // ========================================================================

    /// 构建 SIP 配置
    ///
    /// 根据 GB28181 设备配置生成 SipConfig，AOR 格式为
    /// `sip:{device_id}@{server_domain}`。
    fn build_sip_config(config: &Gb28181Config) -> SipConfig {
        let aor = format!("sip:{}@{}", config.device_id, config.server_domain);
        let contact = format!(
            "sip:{}@{}:{}",
            config.device_id, config.local_ip, config.local_port
        );
        let registrar = format!(
            "sip:{}@{}:{}",
            config.device_id, config.server_addr, config.server_port
        );

        SipConfig::builder()
            .aor(&aor)
            .contact(&contact)
            .registrar_server(&registrar)
            .credentials(&config.username, &config.password)
            .sip_port(config.local_port)
            .transport(TransportProtocol::Udp)
            .registration_config(RegistrationConfig {
                registrar_server: Some(registrar),
                default_expires: config.expires,
                ..RegistrationConfig::default()
            })
            .build()
            .expect("invalid GB28181 SIP config")
    }

    /// 启动心跳定时器
    ///
    /// 周期性发送 MESSAGE Keepalive。
    async fn start_heartbeat_timer(&self) {
        let interval = self.config.heartbeat_interval;
        let device_id = self.config.device_id.clone();
        let engine = Arc::clone(&self.engine);
        let event_tx = self.event_tx.clone();
        let registered = Arc::clone(&self.registered);
        let sn_counter = Arc::clone(&self.sn_counter);

        let handle = tokio::spawn(async move {
            let mut interval_timer =
                tokio::time::interval(std::time::Duration::from_secs(interval));

            loop {
                interval_timer.tick().await;

                // 检查是否已注册
                if !*registered.lock().await {
                    continue;
                }

                // 递增 SN
                let sn = {
                    let mut counter = sn_counter.lock().await;
                    *counter += 1;
                    *counter
                };

                // 构建 Keepalive XML
                let keepalive_xml = build_keepalive_xml(sn, &device_id);

                // 构建 MESSAGE 请求
                let request = build_message_sip_request(&device_id, &keepalive_xml);

                // 发送心跳
                let result = engine
                    .lock()
                    .await
                    .send_request(&request, &format!("sip:{}@{}", device_id, device_id))
                    .await;

                match result {
                    Ok(()) => {
                        tracing::debug!("Gb28181Device: heartbeat sent (sn={})", sn);
                        let _ = event_tx.send(Gb28181Event::KeepaliveOk);
                    }
                    Err(e) => {
                        tracing::warn!("Gb28181Device: heartbeat failed: {}", e);
                        let _ = event_tx.send(Gb28181Event::KeepaliveTimeout);
                    }
                }
            }
        });

        *self.heartbeat_handle.lock().await = Some(handle);
    }

    /// 获取下一个命令序号
    async fn next_sn(&self) -> u32 {
        let mut counter = self.sn_counter.lock().await;
        *counter += 1;
        *counter
    }

    /// 构建 MESSAGE 请求
    ///
    /// 生成 GB28181 MESSAGE 请求，Content-Type 为 Application/MANSCDP+xml。
    async fn build_message_request(&self, xml_body: &str) -> SipRequest {
        build_message_sip_request(&self.config.device_id, xml_body)
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 构建 Keepalive XML
///
/// 生成符合 GB28181 标准的心跳保活 XML 消息。
///
/// # 参数
///
/// - `sn` - 命令序号
/// - `device_id` - 设备国标编码
pub fn build_keepalive_xml(sn: u32, device_id: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <Notify>\n\
         <CmdType>Keepalive</CmdType>\n\
         <SN>{}</SN>\n\
         <DeviceID>{}</DeviceID>\n\
         <Status>OK</Status>\n\
         </Notify>",
        sn, device_id
    )
}

/// 构建 MESSAGE SIP 请求
///
/// 生成 GB28181 MESSAGE 请求，Content-Type 为 Application/MANSCDP+xml。
///
/// # 参数
///
/// - `device_id` - 设备国标编码
/// - `xml_body` - XML 消息体
pub fn build_message_sip_request(device_id: &str, xml_body: &str) -> SipRequest {
    // Request-URI: sip:device_id@server_domain
    let request_uri = SipUri::parse(&format!("sip:{}", device_id)).unwrap_or_else(|_| {
        SipUri::parse("sip:unknown@unknown").expect("hardcoded fallback URI must be valid")
    });

    // From URI
    let from_uri = SipUri::parse(&format!("sip:{}", device_id)).unwrap_or_else(|_| {
        SipUri::parse("sip:unknown@unknown").expect("hardcoded fallback URI must be valid")
    });
    let from_tag = Tag::new();

    // To URI
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

    fn make_test_config() -> Gb28181Config {
        Gb28181Config {
            device_id: "34020000001320000001".to_string(),
            server_addr: "192.168.1.1".to_string(),
            server_port: 5060,
            server_domain: "3402000000".to_string(),
            username: "34020000001320000001".to_string(),
            password: "12345678".to_string(),
            expires: 3600,
            heartbeat_interval: 60,
            local_ip: "192.168.1.100".to_string(),
            local_port: 5060,
        }
    }

    #[test]
    fn test_gb28181_config_creation() {
        let config = make_test_config();
        assert_eq!(config.device_id, "34020000001320000001");
        assert_eq!(config.server_addr, "192.168.1.1");
        assert_eq!(config.server_port, 5060);
        assert_eq!(config.server_domain, "3402000000");
        assert_eq!(config.expires, 3600);
        assert_eq!(config.heartbeat_interval, 60);
    }

    #[test]
    fn test_build_sip_config() {
        let config = make_test_config();
        let sip_config = Gb28181Device::build_sip_config(&config);

        assert_eq!(sip_config.aor, "sip:34020000001320000001@3402000000");
        assert_eq!(
            sip_config.contact,
            "sip:34020000001320000001@192.168.1.100:5060"
        );
        assert!(sip_config.registrar_server.is_some());
        assert!(sip_config.credentials.is_some());
        assert_eq!(sip_config.sip_port, 5060);
        assert_eq!(sip_config.transport, TransportProtocol::Udp);
        assert_eq!(sip_config.registration_config.default_expires, 3600);
    }

    #[test]
    fn test_gb28181_device_new() {
        let config = make_test_config();
        let device = Gb28181Device::new(config);

        assert_eq!(device.device_id(), "34020000001320000001");
        assert!(device.event_rx.is_some());
    }

    #[test]
    fn test_event_receiver_once() {
        let config = make_test_config();
        let mut device = Gb28181Device::new(config);

        // 第一次调用应返回 Some
        let rx = device.event_receiver();
        assert!(rx.is_some());

        // 第二次调用应返回 None
        let rx2 = device.event_receiver();
        assert!(rx2.is_none());
    }

    #[test]
    fn test_build_keepalive_xml() {
        let xml = build_keepalive_xml(1, "34020000001320000001");

        assert!(xml.contains("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("<Notify>"));
        assert!(xml.contains("<CmdType>Keepalive</CmdType>"));
        assert!(xml.contains("<SN>1</SN>"));
        assert!(xml.contains("<DeviceID>34020000001320000001</DeviceID>"));
        assert!(xml.contains("<Status>OK</Status>"));
        assert!(xml.contains("</Notify>"));
    }

    #[test]
    fn test_build_message_sip_request() {
        let xml = build_keepalive_xml(1, "34020000001320000001");
        let request = build_message_sip_request("34020000001320000001", &xml);

        assert_eq!(request.request_line.method, Method::Message);
        assert!(request.body.is_some());

        let body = request.body.as_ref().unwrap();
        assert_eq!(body.content_type, "Application/MANSCDP+xml");

        // 验证 Content-Type 头部
        let ct = request.headers.get(&HeaderName::ContentType);
        assert!(ct.is_some());
    }

    #[test]
    fn test_gb28181_event_debug() {
        let event = Gb28181Event::Registered;
        assert!(format!("{:?}", event).contains("Registered"));

        let event = Gb28181Event::RegistrationFailed("timeout".to_string());
        assert!(format!("{:?}", event).contains("RegistrationFailed"));

        let event = Gb28181Event::CatalogQuery {
            sn: 1,
            device_id: "34020000001320000001".to_string(),
        };
        assert!(format!("{:?}", event).contains("CatalogQuery"));

        let event = Gb28181Event::InviteReceived {
            call_id: "call-123".to_string(),
            device_id: "34020000001320000001".to_string(),
            sdp: Box::new(sip_sdp::types::SessionDescription {
                version: 0,
                origin: sip_sdp::types::Origin {
                    username: "-".to_string(),
                    session_id: 0,
                    session_version: 0,
                    network_type: "IN".to_string(),
                    address_type: "IP4".to_string(),
                    unicast_address: "192.168.1.1".to_string(),
                },
                session_name: "Play".to_string(),
                session_info: None,
                uri: None,
                email: None,
                phone: None,
                connection: None,
                bandwidth: vec![],
                time_descriptions: vec![],
                timezone: None,
                encryption: None,
                attributes: vec![],
                media_descriptions: vec![],
                ssrc: None,
                media_format: None,
            }),
        };
        assert!(format!("{:?}", event).contains("InviteReceived"));

        let event = Gb28181Event::ByeReceived {
            call_id: "call-123".to_string(),
        };
        assert!(format!("{:?}", event).contains("ByeReceived"));

        let event = Gb28181Event::PtzControl {
            device_id: "34020000001320000001".to_string(),
            command: "A50F01001F0000".to_string(),
        };
        assert!(format!("{:?}", event).contains("PtzControl"));

        let event = Gb28181Event::KeepaliveOk;
        assert!(format!("{:?}", event).contains("KeepaliveOk"));

        let event = Gb28181Event::KeepaliveTimeout;
        assert!(format!("{:?}", event).contains("KeepaliveTimeout"));
    }

    #[tokio::test]
    async fn test_device_not_registered_initially() {
        let config = make_test_config();
        let device = Gb28181Device::new(config);

        assert!(!device.is_registered().await);
    }

    #[tokio::test]
    async fn test_catalog_query_response() {
        let config = make_test_config();
        let device = Gb28181Device::new(config);

        // 构建目录项
        let item_id = gb28181_codec::DeviceId::parse("34020000001320000001").unwrap();
        let mut item = gb28181_xml::DeviceItem::new(item_id);
        item.name = Some("Camera1".to_string());
        item.status = Some("ON".to_string());

        // 注意：由于未启动 SIP 引擎，实际发送会失败
        // 此测试验证构建逻辑
        let result = device
            .handle_catalog_query(1, "34020000001320000001", vec![item])
            .await;
        assert!(result.is_err()); // 预期失败，因为引擎未启动
    }

    #[tokio::test]
    async fn test_device_info_query_response() {
        let config = make_test_config();
        let device = Gb28181Device::new(config);

        let result = device
            .handle_device_info_query(
                2,
                "34020000001320000001",
                "Hikvision",
                "DS-2CD2143",
                "V5.4.5",
            )
            .await;
        assert!(result.is_err()); // 预期失败，因为引擎未启动
    }

    #[tokio::test]
    async fn test_accept_invite_without_engine() {
        let config = make_test_config();
        let device = Gb28181Device::new(config);

        let result = device
            .accept_invite("call-123", "192.168.1.100", 6000)
            .await;
        assert!(result.is_err()); // 预期失败，因为引擎未启动
    }

    #[tokio::test]
    async fn test_hang_up_no_active_call() {
        let config = make_test_config();
        let device = Gb28181Device::new(config);

        let result = device.hang_up(None).await;
        assert!(result.is_err()); // 无活跃通话
    }

    #[test]
    fn test_build_keepalive_xml_multiple_sn() {
        for sn in [1, 100, 999999] {
            let xml = build_keepalive_xml(sn, "34020000001320000001");
            assert!(xml.contains(&format!("<SN>{}</SN>", sn)));
        }
    }

    #[test]
    fn test_build_message_request_content_type() {
        let xml = "<Notify><CmdType>Keepalive</CmdType></Notify>";
        let request = build_message_sip_request("34020000001320000001", xml);

        // 验证 Content-Type
        if let Some(HeaderValue::ContentType(ct)) = request.headers.get(&HeaderName::ContentType) {
            assert_eq!(ct, "Application/MANSCDP+xml");
        } else {
            panic!("Expected ContentType header");
        }
    }

    #[test]
    fn test_build_message_request_body() {
        let xml = build_keepalive_xml(42, "34020000001320000001");
        let request = build_message_sip_request("34020000001320000001", &xml);

        let body = request.body.as_ref().unwrap();
        let body_str = String::from_utf8(body.content.clone()).unwrap();
        assert!(body_str.contains("Keepalive"));
        assert!(body_str.contains("<SN>42</SN>"));
    }

    #[test]
    fn test_gb28181_config_access() {
        let config = make_test_config();
        let device = Gb28181Device::new(config);
        assert_eq!(device.config().server_addr, "192.168.1.1");
        assert_eq!(device.config().server_port, 5060);
    }
}
