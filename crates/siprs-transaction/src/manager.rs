//! SIP 事务管理器
//!
//! 统一管理所有客户端和服务端事务，提供事务生命周期管理、
//! 事件分发和定时器调度功能。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use siprs_core::config::TransactionConfig;
use siprs_core::metrics::SipMetrics;
use siprs_core::TransportProtocol;
use siprs_message::{Method, SipRequest, SipResponse};
use tokio::sync::mpsc;
use tokio::time::Duration;

use crate::event::{
    TerminationReason, TimerEvent, TransactionAction, TransactionEvent, TransactionId,
    TransactionKey,
};
use crate::invite_client::InviteClientTransaction;
use crate::invite_server::InviteServerTransaction;
use crate::non_invite_client::NonInviteClientTransaction;
use crate::non_invite_server::NonInviteServerTransaction;
use crate::table::{ClientTransaction, ServerTransaction, TransactionTable};
use crate::timer::TimerManager;

// ============================================================================
// TransactionManager - 事务管理器
// ============================================================================

/// SIP 事务管理器
///
/// 统一管理所有客户端和服务端事务，提供：
/// - 事务创建与生命周期管理
/// - 接收消息分发到对应事务
/// - 定时器调度
/// - 事务事件向 TU 层通知
/// - 100 Trying 自动发送
pub struct TransactionManager {
    /// 事务层配置
    config: TransactionConfig,
    /// 传输层发送端（用于发送 SIP 消息）
    transport_tx: Option<mpsc::UnboundedSender<(Vec<u8>, SocketAddr, TransportProtocol)>>,
    /// 定时器管理器
    timer_manager: TimerManager,
    /// 事务匹配表
    table: TransactionTable,
    /// 事务事件发送端
    event_tx: mpsc::UnboundedSender<TransactionEvent>,
    /// 运行指标
    metrics: Arc<SipMetrics>,
    /// INVITE 服务端事务的 100 Trying 计时器
    trying_timers: HashMap<TransactionId, tokio::task::JoinHandle<()>>,
    /// 是否已启动
    running: bool,
}

impl TransactionManager {
    /// 创建新的事务管理器
    ///
    /// # 参数
    ///
    /// - `config` - 事务层配置
    /// - `event_tx` - 事务事件发送端
    /// - `metrics` - 运行指标收集器
    pub fn new(
        config: TransactionConfig,
        event_tx: mpsc::UnboundedSender<TransactionEvent>,
        metrics: Arc<SipMetrics>,
    ) -> Self {
        let (timer_tx, _timer_rx) = mpsc::unbounded_channel();
        let timer_manager = TimerManager::new(config.clone(), timer_tx);

        Self {
            config,
            transport_tx: None,
            timer_manager,
            table: TransactionTable::new(),
            event_tx,
            metrics,
            trying_timers: HashMap::new(),
            running: false,
        }
    }

    /// 创建带定时器事件通道的事务管理器
    ///
    /// 返回管理器和定时器事件接收端。
    pub fn with_timer_channel(
        config: TransactionConfig,
        event_tx: mpsc::UnboundedSender<TransactionEvent>,
        metrics: Arc<SipMetrics>,
    ) -> (Self, mpsc::UnboundedReceiver<TimerEvent>) {
        let (timer_tx, timer_rx) = mpsc::unbounded_channel();
        let timer_manager = TimerManager::new(config.clone(), timer_tx);

        let manager = Self {
            config,
            transport_tx: None,
            timer_manager,
            table: TransactionTable::new(),
            event_tx,
            metrics,
            trying_timers: HashMap::new(),
            running: false,
        };

        (manager, timer_rx)
    }

    /// 设置传输层发送端
    ///
    /// 用于通过传输层发送 SIP 消息。
    pub fn set_transport_sender(
        &mut self,
        sender: mpsc::UnboundedSender<(Vec<u8>, SocketAddr, TransportProtocol)>,
    ) {
        self.transport_tx = Some(sender);
    }

    /// 启动事务管理器
    pub fn start(&mut self) {
        if self.running {
            tracing::warn!("TransactionManager: already running");
            return;
        }
        self.running = true;
        tracing::info!("TransactionManager: started");
    }

