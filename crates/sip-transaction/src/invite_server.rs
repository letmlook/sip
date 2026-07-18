//! INVITE 服务端事务状态机
//!
//! 按照 RFC 3261 Section 17.2.1 实现 INVITE 服务端事务。
//!
//! # 状态转换
//!
//! ```text
//!                              |INVITE
//!                              |pass INVITE to TU
//!                    INVITE    V send 100 if TU won't in 200ms
//!                    send response+-----------+
//!                                |           |
//!                                |           |
//!                    +-----------+           |
//!                    |           |           |
//!                    |           |           |
//!                    V           |           |
//!              +--------+       |           |
//!              |        |       |           |
//!              | Proceed|       |           |
//!              |  ing   |       |           |
//!              +--------+       |           |
//!                |  |           |           |
//!                |  |           |           |
//!                |  +-----------|-----------+
//!                |  ACK         |  ACK
//!                |  -           |  -
//!                |  send        |  send
//!                |  response    |  response
//!                V              |
//!          +----------+        |
//!          |          |        |
//!          | Completed|        |
//!          |          |        |
//!          +----------+        |
//!                |              |
//!                | Timer H      |
//!                |              |
//!                V              |
//!          +----------+        |
//!          |          |        |
//!          | Confirmed|        |
//!          |          |        |
//!          +----------+        |
//!                |              |
//!                | Timer I      |
//!                |              |
//!                V              |
//!          +----------+        |
//!          |          |        |
//!          |Terminated|        |
//!          |          |        |
//!          +----------+        |
//! ```

use std::net::SocketAddr;

use sip_core::{StatusCode, TransportProtocol};
use sip_message::{Method, SipRequest, SipResponse};

use crate::event::{
    InviteServerState, TerminationReason, TimerEvent, TransactionAction, TransactionEvent,
    TransactionId, TransactionKey,
};

// ============================================================================
// InviteServerTransaction - INVITE 服务端事务
// ============================================================================

/// INVITE 服务端事务
///
/// 按照 RFC 3261 Section 17.2.1 实现 INVITE 服务端事务状态机。
pub struct InviteServerTransaction {
    /// 事务 ID
    id: TransactionId,
    /// 事务匹配键
    key: TransactionKey,
    /// 当前状态
    state: InviteServerState,
    /// 原始 INVITE 请求
    original_request: SipRequest,
    /// 请求来源地址
    source_addr: SocketAddr,
    /// 传输协议
    transport: TransportProtocol,
    /// 最后发送的响应（用于重传）
    last_response: Option<SipResponse>,
    /// 是否已发送 100 Trying
    trying_sent: bool,
}

impl InviteServerTransaction {
    /// 创建新的 INVITE 服务端事务
    ///
    /// # 参数
    ///
    /// - `request` - 收到的 INVITE 请求
    /// - `source_addr` - 请求来源地址
    /// - `transport` - 传输协议
    pub fn new(request: SipRequest, source_addr: SocketAddr, transport: TransportProtocol) -> Self {
        let key = TransactionKey::from_request(&request).unwrap_or_else(|| {
            TransactionKey::new(
                sip_message::BranchId::new(),
                Method::Invite,
                "unknown".to_string(),
            )
        });

        let id = TransactionId::from_branch_and_method(&key.branch_id, &key.method);

        Self {
            id,
            key,
            state: InviteServerState::Proceeding,
            original_request: request,
            source_addr,
            transport,
            last_response: None,
            trying_sent: false,
        }
    }

    /// 获取事务 ID
    pub fn id(&self) -> &TransactionId {
        &self.id
    }

    /// 获取事务匹配键
    pub fn key(&self) -> &TransactionKey {
        &self.key
    }

    /// 获取当前状态
    pub fn state(&self) -> InviteServerState {
        self.state
    }

    /// 获取传输协议
    pub fn transport(&self) -> TransportProtocol {
        self.transport
    }

    /// 获取来源地址
    pub fn source_addr(&self) -> SocketAddr {
        self.source_addr
    }

    /// 获取原始请求的引用
    pub fn original_request(&self) -> &SipRequest {
        &self.original_request
    }

