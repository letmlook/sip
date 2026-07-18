//! SipEngine 主入口
//!
//! SIP 协议栈的主入口，协调传输层、事务层、对话层和注册层，
//! 提供高层 API 供应用层调用。
//!
//! # 架构
//!
//! ```text
//! Application Layer
//!       ↕ (SipEvent)
//!    SipEngine
//!       ↕
//! ┌────┼────┬────────┐
//! UAC  UAS  Dialog   Registration
//!       ↕
//! ┌────┼────┬────────┐
//! Transport Transaction Dialog Registration
//! ```

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use siprs_core::config::SipConfig;
use siprs_core::metrics::SipMetrics;
use siprs_core::{SipError, TransportProtocol};
use siprs_dialog::DialogManager;
use siprs_message::{Method, SipMessage, SipRequest, SipResponse};
use siprs_registration::{RegistrationId, RegistrationManager};
use siprs_transaction::{TransactionEvent, TransactionId, TransactionManager};
use siprs_transport::{TransportEvent, TransportManager};
use tokio::sync::{mpsc, Mutex};

use crate::config::UaConfig;
use crate::event::{CallTerminationReason, SipEvent};
use crate::uac::Uac;
use crate::uas::Uas;

// ============================================================================
// CallId 映射
// ============================================================================

/// Call-ID 到 TransactionId 的映射
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CallTransactionMapping {
    /// INVITE 事务 ID
    invite_transaction_id: TransactionId,
    /// Call-ID
    call_id: String,
}

// ============================================================================
// SipEngine - SIP 引擎
// ============================================================================

/// SIP 引擎
///
/// SIP 协议栈的主入口，协调传输层、事务层、对话层和注册层。
/// 提供高层 API 供应用层调用，包括呼叫控制、注册管理等。
///
/// # 示例
///
/// ```ignore
/// use siprs_ua::engine::SipEngine;
/// use siprs_core::config::SipConfig;
///
/// let config = SipConfig::builder()
///     .aor("sip:alice@example.com")
///     .contact("sip:alice@192.168.1.1:5060")
///     .build()
///     .unwrap();
///
/// let mut engine = SipEngine::new(config);
/// engine.start().await.unwrap();
///
/// // 获取事件接收器
/// let mut event_rx = engine.event_receiver().unwrap();
///
/// // 发起呼叫
/// let call_id = engine.make_call("sip:bob@example.com", None, None).await.unwrap();
/// ```
pub struct SipEngine {
    /// SIP 配置
    config: SipConfig,
    /// UA 配置
    ua_config: UaConfig,
    /// 传输管理器
    transport: Arc<Mutex<TransportManager>>,
    /// 事务管理器
    transaction: Arc<Mutex<TransactionManager>>,
    /// 对话管理器
    dialog: Arc<DialogManager>,
    /// 注册管理器
    registration: Arc<RegistrationManager>,
    /// UAC 管理器
    uac: Arc<Uac>,
    /// UAS 管理器
    uas: Arc<Uas>,
    /// 运行指标
    metrics: Arc<SipMetrics>,
    /// UA 事件发送端
    event_tx: mpsc::UnboundedSender<SipEvent>,
    /// UA 事件接收端
    event_rx: Option<mpsc::UnboundedReceiver<SipEvent>>,
    /// Call-ID 到事务 ID 的映射
    call_transaction_map: Arc<Mutex<HashMap<String, CallTransactionMapping>>>,
    /// Call-ID 到 RegistrationId 的映射
    call_registration_map: Arc<Mutex<HashMap<String, RegistrationId>>>,
    /// 是否已启动
    running: bool,
    /// 后台任务句柄
    event_loop_handle: Option<tokio::task::JoinHandle<()>>,
    /// 事务事件接收端（将在 start_event_loop 中消费）
    transaction_event_rx: Option<mpsc::UnboundedReceiver<TransactionEvent>>,
    /// 对话事件接收端（将在 start_event_loop 中消费）
    dialog_event_rx: Option<mpsc::UnboundedReceiver<siprs_dialog::DialogEvent>>,
    /// 注册事件接收端（将在 start_event_loop 中消费）
    reg_event_rx: Option<mpsc::UnboundedReceiver<siprs_registration::RegistrationEvent>>,
}