    /// 停止事务管理器
    pub fn stop(&mut self) {
        if !self.running {
            return;
        }

        // 取消所有定时器
        self.timer_manager.cancel_all();

        // 取消所有 100 Trying 计时器
        for (_, handle) in self.trying_timers.drain() {
            handle.abort();
        }

        // 清理所有事务
        self.table.clear();

        self.running = false;
        tracing::info!("TransactionManager: stopped");
    }

    /// 判断是否正在运行
    pub fn is_running(&self) -> bool {
        self.running
    }

    // ========================================================================
    // 发送请求（创建客户端事务）
    // ========================================================================

    /// 发送 SIP 请求（创建客户端事务）
    ///
    /// 根据请求方法创建对应的客户端事务：
    /// - INVITE → InviteClientTransaction
    /// - 其他 → NonInviteClientTransaction
    ///
    /// # 参数
    ///
    /// - `request` - 要发送的 SIP 请求
    /// - `destination` - 目标地址
    /// - `transport` - 传输协议
    ///
    /// # 返回
    ///
    /// 返回创建的事务 ID。
    pub fn send_request(
        &mut self,
        request: SipRequest,
        destination: SocketAddr,
        transport: TransportProtocol,
    ) -> TransactionId {
        let method = request.request_line.method.clone();

        let (client_tx, actions) = if method == Method::Invite {
            let tx = InviteClientTransaction::new(request, destination, transport);
            let id = tx.id().clone();

            // 启动 Timer A（仅 UDP）和 Timer B
            let mut actions = Vec::new();
            if !transport.is_reliable() {
                actions.push(TransactionAction::StartRetransmitTimer {
                    timer: TimerEvent::TimerA {
                        transaction_id: id.clone(),
                    },
                    initial_delay_ms: self.config.t1,
                    max_delay_ms: self.config.t2,
                });
            }
            actions.push(TransactionAction::StartTimer {
                timer: TimerEvent::TimerB {
                    transaction_id: id.clone(),
                },
                delay_ms: 64 * self.config.t1,
            });

            self.metrics.inc_active_client_transactions();
            self.metrics.inc_transactions_created();

            (ClientTransaction::Invite(tx), actions)
        } else {
            let tx = NonInviteClientTransaction::new(request, destination, transport);
            let id = tx.id().clone();

            // 启动 Timer E（仅 UDP）和 Timer F
            let mut actions = Vec::new();
            if !transport.is_reliable() {
                actions.push(TransactionAction::StartRetransmitTimer {
                    timer: TimerEvent::TimerE {
                        transaction_id: id.clone(),
                    },
                    initial_delay_ms: self.config.t1,
                    max_delay_ms: self.config.t2,
                });
            }
            actions.push(TransactionAction::StartTimer {
                timer: TimerEvent::TimerF {
                    transaction_id: id.clone(),
                },
                delay_ms: 64 * self.config.t1,
            });

            self.metrics.inc_active_client_transactions();
            self.metrics.inc_transactions_created();

            (ClientTransaction::NonInvite(tx), actions)
        };

        let id = client_tx.id().clone();
        self.table.insert_client(client_tx);

        // 执行初始动作
        self.execute_actions(actions);

        tracing::debug!(
            "TransactionManager: created client transaction {} for {}",
            id,
            method
        );
        id
    }

    // ========================================================================
    // 发送响应（匹配服务端事务）
    // ========================================================================

    /// 发送 SIP 响应
    ///
    /// 匹配对应的服务端事务并处理响应。
    ///
    /// # 参数
    ///
    /// - `response` - 要发送的 SIP 响应
    ///
    /// # 返回
    ///
    /// 成功返回 Ok(())，未找到匹配事务返回 Err。
    pub fn send_response(&mut self, response: SipResponse) -> Result<(), String> {
        // 通过响应匹配服务端事务
        let key = TransactionKey::from_response(&response);
        if key.is_none() {
            return Err("cannot extract transaction key from response".to_string());
        }
        let key = key.unwrap();

        // 查找服务端事务
        let actions = {
            let server_tx = self.table.server_transactions.get_mut(&key);
            if server_tx.is_none() {
                return Err(format!("no matching server transaction for key {}", key));
            }

            let server_tx = server_tx.unwrap();
            match server_tx {
                ServerTransaction::Invite(tx) => tx.handle_response_from_tu(response),
                ServerTransaction::NonInvite(tx) => tx.handle_response_from_tu(response),
            }
        };

        self.execute_actions(actions);
        Ok(())
    }