    /// 处理收到的请求
    ///
    /// 返回需要执行的动作列表。
    pub fn handle_request(&mut self, request: &SipRequest) -> Vec<TransactionAction> {
        match request.request_line.method {
            Method::Invite => self.handle_invite_retransmit(),
            Method::Ack => self.handle_ack(),
            _ => Vec::new(),
        }
    }

    /// 处理 TU 层发送的响应
    ///
    /// 当 TU 生成响应时调用此方法。
    pub fn handle_response_from_tu(&mut self, response: SipResponse) -> Vec<TransactionAction> {
        let status_code = response.status_line.status_code;

        match self.state {
            InviteServerState::Proceeding => self.handle_response_proceeding(response, status_code),
            InviteServerState::Completed => {
                // Completed 状态下 TU 不应再发响应，但如果是重传则重传
                self.handle_response_completed(response, status_code)
            }
            InviteServerState::Confirmed | InviteServerState::Terminated => Vec::new(),
        }
    }

    /// 处理定时器事件
    pub fn handle_timer(&mut self, event: &TimerEvent) -> Vec<TransactionAction> {
        match event {
            TimerEvent::TimerG { .. } => self.handle_timer_g(),
            TimerEvent::TimerH { .. } => self.handle_timer_h(),
            TimerEvent::TimerI { .. } => self.handle_timer_i(),
            _ => Vec::new(),
        }
    }

    /// 处理传输错误
    pub fn handle_transport_error(&mut self) -> Vec<TransactionAction> {
        if self.state == InviteServerState::Terminated {
            return Vec::new();
        }

        self.state = InviteServerState::Terminated;
        vec![
            TransactionAction::CancelTimers {
                transaction_id: self.id.clone(),
            },
            TransactionAction::EmitEvent(TransactionEvent::Terminated {
                transaction_id: self.id.clone(),
                reason: TerminationReason::TransportError,
            }),
        ]
    }

    /// 检查是否需要自动发送 100 Trying
    ///
    /// 如果 TU 在 200ms 内未响应，事务层应自动发送 100 Trying。
    pub fn should_send_trying(&self) -> bool {
        !self.trying_sent && self.state == InviteServerState::Proceeding
    }

    /// 标记已发送 100 Trying
    pub fn mark_trying_sent(&mut self) {
        self.trying_sent = true;
    }

    // ========================================================================
    // 内部方法：请求处理
    // ========================================================================

    fn handle_invite_retransmit(&mut self) -> Vec<TransactionAction> {
        match self.state {
            InviteServerState::Proceeding => {
                // 重传最后一个临时响应（如果有）
                if let Some(ref response) = self.last_response {
                    if response.status_line.status_code.is_provisional() {
                        return vec![TransactionAction::SendMessage {
                            message: sip_message::SipMessage::Response(response.clone()),
                            addr: self.source_addr,
                            transport: self.transport,
                        }];
                    }
                }
                Vec::new()
            }
            InviteServerState::Completed => {
                // 重传最后一个最终响应
                if let Some(ref response) = self.last_response {
                    vec![TransactionAction::SendMessage {
                        message: sip_message::SipMessage::Response(response.clone()),
                        addr: self.source_addr,
                        transport: self.transport,
                    }]
                } else {
                    Vec::new()
                }
            }
            InviteServerState::Confirmed | InviteServerState::Terminated => Vec::new(),
        }
    }

    fn handle_ack(&mut self) -> Vec<TransactionAction> {
        match self.state {
            InviteServerState::Completed => {
                // 收到 ACK → Confirmed
                self.state = InviteServerState::Confirmed;
                let mut actions = vec![TransactionAction::CancelTimers {
                    transaction_id: self.id.clone(),
                }];

                // 启动 Timer I
                actions.push(TransactionAction::StartTimer {
                    timer: TimerEvent::TimerI {
                        transaction_id: self.id.clone(),
                    },
                    delay_ms: if self.transport.is_reliable() {
                        0
                    } else {
                        5000
                    },
                });

                actions
            }
            InviteServerState::Proceeding => {
                // 在 Proceeding 状态收到 ACK 说明 TU 发送了 2xx，
                // 2xx 响应的 ACK 不在事务层处理
                tracing::debug!(
                    "ACK received in Proceeding state for INVITE server transaction {}",
                    self.id
                );
                Vec::new()
            }
            InviteServerState::Confirmed | InviteServerState::Terminated => Vec::new(),
        }
    }