impl SipEngine {
    /// 创建新的 SIP 引擎
    ///
    /// # 参数
    ///
    /// - `config` - SIP 配置
    pub fn new(config: SipConfig) -> Self {
        let metrics = Arc::new(SipMetrics::new());

        // 创建事件通道
        let (event_tx, event_rx) = mpsc::unbounded_channel::<SipEvent>();

        // 创建事务事件通道（接收端将在事件循环中消费）
        let (transaction_event_tx, transaction_event_rx) =
            mpsc::unbounded_channel::<TransactionEvent>();

        // 创建对话管理器（事件接收端将在事件循环中消费）
        let (dialog, dialog_event_rx) = DialogManager::with_event_channel(Arc::clone(&metrics));

        // 创建注册管理器（事件接收端将在事件循环中消费）
        let (reg_event_tx, reg_event_rx) = mpsc::unbounded_channel();
        let registration = RegistrationManager::new(
            config.registration_config.clone(),
            config.credentials.clone(),
            reg_event_tx,
            Arc::clone(&metrics),
        );

        // 创建传输管理器
        let transport = TransportManager::new(
            config.transport_config.clone(),
            config.tls_config.clone(),
            Arc::clone(&metrics),
        );

        // 创建事务管理器
        let transaction = TransactionManager::new(
            config.transaction_config.clone(),
            transaction_event_tx,
            Arc::clone(&metrics),
        );

        let ua_config = UaConfig::default();

        let uac = Uac::new(config.clone(), ua_config.clone(), Arc::clone(&metrics));
        let uas = Uas::new(config.clone(), ua_config.clone(), Arc::clone(&metrics));

        Self {
            config,
            ua_config,
            transport: Arc::new(Mutex::new(transport)),
            transaction: Arc::new(Mutex::new(transaction)),
            dialog: Arc::new(dialog),
            registration: Arc::new(registration),
            uac: Arc::new(uac),
            uas: Arc::new(uas),
            metrics,
            event_tx,
            event_rx: Some(event_rx),
            call_transaction_map: Arc::new(Mutex::new(HashMap::new())),
            call_registration_map: Arc::new(Mutex::new(HashMap::new())),
            running: false,
            event_loop_handle: None,
            // 保存事件接收端，将在 start_event_loop 中消费
            transaction_event_rx: Some(transaction_event_rx),
            dialog_event_rx: Some(dialog_event_rx),
            reg_event_rx: Some(reg_event_rx),
        }
    }

    /// 使用自定义 UA 配置创建 SIP 引擎
    pub fn with_ua_config(config: SipConfig, ua_config: UaConfig) -> Self {
        let mut engine = Self::new(config);
        engine.ua_config = ua_config;
        // 重建 UAC 和 UAS 以使用新配置
        engine.uac = Arc::new(Uac::new(
            engine.config.clone(),
            engine.ua_config.clone(),
            Arc::clone(&engine.metrics),
        ));
        engine.uas = Arc::new(Uas::new(
            engine.config.clone(),
            engine.ua_config.clone(),
            Arc::clone(&engine.metrics),
        ));
        engine
    }

    /// 启动 SIP 协议栈
    ///
    /// 启动传输层监听、事务层和事件循环。
    ///
    /// # 返回
    ///
    /// 成功返回 Ok(())，失败返回 SipError。
    pub async fn start(&mut self) -> Result<(), SipError> {
        if self.running {
            tracing::warn!("SipEngine: already running");
            return Ok(());
        }

        // 启动传输层
        let bind_addr: SocketAddr = format!("{}:{}", self.config.bind_ip, self.config.sip_port)
            .parse()
            .map_err(|e: std::net::AddrParseError| {
                SipError::Config(siprs_core::ConfigError::InvalidValue {
                    field: "bind_addr".to_string(),
                    value: format!("{}:{}", self.config.bind_ip, self.config.sip_port),
                    reason: e.to_string(),
                })
            })?;

        self.transport
            .lock()
            .await
            .start(bind_addr)
            .await
            .map_err(SipError::Transport)?;

        // 启动事务层
        self.transaction.lock().await.start();

        // 启动事件循环
        self.start_event_loop().await;

        self.running = true;
        tracing::info!("SipEngine: started on port {}", self.config.sip_port);
        Ok(())
    }