    // ========================================================================
    // 接收消息处理
    // ========================================================================

    /// 处理收到的 SIP 响应
    ///
    /// 匹配对应的客户端事务并分发响应。
    pub fn handle_response(&mut self, response: SipResponse) -> Result<(), String> {
        let key = TransactionKey::from_response(&response);
        if key.is_none() {
            return Err("cannot extract transaction key from response".to_string());
        }
        let key = key.unwrap();

        let actions = {
            let client_tx = self.table.client_transactions.get_mut(&key);
            if client_tx.is_none() {
                // 无匹配事务，将消息传递给 TU 核心处理
                tracing::debug!(
                    "TransactionManager: no matching client transaction for response, forwarding to TU"
                );
                return Err(format!("no matching client transaction for key {}", key));
            }

            let client_tx = client_tx.unwrap();
            match client_tx {
                ClientTransaction::Invite(tx) => tx.handle_response(response),
                ClientTransaction::NonInvite(tx) => tx.handle_response(response),
            }
        };

        self.execute_actions(actions);
        self.cleanup_if_needed();
        Ok(())
    }

    /// 处理收到的 SIP 请求
    ///
    /// 匹配对应的服务端事务。无匹配时创建新事务并通知 TU。
    pub fn handle_request(
        &mut self,
        request: SipRequest,
        source_addr: SocketAddr,
        transport: TransportProtocol,
    ) -> Result<TransactionId, String> {
        let method = request.request_line.method.clone();

        // ACK 特殊处理：匹配 INVITE 服务端事务
        if method == Method::Ack {
            return self.handle_ack_request(&request);
        }

        // 尝试匹配已有服务端事务
        let key = TransactionKey::from_request(&request);
        if let Some(ref k) = key {
            if let Some(server_tx) = self.table.server_transactions.get_mut(k) {
                // 已有匹配的事务，处理请求重传
                let id = server_tx.id().clone();
                let actions = match server_tx {
                    ServerTransaction::Invite(tx) => tx.handle_request(&request),
                    ServerTransaction::NonInvite(tx) => tx.handle_request(&request),
                };
                // 释放对 server_tx 的可变借用
                let _ = &server_tx;
                self.execute_actions(actions);
                return Ok(id);
            }
        }

        // 无匹配事务，创建新的服务端事务
        let (server_tx, actions) = if method == Method::Invite {
            let tx = InviteServerTransaction::new(request.clone(), source_addr, transport);
            let id = tx.id().clone();

            self.metrics.inc_active_server_transactions();
            self.metrics.inc_transactions_created();

            // 启动 100 Trying 自动发送计时器
            let actions = vec![TransactionAction::EmitEvent(
                TransactionEvent::RequestReceived {
                    transaction_id: id.clone(),
                    request: request.clone(),
                    source_addr,
                },
            )];

            (ServerTransaction::Invite(tx), actions)
        } else {
            let tx = NonInviteServerTransaction::new(request.clone(), source_addr, transport);
            let id = tx.id().clone();

            self.metrics.inc_active_server_transactions();
            self.metrics.inc_transactions_created();

            let actions = vec![TransactionAction::EmitEvent(
                TransactionEvent::RequestReceived {
                    transaction_id: id.clone(),
                    request: request.clone(),
                    source_addr,
                },
            )];

            (ServerTransaction::NonInvite(tx), actions)
        };

        let id = server_tx.id().clone();
        self.table.insert_server(server_tx);

        // 启动 100 Trying 计时器（仅 INVITE）
        if method == Method::Invite {
            self.start_trying_timer(id.clone());
        }

        self.execute_actions(actions);
        tracing::debug!(
            "TransactionManager: created server transaction {} for {}",
            id,
            method
        );
        Ok(id)
    }

    /// 处理 ACK 请求
    fn handle_ack_request(&mut self, request: &SipRequest) -> Result<TransactionId, String> {
        // 构造匹配键（ACK 匹配 INVITE 事务）
        let mut key = TransactionKey::from_request(request);
        if let Some(ref mut k) = key {
            k.method = Method::Invite;
        }

        if let Some(ref k) = key {
            if let Some(server_tx) = self.table.server_transactions.get_mut(k) {
                let id = server_tx.id().clone();
                let actions = match server_tx {
                    ServerTransaction::Invite(tx) => tx.handle_request(request),
                    ServerTransaction::NonInvite(_) => Vec::new(),
                };
                self.execute_actions(actions);
                return Ok(id);
            }
        }

        // 无匹配的 INVITE 事务，ACK 由 TU 处理（2xx 响应的 ACK）
        tracing::debug!(
            "TransactionManager: ACK with no matching INVITE transaction, forwarding to TU"
        );
        Err("no matching INVITE server transaction for ACK".to_string())
    }