    // ========================================================================
    // 内部方法：TU 响应处理
    // ========================================================================

    fn handle_response_proceeding(
        &mut self,
        response: SipResponse,
        status_code: StatusCode,
    ) -> Vec<TransactionAction> {
        let mut actions = Vec::new();

        if status_code.is_provisional() {
            // 1xx → 保存并转发
            self.last_response = Some(response.clone());
            actions.push(TransactionAction::SendMessage {
                message: sip_message::SipMessage::Response(response),
                addr: self.source_addr,
                transport: self.transport,
            });
        } else if status_code.is_success() {
            // 2xx → Terminated
            self.last_response = Some(response.clone());
            self.state = InviteServerState::Terminated;
            actions.push(TransactionAction::CancelTimers {
                transaction_id: self.id.clone(),
            });
            actions.push(TransactionAction::SendMessage {
                message: sip_message::SipMessage::Response(response),
                addr: self.source_addr,
                transport: self.transport,
            });
            actions.push(TransactionAction::EmitEvent(TransactionEvent::Terminated {
                transaction_id: self.id.clone(),
                reason: TerminationReason::Completed,
            }));
        } else {
            // 3xx-6xx → Completed
            self.last_response = Some(response.clone());
            self.state = InviteServerState::Completed;
            actions.push(TransactionAction::CancelTimers {
                transaction_id: self.id.clone(),
            });
            actions.push(TransactionAction::SendMessage {
                message: sip_message::SipMessage::Response(response),
                addr: self.source_addr,
                transport: self.transport,
            });
            // 启动 Timer G（响应重传）
            if !self.transport.is_reliable() {
                actions.push(TransactionAction::StartRetransmitTimer {
                    timer: TimerEvent::TimerG {
                        transaction_id: self.id.clone(),
                    },
                    initial_delay_ms: 500, // T1
                    max_delay_ms: 4000,    // T2
                });
            }
            // 启动 Timer H（等待 ACK 超时）
            actions.push(TransactionAction::StartTimer {
                timer: TimerEvent::TimerH {
                    transaction_id: self.id.clone(),
                },
                delay_ms: 64 * 500, // 64*T1
            });
        }

        actions
    }

    fn handle_response_completed(
        &mut self,
        response: SipResponse,
        status_code: StatusCode,
    ) -> Vec<TransactionAction> {
        // Completed 状态下重传最终响应
        if !status_code.is_provisional() && !status_code.is_success() {
            self.last_response = Some(response.clone());
            vec![TransactionAction::SendMessage {
                message: sip_message::SipMessage::Response(response),
                addr: self.source_addr,
                transport: self.transport,
            }]
        } else {
            Vec::new()
        }
    }

    // ========================================================================
    // 内部方法：定时器处理
    // ========================================================================

    fn handle_timer_g(&mut self) -> Vec<TransactionAction> {
        if self.state != InviteServerState::Completed {
            return Vec::new();
        }

        // Timer G 超时：重传最终响应
        if let Some(ref response) = self.last_response {
            tracing::debug!(
                "Timer G fired for INVITE server transaction {}, retransmitting response",
                self.id
            );
            vec![TransactionAction::SendMessage {
                message: sip_message::SipMessage::Response(response.clone()),
                addr: self.source_addr,
                transport: self.transport,
            }]
        } else {
            Vec::new()
        }
    }

    fn handle_timer_h(&mut self) -> Vec<TransactionAction> {
        if self.state != InviteServerState::Completed {
            return Vec::new();
        }

        // Timer H 超时：未收到 ACK，事务终止
        tracing::warn!(
            "Timer H fired for INVITE server transaction {}, no ACK received",
            self.id
        );
        self.state = InviteServerState::Terminated;
        vec![
            TransactionAction::CancelTimers {
                transaction_id: self.id.clone(),
            },
            TransactionAction::EmitEvent(TransactionEvent::Terminated {
                transaction_id: self.id.clone(),
                reason: TerminationReason::Timeout,
            }),
        ]
    }