    /// 停止 SIP 协议栈
    ///
    /// 终止所有活跃对话和事务，关闭传输连接。
    pub async fn stop(&mut self) {
        if !self.running {
            return;
        }

        // 停止事件循环
        if let Some(handle) = self.event_loop_handle.take() {
            handle.abort();
        }

        // 停止事务层
        self.transaction.lock().await.stop();

        // 停止传输层
        self.transport.lock().await.stop().await;

        // 清理映射
        self.call_transaction_map.lock().await.clear();
        self.call_registration_map.lock().await.clear();

        self.running = false;
        tracing::info!("SipEngine: stopped");
    }

    /// 发起呼叫
    ///
    /// 构建 INVITE 请求并通过事务层发送。
    ///
    /// # 参数
    ///
    /// - `target` - 被叫方 URI
    /// - `body` - 会话描述（可选）
    /// - `content_type` - 内容类型（可选）
    ///
    /// # 返回
    ///
    /// 返回 Call-ID。
    pub async fn make_call(
        &self,
        target: &str,
        body: Option<Vec<u8>>,
        content_type: Option<&str>,
    ) -> Result<String, SipError> {
        // 通过 UAC 构建 INVITE
        let (call_id, invite) = self
            .uac
            .make_call(target, body, content_type)
            .await
            .map_err(|e| SipError::Engine(format!("failed to build INVITE: {}", e)))?;

        // 通过传输层发送 INVITE
        self.send_message(&SipMessage::Request(invite.clone()), target)
            .await?;

        tracing::info!("SipEngine: made call to {} (call_id={})", target, call_id);
        Ok(call_id)
    }

    /// 接听来电
    ///
    /// 发送 200 OK 响应。
    ///
    /// # 参数
    ///
    /// - `call_id` - 呼叫标识
    /// - `body` - 会话描述（可选）
    /// - `content_type` - 内容类型（可选）
    pub async fn answer_call(
        &self,
        call_id: &str,
        body: Option<Vec<u8>>,
        content_type: Option<&str>,
    ) -> Result<(), SipError> {
        let response = self
            .uas
            .answer_call(call_id, body, content_type)
            .await
            .ok_or_else(|| {
                SipError::Engine(format!("no incoming call found for call_id={}", call_id))
            })?;

        // 通过事务层发送响应
        self.transaction.lock().await.send_response(response)?;

        tracing::info!("SipEngine: answered call {}", call_id);
        Ok(())
    }

    /// 拒绝来电
    ///
    /// 发送拒绝响应。
    ///
    /// # 参数
    ///
    /// - `call_id` - 呼叫标识
    /// - `status_code` - 拒绝状态码（可选，默认 486）
    /// - `reason` - 原因短语（可选）
    pub async fn reject_call(
        &self,
        call_id: &str,
        status_code: Option<u16>,
        reason: Option<&str>,
    ) -> Result<(), SipError> {
        let response = self
            .uas
            .reject_call(call_id, status_code, reason)
            .await
            .ok_or_else(|| {
                SipError::Engine(format!("no incoming call found for call_id={}", call_id))
            })?;

        // 通过事务层发送响应
        self.transaction.lock().await.send_response(response)?;

        tracing::info!("SipEngine: rejected call {}", call_id);
        Ok(())
    }

    /// 挂断通话
    ///
    /// 在对话内发送 BYE 请求。
    ///
    /// # 参数
    ///
    /// - `call_id` - 呼叫标识
    pub async fn hang_up(&self, call_id: &str) -> Result<(), SipError> {
        // 查找对话
        let dialog_ids = self.dialog.dialog_ids().await;
        let target_dialog = dialog_ids
            .iter()
            .find(|did| did.call_id == call_id || did.to_string().contains(call_id));

        if let Some(dialog_id) = target_dialog {
            // 在对话内构建 BYE 请求
            let bye_request = self
                .dialog
                .build_in_dialog_request(dialog_id, Method::Bye)
                .await?;

            // 通过传输层发送 BYE
            let target = self
                .dialog
                .find_dialog(dialog_id)
                .await
                .ok()
                .and_then(|d| d.remote_target.map(|uri| uri.to_string()))
                .unwrap_or_else(|| call_id.to_string());

            self.send_message(&SipMessage::Request(bye_request), &target)
                .await?;

            // 终止对话
            self.dialog
                .terminate_dialog(dialog_id, "BYE sent".to_string())
                .await?;

            tracing::info!("SipEngine: hung up call {}", call_id);
            Ok(())
        } else {
            Err(SipError::Engine(format!(
                "no active dialog found for call_id={}",
                call_id
            )))
        }
    }