    // ========================================================================
    // 定时器事件处理
    // ========================================================================

    /// 处理定时器事件
    ///
    /// 从定时器事件通道读取事件并分发到对应的事务。
    pub fn handle_timer_event(&mut self, event: TimerEvent) {
        let transaction_id = transaction_id_from_timer(&event);

        // 先尝试客户端事务
        let actions = if let Some(client_tx) = self.table.get_client_by_id_mut(&transaction_id) {
            match client_tx {
                ClientTransaction::Invite(tx) => tx.handle_timer(&event),
                ClientTransaction::NonInvite(tx) => tx.handle_timer(&event),
            }
        } else if let Some(server_tx) = self.table.get_server_by_id_mut(&transaction_id) {
            match server_tx {
                ServerTransaction::Invite(tx) => tx.handle_timer(&event),
                ServerTransaction::NonInvite(tx) => tx.handle_timer(&event),
            }
        } else {
            tracing::trace!(
                "TransactionManager: timer event for unknown transaction {}",
                transaction_id
            );
            return;
        };

        self.execute_actions(actions);
        self.cleanup_if_needed();
    }

    // ========================================================================
    // 取消事务
    // ========================================================================

    /// 取消指定事务
    pub fn cancel(&mut self, transaction_id: &TransactionId) -> Result<(), String> {
        // 先尝试客户端事务
        if let Some(_client_tx) = self.table.get_client_by_id_mut(transaction_id) {
            let actions = vec![
                TransactionAction::CancelTimers {
                    transaction_id: transaction_id.clone(),
                },
                TransactionAction::EmitEvent(TransactionEvent::Terminated {
                    transaction_id: transaction_id.clone(),
                    reason: TerminationReason::UserCancel,
                }),
            ];

            self.execute_actions(actions);
            return Ok(());
        }

        // 尝试服务端事务
        if let Some(_server_tx) = self.table.get_server_by_id_mut(transaction_id) {
            let actions = vec![
                TransactionAction::CancelTimers {
                    transaction_id: transaction_id.clone(),
                },
                TransactionAction::EmitEvent(TransactionEvent::Terminated {
                    transaction_id: transaction_id.clone(),
                    reason: TerminationReason::UserCancel,
                }),
            ];

            self.execute_actions(actions);
            return Ok(());
        }

        Err(format!("transaction not found: {}", transaction_id))
    }

    // ========================================================================
    // 事件流
    // ========================================================================

    /// 获取事务事件发送端的克隆
    pub fn event_sender(&self) -> mpsc::UnboundedSender<TransactionEvent> {
        self.event_tx.clone()
    }

    // ========================================================================
    // 内部方法
    // ========================================================================

    /// 执行事务动作列表
    fn execute_actions(&mut self, actions: Vec<TransactionAction>) {
        for action in actions {
            match action {
                TransactionAction::SendMessage {
                    message,
                    addr,
                    transport,
                } => {
                    self.send_message_via_transport(&message, addr, transport);
                }
                TransactionAction::EmitEvent(event) => {
                    // 更新指标
                    if let TransactionEvent::Timeout { .. } = &event {
                        self.metrics.inc_transaction_timeouts();
                    }
                    let _ = self.event_tx.send(event);
                }
                TransactionAction::StartTimer { timer, delay_ms } => {
                    self.timer_manager
                        .start_timer(timer, Duration::from_millis(delay_ms));
                }
                TransactionAction::StartRetransmitTimer {
                    timer,
                    initial_delay_ms,
                    max_delay_ms,
                } => {
                    let transaction_id = transaction_id_from_timer(&timer);

                    let event_factory: Box<dyn Fn(TransactionId) -> TimerEvent + Send + Sync> =
                        match &timer {
                            TimerEvent::TimerA { .. } => {
                                Box::new(|id| TimerEvent::TimerA { transaction_id: id })
                            }
                            TimerEvent::TimerE { .. } => {
                                Box::new(|id| TimerEvent::TimerE { transaction_id: id })
                            }
                            TimerEvent::TimerG { .. } => {
                                Box::new(|id| TimerEvent::TimerG { transaction_id: id })
                            }
                            _ => {
                                // 其他定时器类型不应使用重传定时器
                                tracing::warn!(
                                    "TransactionManager: unexpected retransmit timer type"
                                );
                                // 回退：使用 TimerA 作为默认
                                Box::new(|id| TimerEvent::TimerA { transaction_id: id })
                            }
                        };

                    self.timer_manager.start_retransmit_timer(
                        event_factory,
                        transaction_id,
                        Duration::from_millis(initial_delay_ms),
                        Duration::from_millis(max_delay_ms),
                    );
                }
                TransactionAction::CancelTimers { transaction_id } => {
                    self.timer_manager.cancel_timers(&transaction_id);
                    // 同时取消 100 Trying 计时器
                    if let Some(handle) = self.trying_timers.remove(&transaction_id) {
                        handle.abort();
                    }
                }
            }
        }
    }

