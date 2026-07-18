//! 非 INVITE 服务端事务状态机
//!
//! 按照 RFC 3261 Section 17.2.2 实现非 INVITE 服务端事务。
//!
//! # 状态转换
//!
//! ```text
//!                              |Request
//!                              |received
//!                              |send to TU
//!                              V
//!                        +-----------+
//!                        |           |
//!                        | Trying    |
//!                        |           |
//!                        +-----------+
//!                              |
//!                              |1xx from TU
//!                              |
//!                              V
//!                        +-----------+
//!                        |           |
//!                        | Proceeding|
//!                        |           |
//!                        +-----------+
//!                              |
//!                              |200-699 from TU
//!                              |
//!                              V
//!                        +-----------+
//!                        |           |
//!                        | Completed |
//!                        |           |
//!                        +-----------+
//!                              |
//!                              |Timer J
//!                              |
//!                              V
//!                        +-----------+
//!                        |           |
//!                        |Terminated |
//!                        |           |
//!                        +-----------+
//! ```

use std::net::SocketAddr;

use siprs_core::{StatusCode, TransportProtocol};
use siprs_message::{SipRequest, SipResponse};

use crate::event::{
    NonInviteServerState, TerminationReason, TimerEvent, TransactionAction, TransactionEvent,
    TransactionId, TransactionKey,
};

// ============================================================================
// NonInviteServerTransaction - 非 INVITE 服务端事务
// ============================================================================

/// 非 INVITE 服务端事务
///
/// 按照 RFC 3261 Section 17.2.2 实现非 INVITE 服务端事务状态机。
pub struct NonInviteServerTransaction {
    /// 事务 ID
    id: TransactionId,
    /// 事务匹配键
    key: TransactionKey,
    /// 当前状态
    state: NonInviteServerState,
    /// 原始请求
    original_request: SipRequest,
    /// 请求来源地址
    source_addr: SocketAddr,
    /// 传输协议
    transport: TransportProtocol,
    /// 最后发送的响应（用于重传）
    last_response: Option<SipResponse>,
}