    /// 取消呼叫
    ///
    /// INVITE 未收到最终响应时发送 CANCEL。
    ///
    /// # 参数
    ///
    /// - `call_id` - 呼叫标识
    pub async fn cancel_call(&self, call_id: &str) -> Result<(), SipError> {
        let cancel_request = self.uac.cancel_call(call_id).await.ok_or_else(|| {
            SipError::Engine(format!("no pending call found for call_id={}", call_id))
        })?;

        // 通过传输层发送 CANCEL
        let target = self
            .uac
            .get_pending_target(call_id)
            .await
            .unwrap_or_else(|| call_id.to_string());

        self.send_message(&SipMessage::Request(cancel_request), &target)
            .await?;

        tracing::info!("SipEngine: cancelled call {}", call_id);
        Ok(())
    }

    /// 发起注册
    ///
    /// 向注册服务器发送 REGISTER 请求。
    ///
    /// # 返回
    ///
    /// 返回注册标识。
    pub async fn register(&self) -> Result<String, SipError> {
        let registrar = self.config.registrar_server.as_deref().or(self
            .config
            .registration_config
            .registrar_server
            .as_deref());

        let (reg_id, request) = self
            .registration
            .register(&self.config.aor, &self.config.contact, registrar)
            .await?;

        // 通过传输层发送 REGISTER
        let target = registrar
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.config.aor.clone());

        self.send_message(&SipMessage::Request(request), &target)
            .await?;

        // 记录映射
        self.call_registration_map
            .lock()
            .await
            .insert(reg_id.to_string(), reg_id.clone());

