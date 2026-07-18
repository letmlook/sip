//! INVITE 客户端事务状态机
//!
//! 按照 RFC 3261 Section 17.1.1 实现 INVITE 客户端事务。
//!
//! # 状态转换
//!
//! ```text
//!                    |INVITE from TU
//!          Timer A   |INVITE from TU
//!          Start     |INVITE from TU
//!            --------|                 |
//!                    | Timer A Fires   |
//!            --------| INVITE          |
//!                    | Timer A Fires   |
//!            --------| INVITE          |
//!                    |                 |
//!  Calling -------->|                 |
//!                    |1xx              |
//!                    |                 |
//!                    |1xx              |
//!  Proceeding ------|                 |
//!                    |                 |2xx
//!                    |                 |---+
//!                    |                 |   |
//!                    |                 |   |2xx
//!                    |                 |---+
//!                    |                 |
//!                    |3xx-6xx          |
//!                    |                 |1xx
//!                    |INVITE           |---+
//!                    |Timer B          |   |
//!  Completed <------|                 |   |
//!                    |                 |   |
//!                    |Timer D          |
//!                    |                 |
//!  Terminated <------|                 |
//! ```

use std::net::SocketAddr;

use siprs_core::SipVersion;
use siprs_core::{StatusCode, TransportProtocol};
use siprs_message::{
    BranchId, HeaderCollection, HeaderName, HeaderValue, Method, RequestLine, SipRequest,
    SipResponse,
};

use crate::event::{
    InviteClientState, TerminationReason, TimerEvent, TransactionAction, TransactionEvent,
    TransactionId, TransactionKey,
};

// ============================================================================
// InviteClientTransaction - INVITE 客户端事务
// ============================================================================

/// INVITE 客户端事务
///
/// 按照 RFC 3261 Section 17.1.1 实现 INVITE 客户端事务状态机。
pub struct InviteClientTransaction {
    /// 事务 ID
    id: TransactionId,
    /// 事务匹配键
    key: TransactionKey,
    /// 当前状态
    state: InviteClientState,
    /// 原始 INVITE 请求（用于重传）
    original_request: SipRequest,
    /// 目标地址
    destination: SocketAddr,
    /// 传输协议
    transport: TransportProtocol,
    /// 最后收到的最终响应（用于 Completed 状态重传 ACK）
    last_final_response: Option<SipResponse>,
}