impl NonInviteServerTransaction {
    /// 创建新的非 INVITE 服务端事务
    ///
    /// # 参数
    ///
    /// - `request` - 收到的请求
    /// - `source_addr` - 请求来源地址
    /// - `transport` - 传输协议
    pub fn new(request: SipRequest, source_addr: SocketAddr, transport: TransportProtocol) -> Self {
        let key = TransactionKey::from_request(&request).unwrap_or_else(|| {
            TransactionKey::new(
                siprs_message::BranchId::new(),
                request.request_line.method.clone(),
                "unknown".to_string(),
            )
        });

        let id = TransactionId::from_branch_and_method(&key.branch_id, &key.method);

        Self {
            id,
            key,
            state: NonInviteServerState::Trying,
            original_request: request,
            source_addr,
            transport,
            last_response: None,
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
    pub fn state(&self) -> NonInviteServerState {
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
    pub fn handle_request(&mut self, _request: &SipRequest) -> Vec<TransactionAction> {
        match self.state {
            NonInviteServerState::Trying => {
                // Trying 状态下收到请求重传，不做处理
                Vec::new()
            }
            NonInviteServerState::Proceeding => {
                // Proceeding 状态下收到请求重传，重传最后一个临时响应
                if let Some(ref response) = self.last_response {
                    if response.status_line.status_code.is_provisional() {
                        return vec![TransactionAction::SendMessage {
                            message: siprs_message::SipMessage::Response(response.clone()),
                            addr: self.source_addr,
                            transport: self.transport,
                        }];
                    }
                }
                Vec::new()
            }
            NonInviteServerState::Completed => {
                // Completed 状态下收到请求重传，重传最终响应
                if let Some(ref response) = self.last_response {
                    vec![TransactionAction::SendMessage {
                        message: siprs_message::SipMessage::Response(response.clone()),
                        addr: self.source_addr,
                        transport: self.transport,
                    }]
                } else {
                    Vec::new()
                }
            }
            NonInviteServerState::Terminated => Vec::new(),
        }
    }

    /// 处理 TU 层发送的响应
    ///
    /// 当 TU 生成响应时调用此方法。
    pub fn handle_response_from_tu(&mut self, response: SipResponse) -> Vec<TransactionAction> {
        let status_code = response.status_line.status_code;

        match self.state {
            NonInviteServerState::Trying => self.handle_response_trying(response, status_code),
            NonInviteServerState::Proceeding => {
                self.handle_response_proceeding(response, status_code)
            }
            NonInviteServerState::Completed | NonInviteServerState::Terminated => Vec::new(),
        }
    }

    /// 处理定时器事件
    pub fn handle_timer(&mut self, event: &TimerEvent) -> Vec<TransactionAction> {
        match event {
            TimerEvent::TimerJ { .. } => self.handle_timer_j(),
            _ => Vec::new(),
        }
    }

    /// 处理传输错误
    pub fn handle_transport_error(&mut self) -> Vec<TransactionAction> {
        if self.state == NonInviteServerState::Terminated {
            return Vec::new();
        }

        self.state = NonInviteServerState::Terminated;
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
    // 内部方法：TU 响应处理
    // ========================================================================

    fn handle_response_trying(
        &mut self,
        response: SipResponse,
        status_code: StatusCode,
    ) -> Vec<TransactionAction> {
        let mut actions = Vec::new();

        if status_code.is_provisional() {
            // 1xx → Proceeding
            self.state = NonInviteServerState::Proceeding;
            self.last_response = Some(response.clone());
            actions.push(TransactionAction::SendMessage {
                message: siprs_message::SipMessage::Response(response),
                addr: self.source_addr,
                transport: self.transport,
            });
        } else {
            // 2xx-6xx → Completed
            self.state = NonInviteServerState::Completed;
            self.last_response = Some(response.clone());
            actions.push(TransactionAction::SendMessage {
                message: siprs_message::SipMessage::Response(response),
                addr: self.source_addr,
                transport: self.transport,
            });
            // 启动 Timer J
            actions.push(TransactionAction::StartTimer {
                timer: TimerEvent::TimerJ {
                    transaction_id: self.id.clone(),
                },
                delay_ms: if self.transport.is_reliable() {
                    0
                } else {
                    64 * 500
                },
            });
        }

        actions
    }

    fn handle_response_proceeding(
        &mut self,
        response: SipResponse,
        status_code: StatusCode,
    ) -> Vec<TransactionAction> {
        let mut actions = Vec::new();

        if status_code.is_provisional() {
            // 1xx → 转发
            self.last_response = Some(response.clone());
            actions.push(TransactionAction::SendMessage {
                message: siprs_message::SipMessage::Response(response),
                addr: self.source_addr,
                transport: self.transport,
            });
        } else {
            // 2xx-6xx → Completed
            self.state = NonInviteServerState::Completed;
            self.last_response = Some(response.clone());
            actions.push(TransactionAction::SendMessage {
                message: siprs_message::SipMessage::Response(response),
                addr: self.source_addr,
                transport: self.transport,
            });
            // 启动 Timer J
            actions.push(TransactionAction::StartTimer {
                timer: TimerEvent::TimerJ {
                    transaction_id: self.id.clone(),
                },
                delay_ms: if self.transport.is_reliable() {
                    0
                } else {
                    64 * 500
                },
            });
        }

        actions
    }

    // ========================================================================
    // 内部方法：定时器处理
    // ========================================================================

    fn handle_timer_j(&mut self) -> Vec<TransactionAction> {
        if self.state != NonInviteServerState::Completed {
            return Vec::new();
        }

        // Timer J 超时：事务终止
        tracing::debug!(
            "Timer J fired for non-INVITE server transaction {}, terminating",
            self.id
        );
        self.state = NonInviteServerState::Terminated;
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
    use siprs_core::Host;
    use siprs_core::SipVersion;
    use siprs_message::uri::SipUri;
    use siprs_message::{
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
    fn test_non_invite_server_initial_state() {
        let request = create_test_register();
        let source: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let tx = NonInviteServerTransaction::new(request, source, TransportProtocol::Udp);

        assert_eq!(tx.state(), NonInviteServerState::Trying);
    }

    #[test]
    fn test_trying_to_proceeding_on_1xx() {
        let request = create_test_register();
        let source: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = NonInviteServerTransaction::new(request, source, TransportProtocol::Udp);

        let response_100 = create_test_response(100);
        let actions = tx.handle_response_from_tu(response_100);

        assert_eq!(tx.state(), NonInviteServerState::Proceeding);

        let has_send = actions
            .iter()
            .any(|a| matches!(a, TransactionAction::SendMessage { .. }));
        assert!(has_send);
    }

    #[test]
    fn test_trying_to_completed_on_2xx() {
        let request = create_test_register();
        let source: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = NonInviteServerTransaction::new(request, source, TransportProtocol::Udp);

        let response_200 = create_test_response(200);
        let actions = tx.handle_response_from_tu(response_200);

        assert_eq!(tx.state(), NonInviteServerState::Completed);

        let has_send = actions
            .iter()
            .any(|a| matches!(a, TransactionAction::SendMessage { .. }));
        assert!(has_send);
    }

    #[test]
    fn test_proceeding_to_completed_on_final_response() {
        let request = create_test_register();
        let source: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = NonInviteServerTransaction::new(request, source, TransportProtocol::Udp);

        // 先发送 1xx
        let response_100 = create_test_response(100);
        tx.handle_response_from_tu(response_100);
        assert_eq!(tx.state(), NonInviteServerState::Proceeding);

        // 发送 2xx
        let response_200 = create_test_response(200);
        tx.handle_response_from_tu(response_200);
        assert_eq!(tx.state(), NonInviteServerState::Completed);
    }

    #[test]
    fn test_completed_retransmits_on_request() {
        let request = create_test_register();
        let source: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = NonInviteServerTransaction::new(request, source, TransportProtocol::Udp);

        // 进入 Completed 状态
        let response_200 = create_test_response(200);
        tx.handle_response_from_tu(response_200);
        assert_eq!(tx.state(), NonInviteServerState::Completed);

        // 收到请求重传
        let retransmit = create_test_register();
        let actions = tx.handle_request(&retransmit);

        let has_send = actions
            .iter()
            .any(|a| matches!(a, TransactionAction::SendMessage { .. }));
        assert!(has_send);
    }

    #[test]
    fn test_timer_j_terminates() {
        let request = create_test_register();
        let source: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = NonInviteServerTransaction::new(request, source, TransportProtocol::Udp);

        // 进入 Completed 状态
        let response_200 = create_test_response(200);
        tx.handle_response_from_tu(response_200);

        // Timer J 超时
        tx.handle_timer(&TimerEvent::TimerJ {
            transaction_id: tx.id().clone(),
        });

        assert_eq!(tx.state(), NonInviteServerState::Terminated);
    }

    #[test]
    fn test_full_trying_proceeding_completed_terminated_flow() {
        let request = create_test_register();
        let source: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let mut tx = NonInviteServerTransaction::new(request, source, TransportProtocol::Udp);

        assert_eq!(tx.state(), NonInviteServerState::Trying);

        // 1xx → Proceeding
        let response_100 = create_test_response(100);
        tx.handle_response_from_tu(response_100);
        assert_eq!(tx.state(), NonInviteServerState::Proceeding);

        // 2xx → Completed
        let response_200 = create_test_response(200);
        tx.handle_response_from_tu(response_200);
        assert_eq!(tx.state(), NonInviteServerState::Completed);

        // Timer J → Terminated
        tx.handle_timer(&TimerEvent::TimerJ {
            transaction_id: tx.id().clone(),
        });
        assert_eq!(tx.state(), NonInviteServerState::Terminated);
    }
}