        tracing::info!("SipEngine: registered (reg_id={})", reg_id);
        Ok(reg_id.to_string())
    }

    /// 发起注销
    ///
    /// 发送 REGISTER (Expires=0) 请求。
    ///
    /// # 参数
    ///
    /// - `registration_id` - 注册标识
    pub async fn unregister(&self, registration_id: &str) -> Result<(), SipError> {
        let reg_id = RegistrationId(registration_id.to_string());

        let request = self.registration.unregister(&reg_id).await?;

        // 通过传输层发送 REGISTER
        let registrar = self
            .config
            .registrar_server
            .as_deref()
            .or(self.config.registration_config.registrar_server.as_deref())
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.config.aor.clone());

        self.send_message(&SipMessage::Request(request), &registrar)
            .await?;

        tracing::info!("SipEngine: unregistered (reg_id={})", registration_id);
        Ok(())
    }

    /// 获取事件接收器
    ///
    /// 返回 UA 事件接收端。此方法只能调用一次。
    pub fn event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<SipEvent>> {
        self.event_rx.take()
    }

    /// 获取运行指标引用
    pub fn metrics(&self) -> Arc<SipMetrics> {
        Arc::clone(&self.metrics)
    }

    /// 判断是否正在运行
    pub fn is_running(&self) -> bool {
        self.running
    }

    // ========================================================================
    // 内部方法

    /// 启动事件循环
    async fn start_event_loop(&mut self) {
        // 获取传输层事件流
        let transport_event_rx = self.transport.lock().await.message_stream();

        // 取出事务/对话/注册事件接收端
        let transaction_event_rx = self.transaction_event_rx.take();
        let dialog_event_rx = self.dialog_event_rx.take();
        let reg_event_rx = self.reg_event_rx.take();

        let event_tx = self.event_tx.clone();
        let transport = Arc::clone(&self.transport);
        let transaction = Arc::clone(&self.transaction);
        let dialog = Arc::clone(&self.dialog);
        let uac = Arc::clone(&self.uac);
        let uas = Arc::new(Uas::new(
            self.config.clone(),
            self.ua_config.clone(),
            Arc::clone(&self.metrics),
        ));
        let metrics = Arc::clone(&self.metrics);
        let call_transaction_map = Arc::clone(&self.call_transaction_map);

        // 启动事务事件消费任务
        if let Some(mut tx_event_rx) = transaction_event_rx {
            let event_tx_clone = event_tx.clone();
            let metrics_clone = Arc::clone(&metrics);
            tokio::spawn(async move {
                while let Some(event) = tx_event_rx.recv().await {
                    match event {
                        TransactionEvent::Timeout { transaction_id } => {
                            tracing::warn!("SipEngine: transaction timeout: {}", transaction_id);
                            metrics_clone.inc_transaction_timeouts();
                            let _ = event_tx_clone.send(SipEvent::Error(SipError::Transaction(
                                siprs_core::TransactionError::Timeout {
                                    id: transaction_id.to_string(),
                                },
                            )));
                        }
                        TransactionEvent::TransportError {
                            transaction_id,
                            error,
                        } => {
                            tracing::warn!(
                                "SipEngine: transaction {} transport error: {}",
                                transaction_id,
                                error
                            );
                            let _ =
                                event_tx_clone.send(SipEvent::Error(SipError::Transport(error)));
                        }
                        TransactionEvent::Terminated {
                            transaction_id,
                            reason,
                        } => {
                            tracing::debug!(
                                "SipEngine: transaction {} terminated: {:?}",
                                transaction_id,
                                reason
                            );
                        }
                        TransactionEvent::ProvisionalResponse {
                            transaction_id,
                            response,
                        } => {
                            tracing::debug!(
                                "SipEngine: transaction {} provisional response {}",
                                transaction_id,
                                response.status_line.status_code
                            );
                        }
                        TransactionEvent::FinalResponse {
                            transaction_id,
                            response,
                        } => {
                            tracing::debug!(
                                "SipEngine: transaction {} final response {}",
                                transaction_id,
                                response.status_line.status_code
                            );
                        }
                        TransactionEvent::RequestReceived {
                            transaction_id,
                            request,
                            source_addr,
                        } => {
                            tracing::debug!(
                                "SipEngine: transaction {} request {} from {}",
                                transaction_id,
                                request.request_line.method,
                                source_addr
                            );
                        }
                    }
                }
                tracing::debug!("SipEngine: transaction event stream closed");
            });
        }

        // 启动对话事件消费任务
        if let Some(mut dlg_event_rx) = dialog_event_rx {
            let event_tx_clone = event_tx.clone();
            tokio::spawn(async move {
                while let Some(event) = dlg_event_rx.recv().await {
                    tracing::debug!("SipEngine: dialog event: {:?}", event);
                    let _ = event_tx_clone.send(SipEvent::DialogEvent(event));
                }
                tracing::debug!("SipEngine: dialog event stream closed");
            });
        }

        // 启动注册事件消费任务
        if let Some(mut reg_event_rx) = reg_event_rx {
            let event_tx_clone = event_tx.clone();
            tokio::spawn(async move {
                while let Some(event) = reg_event_rx.recv().await {
                    tracing::debug!("SipEngine: registration event: {:?}", event);
                    let _ = event_tx_clone.send(SipEvent::RegistrationEvent(event));
                }
                tracing::debug!("SipEngine: registration event stream closed");
            });
        }

        let handle = tokio::spawn(async move {
            // 如果有传输事件流，处理传输事件
            if let Some(mut transport_rx) = transport_event_rx {
                tracing::info!("SipEngine: event loop started");

                loop {
                    tokio::select! {
                        // 处理传输层事件
                        transport_event = transport_rx.recv() => {
                            match transport_event {
                                Some(event) => {
                                    Self::handle_transport_event(
                                        event,
                                        &transport,
                                        &transaction,
                                        &dialog,
                                        &uac,
                                        &uas,
                                        &event_tx,
                                        &metrics,
                                        &call_transaction_map,
                                    ).await;
                                }
                                None => {
                                    tracing::info!("SipEngine: transport event stream closed");
                                    break;
                                }
                            }
                        }
                    }
                }

                tracing::info!("SipEngine: event loop stopped");
            }
        });

        self.event_loop_handle = Some(handle);
    }

    /// 处理传输层事件
    #[allow(clippy::too_many_arguments)]
    async fn handle_transport_event(
        event: TransportEvent,
        _transport: &Arc<Mutex<TransportManager>>,
        transaction: &Arc<Mutex<TransactionManager>>,
        dialog: &Arc<DialogManager>,
        uac: &Arc<Uac>,
        uas: &Arc<Uas>,
        event_tx: &mpsc::UnboundedSender<SipEvent>,
        metrics: &Arc<SipMetrics>,
        call_transaction_map: &Arc<Mutex<HashMap<String, CallTransactionMapping>>>,
    ) {
        match event {
            TransportEvent::Message(received) => {
                let received = *received;
                metrics.inc_messages_received();

                match received.message {
                    SipMessage::Request(request) => {
                        Self::handle_incoming_request(
                            request,
                            received.source_addr,
                            received.transport,
                            transaction,
                            dialog,
                            uas,
                            event_tx,
                            call_transaction_map,
                        )
                        .await;
                    }
                    SipMessage::Response(response) => {
                        Self::handle_incoming_response(
                            response,
                            transaction,
                            dialog,
                            uac,
                            event_tx,
                            call_transaction_map,
                        )
                        .await;
                    }
                }
            }
            TransportEvent::ConnectionEstablished(addr, proto) => {
                tracing::debug!("SipEngine: connection established to {} ({})", addr, proto);
            }
            TransportEvent::ConnectionLost(addr, proto) => {
                tracing::debug!("SipEngine: connection lost to {} ({})", addr, proto);
            }
            TransportEvent::Error(error) => {
                tracing::error!("SipEngine: transport error: {}", error);
                let _ = event_tx.send(SipEvent::Error(SipError::Transport(error)));
            }
        }
    }

    /// 处理收到的请求
    #[allow(clippy::too_many_arguments)]
    async fn handle_incoming_request(
        request: SipRequest,
        source_addr: SocketAddr,
        transport_proto: TransportProtocol,
        transaction: &Arc<Mutex<TransactionManager>>,
        dialog: &Arc<DialogManager>,
        uas: &Arc<Uas>,
        event_tx: &mpsc::UnboundedSender<SipEvent>,
        call_transaction_map: &Arc<Mutex<HashMap<String, CallTransactionMapping>>>,
    ) {
        let method = request.request_line.method.clone();

        // 提取 Call-ID
        let call_id = request
            .headers
            .get(&siprs_message::HeaderName::CallId)
            .and_then(|v| v.as_call_id())
            .map(|cid| cid.0.clone());

        match method {
            Method::Invite => {
                // 交给事务层创建服务端事务
                let result = transaction.lock().await.handle_request(
                    request.clone(),
                    source_addr,
                    transport_proto,
                );

                if let Ok(tx_id) = result {
                    // 记录映射
                    if let Some(ref cid) = call_id {
                        call_transaction_map.lock().await.insert(
                            cid.clone(),
                            CallTransactionMapping {
                                invite_transaction_id: tx_id,
                                call_id: cid.clone(),
                            },
                        );
                    }

                    // 生成来电事件
                    let event = uas.handle_invite(&request).await;
                    let _ = event_tx.send(event);
                }
            }
            Method::Ack => {
                // ACK 处理：交给对话层
                let _ = dialog.handle_in_dialog_request(&request).await;
            }
            Method::Bye => {
                // BYE 处理：交给对话层
                if let Ok((dialog_id, _)) = dialog.handle_in_dialog_request(&request).await {
                    // 发送 200 OK (BYE)
                    let _ = event_tx.send(SipEvent::CallTerminated {
                        call_id: dialog_id.call_id.clone(),
                        reason: CallTerminationReason::NormalBye,
                    });
                }
            }
            Method::Cancel => {
                // CANCEL 处理
                if let Some(ref cid) = call_id {
                    if let Some((event, response)) = uas.handle_cancel(cid).await {
                        // 通过事务层发送 200 OK (CANCEL)
                        let _ = transaction.lock().await.send_response(response);
                        let _ = event_tx.send(event);
                    }
                }
            }
            _ => {
                // 其他请求交给事务层
                let _ =
                    transaction
                        .lock()
                        .await
                        .handle_request(request, source_addr, transport_proto);
            }
        }
    }

    /// 处理收到的响应
    async fn handle_incoming_response(
        response: SipResponse,
        transaction: &Arc<Mutex<TransactionManager>>,
        dialog: &Arc<DialogManager>,
        uac: &Arc<Uac>,
        event_tx: &mpsc::UnboundedSender<SipEvent>,
        call_transaction_map: &Arc<Mutex<HashMap<String, CallTransactionMapping>>>,
    ) {
        // 交给事务层处理
        let _ = transaction.lock().await.handle_response(response.clone());

        // 提取 Call-ID
        let call_id = response
            .headers
            .get(&siprs_message::HeaderName::CallId)
            .and_then(|v| v.as_call_id())
            .map(|cid| cid.0.clone());

        if let Some(ref cid) = call_id {
            // 检查是否为 UAC 待处理的呼叫
            if uac.has_pending_call(cid).await {
                let (events, _request_to_send) = uac.handle_response(cid, &response).await;

                // 发送事件
                for event in events {
                    let _ = event_tx.send(event);
                }
            }

            // 对于 2xx 响应，创建/更新对话
            if response.status_line.status_code.is_success() {
                // 尝试获取原始 INVITE
                if let Some(original_invite) = uac.get_original_invite(cid).await {
                    let _ = dialog.handle_invite_2xx(&original_invite, &response).await;
                }
            }
        }

        // 避免未使用变量警告
        let _ = call_transaction_map;
    }

    /// 发送 SIP 请求
    ///
    /// 构建 SIP 消息并通过传输层发送。
    ///
    /// # 参数
    ///
    /// - `request` - SIP 请求
    /// - `target` - 目标地址
    pub async fn send_request(&self, request: &SipRequest, target: &str) -> Result<(), SipError> {
        self.send_message(&SipMessage::Request(request.clone()), target)
            .await
    }

    /// 通过传输层发送消息
    async fn send_message(&self, message: &SipMessage, target: &str) -> Result<(), SipError> {
        self.transport
            .lock()
            .await
            .send(message, target)
            .await
            .map_err(SipError::Transport)
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use siprs_core::config::SipConfig;

    fn create_test_config() -> SipConfig {
        SipConfig::builder()
            .aor("sip:alice@example.com")
            .contact("sip:alice@192.168.1.1:5060")
            .sip_port(5060)
            .build()
            .unwrap()
    }

    #[test]
    fn test_sip_engine_new() {
        let config = create_test_config();
        let engine = SipEngine::new(config);

        assert!(!engine.is_running());
        assert!(engine.event_rx.is_some());
    }

    #[test]
    fn test_sip_engine_with_ua_config() {
        let config = create_test_config();
        let ua_config = UaConfig {
            call_timeout_secs: 60,
            max_redirects: 10,
            ..UaConfig::default()
        };

        let engine = SipEngine::with_ua_config(config, ua_config);
        assert!(!engine.is_running());
    }

    #[test]
    fn test_event_receiver() {
        let config = create_test_config();
        let mut engine = SipEngine::new(config);

        // 第一次调用应返回 Some
        let rx = engine.event_receiver();
        assert!(rx.is_some());

        // 第二次调用应返回 None
        let rx2 = engine.event_receiver();
        assert!(rx2.is_none());
    }

    #[test]
    fn test_metrics() {
        let config = create_test_config();
        let engine = SipEngine::new(config);

        let metrics = engine.metrics();
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.messages_received, 0);
        assert_eq!(snapshot.messages_sent, 0);
    }

    #[tokio::test]
    async fn test_make_call_without_start() {
        let config = create_test_config();
        let engine = SipEngine::new(config);

        // 未启动时 make_call 应该失败（无法发送消息）
        let result = engine.make_call("sip:bob@example.com", None, None).await;
        // 由于传输层未启动，发送应失败
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_register_without_start() {
        let config = create_test_config();
        let engine = SipEngine::new(config);

        // 未启动时 register 应该失败
        let result = engine.register().await;
        // 由于传输层未启动，发送应失败
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_hang_up_no_dialog() {
        let config = create_test_config();
        let engine = SipEngine::new(config);

        // 无活跃对话时 hang_up 应失败
        let result = engine.hang_up("nonexistent-call-id").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_answer_nonexistent_call() {
        let config = create_test_config();
        let engine = SipEngine::new(config);

        let result = engine.answer_call("nonexistent", None, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_reject_nonexistent_call() {
        let config = create_test_config();
        let engine = SipEngine::new(config);

        let result = engine.reject_call("nonexistent", None, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cancel_nonexistent_call() {
        let config = create_test_config();
        let engine = SipEngine::new(config);

        let result = engine.cancel_call("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_unregister_nonexistent() {
        let config = create_test_config();
        let engine = SipEngine::new(config);

        let result = engine.unregister("nonexistent-reg-id").await;
        assert!(result.is_err());
    }
}
