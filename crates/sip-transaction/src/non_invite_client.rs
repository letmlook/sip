//! 非 INVITE 客户端事务状态机
//!
//! 按照 RFC 3261 Section 17.1.2 实现非 INVITE 客户端事务。
//!
//! # 状态转换
//!
//! ```text
//!                    |Request from TU
//!                    |send request
//!  Trying ---------------+-----------+
//!                         |Timer E    |
//!                         |send req   |
//!                         |           |
//!                         |1xx        |
//!                         |           |
//!  Proceeding ---------+ |           |
//!                         |200-699    |
//!                         |           |
//!  Completed -----------+ |           |
//!                         |Timer K    |
//!                         |           |
//!  Terminated ----------+-----------+
//! ```

use std::net::SocketAddr;

use sip_core::{StatusCode, TransportProtocol};
use sip_message::{SipRequest, SipResponse};

use crate::event::{
    NonInviteClientState, TerminationReason, TimerEvent, TransactionAction, TransactionEvent,
    TransactionId, TransactionKey,
};

// ============================================================================
// NonInviteClientTransaction - 非 INVITE 客户端事务
// ============================================================================

/// 非 INVITE 客户端事务
///
/// 按照 RFC 3261 Section 17.1.2 实现非 INVITE 客户端事务状态机。
pub struct NonInviteClientTransaction {
    /// 事务 ID
    id: TransactionId,
    /// 事务匹配键
    key: TransactionKey,
    /// 当前状态
    state: NonInviteClientState,
    /// 原始请求（用于重传）
    original_request: SipRequest,
    /// 目标地址
    destination: SocketAddr,
    /// 传输协议
    transport: TransportProtocol,
}