    /// 通过传输层发送消息
    fn send_message_via_transport(
        &self,
        message: &siprs_message::SipMessage,
        addr: SocketAddr,
        transport: TransportProtocol,
    ) {
        if let Some(ref transport_tx) = self.transport_tx {
            // 使用 MessageBuilder 序列化消息
            let builder = siprs_message::MessageBuilder::new();
            match builder.build(message) {
                Ok(bytes) => {
                    if let Err(e) = transport_tx.send((bytes, addr, transport)) {
                        tracing::warn!(
                            "TransactionManager: failed to queue message for transport: {}",
                            e
                        );
                    }
                }
                Err(e) => {
                    tracing::error!("TransactionManager: failed to serialize message: {}", e);
                }
            }
        } else {
            tracing::warn!(
                "TransactionManager: no transport sender configured, cannot send message"
            );
        }
    }

    /// 启动 100 Trying 自动发送计时器
    fn start_trying_timer(&mut self, transaction_id: TransactionId) {
        let event_tx = self.event_tx.clone();
        let tid = transaction_id.clone();
        let trying_timeout = self.config.trying_timeout;

        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(trying_timeout)).await;
            // 100 Trying 超时事件由 manager 的定时器循环处理
            tracing::trace!("100 Trying timer fired for transaction {}", tid);
            let _ = event_tx.send(TransactionEvent::Timeout {
                transaction_id: tid,
            });
        });

        self.trying_timers.insert(transaction_id, handle);
    }

    /// 清理已终止的事务
    fn cleanup_if_needed(&mut self) {
        self.table.cleanup_terminated();
    }
}