    fn handle_timer_i(&mut self) -> Vec<TransactionAction> {
        if self.state != InviteServerState::Confirmed {
            return Vec::new();
        }

        // Timer I 超时：事务终止
        tracing::debug!(
            "Timer I fired for INVITE server transaction {}, terminating",
            self.id
        );
        self.state = InviteServerState::Terminated;
        vec![
            TransactionAction::CancelTimers {
                transaction_id: self.id.clone(),
            },
            TransactionAction::EmitEvent(TransactionEvent::Terminated {
                transaction_id: self.id.clone(),
                reason: TerminationReason::Completed,
            }),
        ]
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use sip_core::Host;
    use sip_core::SipVersion;
    use sip_message::uri::SipUri;
    use sip_message::{
        CSeqHeader, CallId, HeaderCollection, HeaderName, HeaderValue, RequestLine, StatusLine,
        ViaHeader,
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
            HeaderValue::FromTo(sip_message::FromToHeader {
                display_name: None,
                uri: SipUri::parse("sip:alice@example.com").unwrap(),
                tag: Some(sip_message::Tag::new()),
            }),
        );
        headers.insert(
            HeaderName::To,
            HeaderValue::FromTo(sip_message::FromToHeader {
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

    fn create_test_ack() -> SipRequest {
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
        headers.insert(HeaderName::CallId, HeaderValue::CallId(CallId::new()));
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(CSeqHeader::new(1, Method::Ack)),
        );

        SipRequest {
            request_line: RequestLine {
                method: Method::Ack,
                request_uri: uri,
                version: SipVersion,
            },
            headers,
            body: None,
        }
    }

    fn create_test_response(status_code: u16) -> SipResponse {
        let mut headers = HeaderCollection::new();
        headers.insert(
            HeaderName::Via,
            HeaderValue::Via(ViaHeader::new(
                TransportProtocol::Udp,
                Host::Domain("192.168.1.1".to_string()),
                Some(5060),
            )),
        );
        headers.insert(HeaderName::CallId, HeaderValue::CallId(CallId::new()));
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(CSeqHeader::new(1, Method::Invite)),
        );

        let reason = if status_code < 200 {
            "Trying"
        } else if status_code < 300 {
            "OK"
        } else {
            "Error"
        };

        SipResponse {
            status_line: StatusLine {
                version: SipVersion,
                status_code: StatusCode(status_code),
                reason_phrase: reason.to_string(),
            },
            headers,
            body: None,
        }
    }

    #[test]
    fn test_invite_server_initial_state() {
        let request = create_test_invite();
        let source: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let tx = InviteServerTransaction::new(request, source, TransportProtocol::Udp);

        assert_eq!(tx.state(), InviteServerState::Proceeding);
    }

    #[test]
    fn test_proceeding_sends_1xx() {
        let request = create_test_invite();
        let source: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteServerTransaction::new(request, source, TransportProtocol::Udp);

        let response_180 = create_test_response(180);
        let actions = tx.handle_response_from_tu(response_180);

        assert_eq!(tx.state(), InviteServerState::Proceeding);

        let has_send = actions
            .iter()
            .any(|a| matches!(a, TransactionAction::SendMessage { .. }));
        assert!(has_send);
    }

    #[test]
    fn test_proceeding_to_completed_on_error_response() {
        let request = create_test_invite();
        let source: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteServerTransaction::new(request, source, TransportProtocol::Udp);

        let response_404 = create_test_response(404);
        let actions = tx.handle_response_from_tu(response_404);

        assert_eq!(tx.state(), InviteServerState::Completed);

        let has_send = actions
            .iter()
            .any(|a| matches!(a, TransactionAction::SendMessage { .. }));
        assert!(has_send);
    }

    #[test]
    fn test_proceeding_to_terminated_on_2xx() {
        let request = create_test_invite();
        let source: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteServerTransaction::new(request, source, TransportProtocol::Udp);

        let response_200 = create_test_response(200);
        let actions = tx.handle_response_from_tu(response_200);

        assert_eq!(tx.state(), InviteServerState::Terminated);

        let has_terminated = actions.iter().any(|a| {
            matches!(
                a,
                TransactionAction::EmitEvent(TransactionEvent::Terminated { .. })
            )
        });
        assert!(has_terminated);
    }

    #[test]
    fn test_completed_to_confirmed_on_ack() {
        let request = create_test_invite();
        let source: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteServerTransaction::new(request, source, TransportProtocol::Udp);

        // 先进入 Completed 状态
        let response_404 = create_test_response(404);
        tx.handle_response_from_tu(response_404);
        assert_eq!(tx.state(), InviteServerState::Completed);

        // 收到 ACK
        let ack = create_test_ack();
        let _actions = tx.handle_request(&ack);

        assert_eq!(tx.state(), InviteServerState::Confirmed);
    }

    #[test]
    fn test_completed_retransmits_response_on_invite() {
        let request = create_test_invite();
        let source: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteServerTransaction::new(request, source, TransportProtocol::Udp);

        // 先进入 Completed 状态
        let response_404 = create_test_response(404);
        tx.handle_response_from_tu(response_404);
        assert_eq!(tx.state(), InviteServerState::Completed);

        // 收到 INVITE 重传
        let invite_retransmit = create_test_invite();
        let actions = tx.handle_request(&invite_retransmit);

        let has_send = actions
            .iter()
            .any(|a| matches!(a, TransactionAction::SendMessage { .. }));
        assert!(has_send);
    }

    #[test]
    fn test_timer_h_timeout() {
        let request = create_test_invite();
        let source: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteServerTransaction::new(request, source, TransportProtocol::Udp);

        // 进入 Completed 状态
        let response_404 = create_test_response(404);
        tx.handle_response_from_tu(response_404);

        // Timer H 超时
        let _actions = tx.handle_timer(&TimerEvent::TimerH {
            transaction_id: tx.id().clone(),
        });

        assert_eq!(tx.state(), InviteServerState::Terminated);
    }

    #[test]
    fn test_timer_i_terminates() {
        let request = create_test_invite();
        let source: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteServerTransaction::new(request, source, TransportProtocol::Udp);

        // Proceeding → Completed → Confirmed
        let response_404 = create_test_response(404);
        tx.handle_response_from_tu(response_404);
        let ack = create_test_ack();
        tx.handle_request(&ack);
        assert_eq!(tx.state(), InviteServerState::Confirmed);

        // Timer I 超时
        tx.handle_timer(&TimerEvent::TimerI {
            transaction_id: tx.id().clone(),
        });

        assert_eq!(tx.state(), InviteServerState::Terminated);
    }

    #[test]
    fn test_should_send_trying() {
        let request = create_test_invite();
        let source: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteServerTransaction::new(request, source, TransportProtocol::Udp);

        assert!(tx.should_send_trying());
        tx.mark_trying_sent();
        assert!(!tx.should_send_trying());
    }

    #[test]
    fn test_full_proceeding_completed_confirmed_terminated_flow() {
        let request = create_test_invite();
        let source: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteServerTransaction::new(request, source, TransportProtocol::Udp);

        assert_eq!(tx.state(), InviteServerState::Proceeding);

        // 发送 1xx
        let response_180 = create_test_response(180);
        tx.handle_response_from_tu(response_180);
        assert_eq!(tx.state(), InviteServerState::Proceeding);

        // 发送 3xx-6xx → Completed
        let response_404 = create_test_response(404);
        tx.handle_response_from_tu(response_404);
        assert_eq!(tx.state(), InviteServerState::Completed);

        // 收到 ACK → Confirmed
        let ack = create_test_ack();
        tx.handle_request(&ack);
        assert_eq!(tx.state(), InviteServerState::Confirmed);

        // Timer I → Terminated
        tx.handle_timer(&TimerEvent::TimerI {
            transaction_id: tx.id().clone(),
        });
        assert_eq!(tx.state(), InviteServerState::Terminated);
    }
}