impl NonInviteClientTransaction {
    /// 创建新的非 INVITE 客户端事务
    ///
    /// # 参数
    ///
    /// - `request` - 非 INVITE 请求
    /// - `destination` - 目标地址
    /// - `transport` - 传输协议
    pub fn new(request: SipRequest, destination: SocketAddr, transport: TransportProtocol) -> Self {
        let key = TransactionKey::from_request(&request).unwrap_or_else(|| {
            TransactionKey::new(
                sip_message::BranchId::new(),
                request.request_line.method.clone(),
                "unknown".to_string(),
            )
        });

        let id = TransactionId::from_branch_and_method(&key.branch_id, &key.method);

        Self {
            id,
            key,
            state: NonInviteClientState::Trying,
            original_request: request,
            destination,
            transport,
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
    pub fn state(&self) -> NonInviteClientState {
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
            NonInviteClientState::Trying => self.handle_response_trying(response, status_code),
            NonInviteClientState::Proceeding => {
                self.handle_response_proceeding(response, status_code)
            }
            NonInviteClientState::Completed => {
                self.handle_response_completed(response, status_code)
            }
            NonInviteClientState::Terminated => Vec::new(),
        }
    }

    /// 处理定时器事件
    pub fn handle_timer(&mut self, event: &TimerEvent) -> Vec<TransactionAction> {
        match event {
            TimerEvent::TimerE { .. } => self.handle_timer_e(),
            TimerEvent::TimerF { .. } => self.handle_timer_f(),
            TimerEvent::TimerK { .. } => self.handle_timer_k(),
            _ => Vec::new(),
        }
    }

    /// 处理传输错误
    pub fn handle_transport_error(&mut self) -> Vec<TransactionAction> {
        if self.state == NonInviteClientState::Terminated {
            return Vec::new();
        }

        self.state = NonInviteClientState::Terminated;
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
    // 内部方法：Trying 状态处理
    // ========================================================================

    fn handle_response_trying(
        &mut self,
        response: SipResponse,
        status_code: StatusCode,
    ) -> Vec<TransactionAction> {
        let mut actions = Vec::new();

        if status_code.is_provisional() {
            // 1xx → Proceeding
            self.state = NonInviteClientState::Proceeding;
            actions.push(TransactionAction::EmitEvent(
                TransactionEvent::ProvisionalResponse {
                    transaction_id: self.id.clone(),
                    response,
                },
            ));
        } else {
            // 2xx-6xx → Completed
            self.state = NonInviteClientState::Completed;
            actions.push(TransactionAction::CancelTimers {
                transaction_id: self.id.clone(),
            });
            // 启动 Timer K
            actions.push(TransactionAction::StartTimer {
                timer: TimerEvent::TimerK {
                    transaction_id: self.id.clone(),
                },
                delay_ms: if self.transport.is_reliable() {
                    0
                } else {
                    5000
                },
            });
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
        } else {
            // 2xx-6xx → Completed
            self.state = NonInviteClientState::Completed;
            actions.push(TransactionAction::CancelTimers {
                transaction_id: self.id.clone(),
            });
            // 启动 Timer K
            actions.push(TransactionAction::StartTimer {
                timer: TimerEvent::TimerK {
                    transaction_id: self.id.clone(),
                },
                delay_ms: if self.transport.is_reliable() {
                    0
                } else {
                    5000
                },
            });
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
        _response: SipResponse,
        _status_code: StatusCode,
    ) -> Vec<TransactionAction> {
        // 在 Completed 状态下收到任何响应都丢弃
        Vec::new()
    }

    // ========================================================================
    // 内部方法：定时器处理
    // ========================================================================

    fn handle_timer_e(&mut self) -> Vec<TransactionAction> {
        if self.state != NonInviteClientState::Trying
            && self.state != NonInviteClientState::Proceeding
        {
            return Vec::new();
        }

        // Timer E 超时：重传请求（仅 UDP）
        if !self.transport.is_reliable() {
            tracing::debug!(
                "Timer E fired for non-INVITE client transaction {}, retransmitting request",
                self.id
            );
            vec![TransactionAction::SendMessage {
                message: sip_message::SipMessage::Request(self.original_request.clone()),
                addr: self.destination,
                transport: self.transport,
            }]
        } else {
            Vec::new()
        }
    }

    fn handle_timer_f(&mut self) -> Vec<TransactionAction> {
        if self.state != NonInviteClientState::Trying
            && self.state != NonInviteClientState::Proceeding
        {
            return Vec::new();
        }

        // Timer F 超时：事务超时
        tracing::warn!(
            "Timer F fired for non-INVITE client transaction {}, transaction timeout",
            self.id
        );
        self.state = NonInviteClientState::Terminated;
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

    fn handle_timer_k(&mut self) -> Vec<TransactionAction> {
        if self.state != NonInviteClientState::Completed {
            return Vec::new();
        }

        // Timer K 超时：事务终止
        tracing::debug!(
            "Timer K fired for non-INVITE client transaction {}, terminating",
            self.id
        );
        self.state = NonInviteClientState::Terminated;
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
        CSeqHeader, CallId, HeaderCollection, HeaderName, HeaderValue, Method, RequestLine,
        StatusLine, ViaHeader,
    };

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
            HeaderValue::CSeq(CSeqHeader::new(1, Method::Register)),
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
    fn test_non_invite_client_initial_state() {
        let request = create_test_register();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let tx = NonInviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        assert_eq!(tx.state(), NonInviteClientState::Trying);
    }

    #[test]
    fn test_trying_to_proceeding_on_1xx() {
        let request = create_test_register();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = NonInviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        let response = create_test_response(100);
        let actions = tx.handle_response(response);

        assert_eq!(tx.state(), NonInviteClientState::Proceeding);

        let has_provisional = actions.iter().any(|a| {
            matches!(
                a,
                TransactionAction::EmitEvent(TransactionEvent::ProvisionalResponse { .. })
            )
        });
        assert!(has_provisional);
    }

    #[test]
    fn test_trying_to_completed_on_2xx() {
        let request = create_test_register();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = NonInviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        let response = create_test_response(200);
        let actions = tx.handle_response(response);

        assert_eq!(tx.state(), NonInviteClientState::Completed);

        let has_final = actions.iter().any(|a| {
            matches!(
                a,
                TransactionAction::EmitEvent(TransactionEvent::FinalResponse { .. })
            )
        });
        assert!(has_final);
    }

    #[test]
    fn test_trying_to_completed_on_error() {
        let request = create_test_register();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = NonInviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        let response = create_test_response(500);
        tx.handle_response(response);

        assert_eq!(tx.state(), NonInviteClientState::Completed);
    }

    #[test]
    fn test_proceeding_to_completed_on_2xx() {
        let request = create_test_register();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = NonInviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        // 先收到 1xx
        let response_1xx = create_test_response(100);
        tx.handle_response(response_1xx);
        assert_eq!(tx.state(), NonInviteClientState::Proceeding);

        // 再收到 2xx
        let response_2xx = create_test_response(200);
        tx.handle_response(response_2xx);

        assert_eq!(tx.state(), NonInviteClientState::Completed);
    }

    #[test]
    fn test_completed_discards_response() {
        let request = create_test_register();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = NonInviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        // 进入 Completed 状态
        let response_200 = create_test_response(200);
        tx.handle_response(response_200);
        assert_eq!(tx.state(), NonInviteClientState::Completed);

        // 收到另一个响应应丢弃
        let response_500 = create_test_response(500);
        let actions = tx.handle_response(response_500);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_timer_f_timeout() {
        let request = create_test_register();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = NonInviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        let actions = tx.handle_timer(&TimerEvent::TimerF {
            transaction_id: tx.id().clone(),
        });

        assert_eq!(tx.state(), NonInviteClientState::Terminated);

        let has_timeout = actions.iter().any(|a| {
            matches!(
                a,
                TransactionAction::EmitEvent(TransactionEvent::Timeout { .. })
            )
        });
        assert!(has_timeout);
    }

    #[test]
    fn test_timer_k_terminates() {
        let request = create_test_register();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = NonInviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        // 进入 Completed 状态
        let response_200 = create_test_response(200);
        tx.handle_response(response_200);
        assert_eq!(tx.state(), NonInviteClientState::Completed);

        // Timer K 超时
        tx.handle_timer(&TimerEvent::TimerK {
            transaction_id: tx.id().clone(),
        });

        assert_eq!(tx.state(), NonInviteClientState::Terminated);
    }

    #[test]
    fn test_timer_e_retransmits_request() {
        let request = create_test_register();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = NonInviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        let actions = tx.handle_timer(&TimerEvent::TimerE {
            transaction_id: tx.id().clone(),
        });

        let has_request = actions.iter().any(|a| matches!(
            a,
            TransactionAction::SendMessage { message, .. } if matches!(message, sip_message::SipMessage::Request(req) if req.request_line.method == Method::Register)
        ));
        assert!(has_request);
    }

    #[test]
    fn test_full_trying_proceeding_completed_terminated_flow() {
        let request = create_test_register();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = NonInviteClientTransaction::new(request, dest, TransportProtocol::Udp);

        assert_eq!(tx.state(), NonInviteClientState::Trying);

        // 1xx → Proceeding
        let response_100 = create_test_response(100);
        tx.handle_response(response_100);
        assert_eq!(tx.state(), NonInviteClientState::Proceeding);

        // 2xx → Completed
        let response_200 = create_test_response(200);
        tx.handle_response(response_200);
        assert_eq!(tx.state(), NonInviteClientState::Completed);

        // Timer K → Terminated
        tx.handle_timer(&TimerEvent::TimerK {
            transaction_id: tx.id().clone(),
        });
        assert_eq!(tx.state(), NonInviteClientState::Terminated);
    }
}