impl InviteClientTransaction {
    /// 创建新的 INVITE 客户端事务
    ///
    /// # 参数
    ///
    /// - `request` - INVITE 请求
    /// - `destination` - 目标地址
    /// - `transport` - 传输协议
    pub fn new(request: SipRequest, destination: SocketAddr, transport: TransportProtocol) -> Self {
        let key = TransactionKey::from_request(&request).unwrap_or_else(|| {
            TransactionKey::new(BranchId::new(), Method::Invite, "unknown".to_string())
        });

        let id = TransactionId::from_branch_and_method(&key.branch_id, &key.method);

        Self {
            id,
            key,
            state: InviteClientState::Calling,
            original_request: request,
            destination,
            transport,
            last_final_response: None,
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
    pub fn state(&self) -> InviteClientState {
        self.state
    }

    /// 获取传输协议
    pub fn transport(&self) -> TransportProtocol {
        self.transport
    }

    /// 获取目标地址
    pub fn destination(&self) -> SocketAddr {
        self.destination
    }

    /// 获取原始请求的引用
    pub fn original_request(&self) -> &SipRequest {
        &self.original_request
    }

    /// 处理收到的响应
    ///
    /// 返回需要执行的动作列表。
    pub fn handle_response(&mut self, response: SipResponse) -> Vec<TransactionAction> {
        let status_code = response.status_line.status_code;

        match self.state {
            InviteClientState::Calling => self.handle_response_calling(response, status_code),
            InviteClientState::Proceeding => self.handle_response_proceeding(response, status_code),
            InviteClientState::Completed => self.handle_response_completed(response, status_code),
            InviteClientState::Terminated => Vec::new(),
        }
    }

    /// 处理定时器事件
    pub fn handle_timer(&mut self, event: &TimerEvent) -> Vec<TransactionAction> {
        match event {
            TimerEvent::TimerA { .. } => self.handle_timer_a(),
            TimerEvent::TimerB { .. } => self.handle_timer_b(),
            TimerEvent::TimerD { .. } => self.handle_timer_d(),
            _ => Vec::new(),
        }
    }

    /// 处理传输错误
    pub fn handle_transport_error(&mut self) -> Vec<TransactionAction> {
        if self.state == InviteClientState::Terminated {
            return Vec::new();
        }

        self.state = InviteClientState::Terminated;
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

    // ========================================================================
    // 内部方法：Calling 状态处理
    // ========================================================================

    fn handle_response_calling(
        &mut self,
        response: SipResponse,
        status_code: StatusCode,
    ) -> Vec<TransactionAction> {
        let mut actions = Vec::new();

        if status_code.is_provisional() {
            // 1xx → Proceeding
            self.state = InviteClientState::Proceeding;
            // 取消 Timer A（重传定时器）
            actions.push(TransactionAction::CancelTimers {
                transaction_id: self.id.clone(),
            });
            // 重新启动 Timer B（超时定时器）- 实际上 Timer B 不需要重启
            actions.push(TransactionAction::EmitEvent(
                TransactionEvent::ProvisionalResponse {
                    transaction_id: self.id.clone(),
                    response,
                },
            ));
        } else if status_code.is_success() {
            // 2xx → Terminated，通知 TU
            self.state = InviteClientState::Terminated;
            actions.push(TransactionAction::CancelTimers {
                transaction_id: self.id.clone(),
            });
            actions.push(TransactionAction::EmitEvent(
                TransactionEvent::FinalResponse {
                    transaction_id: self.id.clone(),
                    response,
                },
            ));
            actions.push(TransactionAction::EmitEvent(TransactionEvent::Terminated {
                transaction_id: self.id.clone(),
                reason: TerminationReason::Completed,
            }));
        } else {
            // 3xx-6xx → Completed，发送 ACK
            self.state = InviteClientState::Completed;
            self.last_final_response = Some(response.clone());
            actions.push(TransactionAction::CancelTimers {
                transaction_id: self.id.clone(),
            });
            // 构造并发送 ACK
            let ack = self.build_ack(&response);
            actions.push(TransactionAction::SendMessage {
                message: siprs_message::SipMessage::Request(ack),
                addr: self.destination,
                transport: self.transport,
            });
            // 启动 Timer D
            actions.push(TransactionAction::StartTimer {
                timer: TimerEvent::TimerD {
                    transaction_id: self.id.clone(),
                },
                delay_ms: if self.transport.is_reliable() {
                    0
                } else {
                    32000
                },
            });
            // 通知 TU 最终响应
            actions.push(TransactionAction::EmitEvent(
                TransactionEvent::FinalResponse {
                    transaction_id: self.id.clone(),
                    response,
                },
            ));
        }

        actions
    }

    // ========================================================================
    // 内部方法：Proceeding 状态处理
    // ========================================================================

    fn handle_response_proceeding(
        &mut self,
        response: SipResponse,
        status_code: StatusCode,
    ) -> Vec<TransactionAction> {
        let mut actions = Vec::new();

        if status_code.is_provisional() {
            // 1xx → 通知 TU
            actions.push(TransactionAction::EmitEvent(
                TransactionEvent::ProvisionalResponse {
                    transaction_id: self.id.clone(),
                    response,
                },
            ));
        } else if status_code.is_success() {
            // 2xx → Terminated，通知 TU
            self.state = InviteClientState::Terminated;
            actions.push(TransactionAction::CancelTimers {
                transaction_id: self.id.clone(),
            });
            actions.push(TransactionAction::EmitEvent(
                TransactionEvent::FinalResponse {
                    transaction_id: self.id.clone(),
                    response,
                },
            ));
            actions.push(TransactionAction::EmitEvent(TransactionEvent::Terminated {
                transaction_id: self.id.clone(),
                reason: TerminationReason::Completed,
            }));
        } else {
            // 3xx-6xx → Completed，发送 ACK
            self.state = InviteClientState::Completed;
            self.last_final_response = Some(response.clone());
            actions.push(TransactionAction::CancelTimers {
                transaction_id: self.id.clone(),
            });
            // 构造并发送 ACK
            let ack = self.build_ack(&response);
            actions.push(TransactionAction::SendMessage {
                message: siprs_message::SipMessage::Request(ack),
                addr: self.destination,
                transport: self.transport,
            });
            // 启动 Timer D
            actions.push(TransactionAction::StartTimer {
                timer: TimerEvent::TimerD {
                    transaction_id: self.id.clone(),
                },
                delay_ms: if self.transport.is_reliable() {
                    0
                } else {
                    32000
                },
            });
            // 通知 TU 最终响应
            actions.push(TransactionAction::EmitEvent(
                TransactionEvent::FinalResponse {
                    transaction_id: self.id.clone(),
                    response,
                },
            ));
        }

        actions
    }

    // ========================================================================
    // 内部方法：Completed 状态处理
    // ========================================================================

    fn handle_response_completed(
        &mut self,
        response: SipResponse,
        status_code: StatusCode,
    ) -> Vec<TransactionAction> {
        let mut actions = Vec::new();

        if !status_code.is_provisional() && !status_code.is_success() {
            // 3xx-6xx → 重传 ACK
            let ack = self.build_ack(&response);
            actions.push(TransactionAction::SendMessage {
                message: siprs_message::SipMessage::Request(ack),
                addr: self.destination,
                transport: self.transport,
            });
        }

        actions
    }

    // ========================================================================
    // 内部方法：定时器处理
    // ========================================================================

    fn handle_timer_a(&mut self) -> Vec<TransactionAction> {
        if self.state != InviteClientState::Calling {
            return Vec::new();
        }

        // Timer A 超时：重传 INVITE（仅 UDP）
        if !self.transport.is_reliable() {
            tracing::debug!(
                "Timer A fired for INVITE client transaction {}, retransmitting INVITE",
                self.id
            );
            vec![TransactionAction::SendMessage {
                message: siprs_message::SipMessage::Request(self.original_request.clone()),
                addr: self.destination,
                transport: self.transport,
            }]
        } else {
            Vec::new()
        }
    }

    fn handle_timer_b(&mut self) -> Vec<TransactionAction> {
        if self.state != InviteClientState::Calling && self.state != InviteClientState::Proceeding {
            return Vec::new();
        }

        // Timer B 超时：事务超时
        tracing::warn!(
            "Timer B fired for INVITE client transaction {}, transaction timeout",
            self.id
        );
        self.state = InviteClientState::Terminated;
        vec![
            TransactionAction::CancelTimers {
                transaction_id: self.id.clone(),
            },
            TransactionAction::EmitEvent(TransactionEvent::Timeout {
                transaction_id: self.id.clone(),
            }),
            TransactionAction::EmitEvent(TransactionEvent::Terminated {
                transaction_id: self.id.clone(),
                reason: TerminationReason::Timeout,
            }),
        ]
    }

    fn handle_timer_d(&mut self) -> Vec<TransactionAction> {
        if self.state != InviteClientState::Completed {
            return Vec::new();
        }

        // Timer D 超时：事务终止
        tracing::debug!(
            "Timer D fired for INVITE client transaction {}, terminating",
            self.id
        );
        self.state = InviteClientState::Terminated;
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

    // ========================================================================
    // 内部方法：ACK 构造
    // ========================================================================

    /// 构造 ACK 请求
    ///
    /// ACK 请求的构造规则（RFC 3261 Section 17.1.1.3）：
    /// - Request-URI 与 INVITE 相同
    /// - CSeq 方法为 ACK，序号与 INVITE 相同
    /// - Via 头部与 INVITE 相同（使用相同的 branch）
    /// - 包含 INVITE 中的 From、To、Call-ID 头部
    /// - 不包含消息体
    fn build_ack(&self, response: &SipResponse) -> SipRequest {
        let original = &self.original_request;

        // 复制头部，修改 CSeq 方法为 ACK
        let mut headers = HeaderCollection::new();

        // 复制 Via 头部
        for (_, value) in original.headers.iter() {
            if let HeaderValue::Via(via) = value {
                headers.insert(HeaderName::Via, HeaderValue::Via(via.clone()));
                break; // 只取顶部 Via
            }
        }

        // 复制 From 头部
        if let Some(from) = original.headers.get(&HeaderName::From) {
            headers.insert(HeaderName::From, from.clone());
        }

        // 复制 To 头部（从响应中获取，可能包含 tag）
        if let Some(to) = response.headers.get(&HeaderName::To) {
            headers.insert(HeaderName::To, to.clone());
        } else if let Some(to) = original.headers.get(&HeaderName::To) {
            headers.insert(HeaderName::To, to.clone());
        }

        // 复制 Call-ID
        if let Some(call_id) = original.headers.get(&HeaderName::CallId) {
            headers.insert(HeaderName::CallId, call_id.clone());
        }

        // 构造 CSeq（方法改为 ACK，序号不变）
        if let Some(cseq_val) = original.headers.get(&HeaderName::CSeq) {
            if let Some(cseq) = cseq_val.as_cseq() {
                headers.insert(
                    HeaderName::CSeq,
                    HeaderValue::CSeq(siprs_message::CSeqHeader::new(cseq.sequence.0, Method::Ack)),
                );
            }
        }

        // 复制 Max-Forwards
        if let Some(max_forwards) = original.headers.get(&HeaderName::MaxForwards) {
            headers.insert(HeaderName::MaxForwards, max_forwards.clone());
        }

        SipRequest {
            request_line: RequestLine {
                method: Method::Ack,
                request_uri: original.request_line.request_uri.clone(),
                version: SipVersion,
            },
            headers,
            body: None,
        }
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use siprs_core::Host;
    use siprs_message::uri::SipUri;
    use siprs_message::{CSeqHeader, CallId, StatusLine, ViaHeader};

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
                tag: Some(siprs_message::Tag::new()),
            }),
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
    fn test_invite_client_initial_state() {
        let request = create_test_invite();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let tx = InviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        assert_eq!(tx.state(), InviteClientState::Calling);
        assert_eq!(tx.transport(), TransportProtocol::Udp);
    }

    #[test]
    fn test_calling_to_proceeding_on_1xx() {
        let request = create_test_invite();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        let response = create_test_response(180);
        let actions = tx.handle_response(response);

        assert_eq!(tx.state(), InviteClientState::Proceeding);

        // 应该有取消定时器和发送 ProvisionalResponse 事件
        let has_provisional = actions.iter().any(|a| {
            matches!(
                a,
                TransactionAction::EmitEvent(TransactionEvent::ProvisionalResponse { .. })
            )
        });
        assert!(has_provisional);
    }

    #[test]
    fn test_calling_to_terminated_on_2xx() {
        let request = create_test_invite();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        let response = create_test_response(200);
        let actions = tx.handle_response(response);

        assert_eq!(tx.state(), InviteClientState::Terminated);

        let has_final = actions.iter().any(|a| {
            matches!(
                a,
                TransactionAction::EmitEvent(TransactionEvent::FinalResponse { .. })
            )
        });
        assert!(has_final);

        let has_terminated = actions.iter().any(|a| {
            matches!(
                a,
                TransactionAction::EmitEvent(TransactionEvent::Terminated { .. })
            )
        });
        assert!(has_terminated);
    }

    #[test]
    fn test_calling_to_completed_on_3xx_6xx() {
        let request = create_test_invite();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        let response = create_test_response(404);
        let actions = tx.handle_response(response);

        assert_eq!(tx.state(), InviteClientState::Completed);

        // 应该有发送 ACK 的动作
        let has_ack = actions.iter().any(|a| matches!(
            a,
            TransactionAction::SendMessage { message, .. } if matches!(message, siprs_message::SipMessage::Request(req) if req.request_line.method == Method::Ack)
        ));
        assert!(has_ack);
    }

    #[test]
    fn test_proceeding_to_terminated_on_2xx() {
        let request = create_test_invite();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        // 先收到 1xx
        let response_1xx = create_test_response(180);
        tx.handle_response(response_1xx);
        assert_eq!(tx.state(), InviteClientState::Proceeding);

        // 再收到 2xx
        let response_2xx = create_test_response(200);
        let actions = tx.handle_response(response_2xx);

        assert_eq!(tx.state(), InviteClientState::Terminated);

        let has_final = actions.iter().any(|a| {
            matches!(
                a,
                TransactionAction::EmitEvent(TransactionEvent::FinalResponse { .. })
            )
        });
        assert!(has_final);
    }

    #[test]
    fn test_proceeding_to_completed_on_3xx_6xx() {
        let request = create_test_invite();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        // 先收到 1xx
        let response_1xx = create_test_response(180);
        tx.handle_response(response_1xx);
        assert_eq!(tx.state(), InviteClientState::Proceeding);

        // 再收到 404
        let response_404 = create_test_response(404);
        let actions = tx.handle_response(response_404);

        assert_eq!(tx.state(), InviteClientState::Completed);

        // 应该有发送 ACK 的动作
        let has_ack = actions.iter().any(|a| matches!(
            a,
            TransactionAction::SendMessage { message, .. } if matches!(message, siprs_message::SipMessage::Request(req) if req.request_line.method == Method::Ack)
        ));
        assert!(has_ack);
    }

    #[test]
    fn test_completed_retransmits_ack_on_3xx_6xx() {
        let request = create_test_invite();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        // 进入 Completed 状态
        let response_404 = create_test_response(404);
        tx.handle_response(response_404);
        assert_eq!(tx.state(), InviteClientState::Completed);

        // 收到重传的 404
        let response_404_retransmit = create_test_response(404);
        let actions = tx.handle_response(response_404_retransmit);

        // 应该重传 ACK
        let has_ack = actions.iter().any(|a| matches!(
            a,
            TransactionAction::SendMessage { message, .. } if matches!(message, siprs_message::SipMessage::Request(req) if req.request_line.method == Method::Ack)
        ));
        assert!(has_ack);
    }

    #[test]
    fn test_timer_b_timeout() {
        let request = create_test_invite();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        let actions = tx.handle_timer(&TimerEvent::TimerB {
            transaction_id: tx.id().clone(),
        });

        assert_eq!(tx.state(), InviteClientState::Terminated);

        let has_timeout = actions.iter().any(|a| {
            matches!(
                a,
                TransactionAction::EmitEvent(TransactionEvent::Timeout { .. })
            )
        });
        assert!(has_timeout);
    }

    #[test]
    fn test_timer_d_terminates() {
        let request = create_test_invite();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        // 进入 Completed 状态
        let response_404 = create_test_response(404);
        tx.handle_response(response_404);
        assert_eq!(tx.state(), InviteClientState::Completed);

        // Timer D 超时
        let _actions = tx.handle_timer(&TimerEvent::TimerD {
            transaction_id: tx.id().clone(),
        });

        assert_eq!(tx.state(), InviteClientState::Terminated);
    }

    #[test]
    fn test_timer_a_retransmits_invite() {
        let request = create_test_invite();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        let actions = tx.handle_timer(&TimerEvent::TimerA {
            transaction_id: tx.id().clone(),
        });

        // 应该重传 INVITE
        let has_invite = actions.iter().any(|a| matches!(
            a,
            TransactionAction::SendMessage { message, .. } if matches!(message, siprs_message::SipMessage::Request(req) if req.request_line.method == Method::Invite)
        ));
        assert!(has_invite);
    }

    #[test]
    fn test_timer_a_no_retransmit_on_tcp() {
        let request = create_test_invite();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteClientTransaction::new(request, dest, TransportProtocol::Tcp);

        let actions = tx.handle_timer(&TimerEvent::TimerA {
            transaction_id: tx.id().clone(),
        });

        // TCP 不应重传
        assert!(actions.is_empty());
    }

    #[test]
    fn test_transport_error() {
        let request = create_test_invite();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        let actions = tx.handle_transport_error();

        assert_eq!(tx.state(), InviteClientState::Terminated);

        let has_transport_error = actions.iter().any(|a| {
            matches!(
                a,
                TransactionAction::EmitEvent(TransactionEvent::Terminated {
                    reason: TerminationReason::TransportError,
                    ..
                })
            )
        });
        assert!(has_transport_error);
    }

    #[test]
    fn test_full_calling_proceeding_completed_terminated_flow() {
        let request = create_test_invite();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = InviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        assert_eq!(tx.state(), InviteClientState::Calling);

        // 1xx → Proceeding
        let response_180 = create_test_response(180);
        tx.handle_response(response_180);
        assert_eq!(tx.state(), InviteClientState::Proceeding);

        // 3xx-6xx → Completed
        let response_404 = create_test_response(404);
        tx.handle_response(response_404);
        assert_eq!(tx.state(), InviteClientState::Completed);

        // Timer D → Terminated
        tx.handle_timer(&TimerEvent::TimerD {
            transaction_id: tx.id().clone(),
        });
        assert_eq!(tx.state(), InviteClientState::Terminated);
    }
}