/// 从定时器事件中提取事务 ID
fn transaction_id_from_timer(event: &TimerEvent) -> TransactionId {
    match event {
        TimerEvent::TimerA { transaction_id } => transaction_id.clone(),
        TimerEvent::TimerB { transaction_id } => transaction_id.clone(),
        TimerEvent::TimerD { transaction_id } => transaction_id.clone(),
        TimerEvent::TimerE { transaction_id } => transaction_id.clone(),
        TimerEvent::TimerF { transaction_id } => transaction_id.clone(),
        TimerEvent::TimerG { transaction_id } => transaction_id.clone(),
        TimerEvent::TimerH { transaction_id } => transaction_id.clone(),
        TimerEvent::TimerI { transaction_id } => transaction_id.clone(),
        TimerEvent::TimerJ { transaction_id } => transaction_id.clone(),
        TimerEvent::TimerK { transaction_id } => transaction_id.clone(),
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use siprs_core::Host;
    use siprs_core::SipVersion;
    use siprs_message::uri::SipUri;
    use siprs_message::{
        CSeqHeader, CallId, HeaderCollection, HeaderName, HeaderValue, RequestLine, ViaHeader,
    };

    fn create_test_invite() -> SipRequest {
        let uri = SipUri::parse("sip:bob@example.com").unwrap();
        let mut headers = HeaderCollection::new();
        headers.insert(
            HeaderName::Via,
            HeaderValue::Via(ViaHeader::new(
                TransportProtocol::Udp,
                Host::Domain("192.168.1.1".to_string()),
                Some(5060),
            )),
        );
        headers.insert(
            HeaderName::From,
            HeaderValue::FromTo(siprs_message::FromToHeader {
                display_name: None,
                uri: SipUri::parse("sip:alice@example.com").unwrap(),
                tag: Some(siprs_message::Tag::new()),
            }),
        );
        headers.insert(
            HeaderName::To,
            HeaderValue::FromTo(siprs_message::FromToHeader {
                display_name: None,
                uri: SipUri::parse("sip:bob@example.com").unwrap(),
                tag: None,
            }),
        );
        headers.insert(HeaderName::CallId, HeaderValue::CallId(CallId::new()));
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(CSeqHeader::new(1, Method::Invite)),
        );
        headers.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

        SipRequest {
            request_line: RequestLine {
                method: Method::Invite,
                request_uri: uri,
                version: SipVersion,
            },
            headers,
            body: None,
        }
    }

    fn create_test_register() -> SipRequest {
        let uri = SipUri::parse("sip:example.com").unwrap();
        let mut headers = HeaderCollection::new();
        headers.insert(
            HeaderName::Via,
            HeaderValue::Via(ViaHeader::new(
                TransportProtocol::Udp,
                Host::Domain("192.168.1.1".to_string()),
                Some(5060),
            )),
        );
        headers.insert(
            HeaderName::From,
            HeaderValue::FromTo(siprs_message::FromToHeader {
                display_name: None,
                uri: SipUri::parse("sip:alice@example.com").unwrap(),
                tag: Some(siprs_message::Tag::new()),
            }),
        );
        headers.insert(
            HeaderName::To,
            HeaderValue::FromTo(siprs_message::FromToHeader {
                display_name: None,
                uri: SipUri::parse("sip:alice@example.com").unwrap(),
                tag: None,
            }),
        );
        headers.insert(HeaderName::CallId, HeaderValue::CallId(CallId::new()));
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(CSeqHeader::new(1, Method::Register)),
        );
        headers.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

        SipRequest {
            request_line: RequestLine {
                method: Method::Register,
                request_uri: uri,
                version: SipVersion,
            },
            headers,
            body: None,
        }
    }

    #[test]
    fn test_transaction_manager_creation() {
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let config = TransactionConfig::default();
        let metrics = Arc::new(SipMetrics::new());

        let manager = TransactionManager::new(config, event_tx, metrics);
        assert!(!manager.is_running());
    }

    #[test]
    fn test_transaction_manager_start_stop() {
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let config = TransactionConfig::default();
        let metrics = Arc::new(SipMetrics::new());

        let mut manager = TransactionManager::new(config, event_tx, metrics);
        manager.start();
        assert!(manager.is_running());

        manager.stop();
        assert!(!manager.is_running());
    }

    #[tokio::test]
    async fn test_send_invite_creates_client_transaction() {
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let config = TransactionConfig::default();
        let metrics = Arc::new(SipMetrics::new());

        let mut manager = TransactionManager::new(config, event_tx, metrics);
        manager.start();

        let request = create_test_invite();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let id = manager.send_request(request, dest, TransportProtocol::Udp);

        assert!(!id.0.is_empty());
    }

    #[tokio::test]
    async fn test_send_register_creates_client_transaction() {
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let config = TransactionConfig::default();
        let metrics = Arc::new(SipMetrics::new());

        let mut manager = TransactionManager::new(config, event_tx, metrics);
        manager.start();

        let request = create_test_register();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let id = manager.send_request(request, dest, TransportProtocol::Udp);

        assert!(!id.0.is_empty());
    }

    #[tokio::test]
    async fn test_handle_request_creates_server_transaction() {
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let config = TransactionConfig::default();
        let metrics = Arc::new(SipMetrics::new());

        let mut manager = TransactionManager::new(config, event_tx, metrics);
        manager.start();

        let request = create_test_invite();
        let source: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let result = manager.handle_request(request, source, TransportProtocol::Udp);

        assert!(result.is_ok());
        let id = result.unwrap();
        assert!(!id.0.is_empty());
    }

    #[test]
    fn test_with_timer_channel() {
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let config = TransactionConfig::default();
        let metrics = Arc::new(SipMetrics::new());

        let (manager, _timer_rx) =
            TransactionManager::with_timer_channel(config, event_tx, metrics);
        assert!(!manager.is_running());
    }
}
