//! 事务事件与定时器事件类型
//!
//! 定义事务层向 TU 层发送的事件、定时器事件类型，
//! 以及事务状态枚举和终止原因。

use std::fmt;
use std::net::SocketAddr;

use siprs_core::{TransportError, TransportProtocol};
use siprs_message::{BranchId, Method, SipRequest, SipResponse};

// ============================================================================
// TransactionId - 事务唯一标识
// ============================================================================

/// 事务唯一标识
///
/// 基于 Branch ID 和方法名生成的唯一字符串标识。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransactionId(pub String);

impl TransactionId {
    /// 根据分支 ID 和方法生成事务 ID
    pub fn from_branch_and_method(branch: &BranchId, method: &Method) -> Self {
        Self(format!("{}:{}", branch.0, method))
    }

    /// 生成随机事务 ID
    pub fn new_random() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl fmt::Display for TransactionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ============================================================================
// TransactionKey - 事务匹配键
// ============================================================================

/// 事务键（用于匹配）
///
/// 根据 Via 头部的 branch 参数和请求方法匹配事务。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransactionKey {
    /// Via 头部分支标识
    pub branch_id: BranchId,
    /// 请求方法
    pub method: Method,
    /// Via 头部 sent-by 值
    pub sent_by: String,
}

impl TransactionKey {
    /// 创建新的事务键
    pub fn new(branch_id: BranchId, method: Method, sent_by: String) -> Self {
        Self {
            branch_id,
            method,
            sent_by,
        }
    }

    /// 从 SIP 请求提取事务键
    ///
    /// 从请求的顶部 Via 头部提取 branch 参数和 sent-by，
    /// 结合请求方法构造事务键。
    pub fn from_request(request: &SipRequest) -> Option<Self> {
        let via_value = request.headers.get(&siprs_message::HeaderName::Via)?;
        let via = via_value.as_via()?;

        Some(Self {
            branch_id: via.branch.clone(),
            method: request.request_line.method.clone(),
            sent_by: via.sent_by.to_string(),
        })
    }

    /// 从 SIP 响应提取事务键
    ///
    /// 从响应的顶部 Via 头部提取 branch 参数和 sent-by，
    /// 结合 CSeq 头部的方法构造事务键。
    pub fn from_response(response: &SipResponse) -> Option<Self> {
        let via_value = response.headers.get(&siprs_message::HeaderName::Via)?;
        let via = via_value.as_via()?;

        let cseq_value = response.headers.get(&siprs_message::HeaderName::CSeq)?;
        let cseq = cseq_value.as_cseq()?;

        Some(Self {
            branch_id: via.branch.clone(),
            method: cseq.method.clone(),
            sent_by: via.sent_by.to_string(),
        })
    }
}

impl fmt::Display for TransactionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.branch_id, self.method)
    }
}

// ============================================================================
// 事务状态枚举
// ============================================================================

/// INVITE 客户端事务状态（RFC 3261 Section 17.1.1）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteClientState {
    /// 初始状态，INVITE 已发送，等待响应
    Calling,
    /// 收到临时响应（1xx），等待最终响应
    Proceeding,
    /// 收到最终响应（3xx-6xx），已发送 ACK，等待 Timer D
    Completed,
    /// 事务终止
    Terminated,
}

impl fmt::Display for InviteClientState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Calling => write!(f, "Calling"),
            Self::Proceeding => write!(f, "Proceeding"),
            Self::Completed => write!(f, "Completed"),
            Self::Terminated => write!(f, "Terminated"),
        }
    }
}

/// 非 INVITE 客户端事务状态（RFC 3261 Section 17.1.2）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonInviteClientState {
    /// 初始状态，请求已发送，等待响应
    Trying,
    /// 收到临时响应（1xx），等待最终响应
    Proceeding,
    /// 收到最终响应（2xx-6xx），等待 Timer K
    Completed,
    /// 事务终止
    Terminated,
}

impl fmt::Display for NonInviteClientState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Trying => write!(f, "Trying"),
            Self::Proceeding => write!(f, "Proceeding"),
            Self::Completed => write!(f, "Completed"),
            Self::Terminated => write!(f, "Terminated"),
        }
    }
}

/// INVITE 服务端事务状态（RFC 3261 Section 17.2.1）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteServerState {
    /// 收到 INVITE，等待 TU 响应
    Proceeding,
    /// 收到 ACK，等待 Timer I
    Confirmed,
    /// TU 生成最终响应（3xx-6xx），等待 ACK
    Completed,
    /// 事务终止
    Terminated,
}

impl fmt::Display for InviteServerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Proceeding => write!(f, "Proceeding"),
            Self::Confirmed => write!(f, "Confirmed"),
            Self::Completed => write!(f, "Completed"),
            Self::Terminated => write!(f, "Terminated"),
        }
    }
}

/// 非 INVITE 服务端事务状态（RFC 3261 Section 17.2.2）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonInviteServerState {
    /// 收到请求，等待 TU 响应
    Trying,
    /// TU 生成临时响应
    Proceeding,
    /// TU 生成最终响应
    Completed,
    /// 事务终止
    Terminated,
}

impl fmt::Display for NonInviteServerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Trying => write!(f, "Trying"),
            Self::Proceeding => write!(f, "Proceeding"),
            Self::Completed => write!(f, "Completed"),
            Self::Terminated => write!(f, "Terminated"),
        }
    }
}

// ============================================================================
// TerminationReason - 事务终止原因
// ============================================================================

/// 事务终止原因
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminationReason {
    /// 正常完成
    Completed,
    /// 超时
    Timeout,
    /// 传输错误
    TransportError,
    /// 用户取消
    UserCancel,
}

impl fmt::Display for TerminationReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Completed => write!(f, "Completed"),
            Self::Timeout => write!(f, "Timeout"),
            Self::TransportError => write!(f, "TransportError"),
            Self::UserCancel => write!(f, "UserCancel"),
        }
    }
}

// ============================================================================
// TransactionEvent - 事务层向 TU 层发送的事件
// ============================================================================

/// 事务层向 TU 层发送的事件
#[derive(Debug)]
pub enum TransactionEvent {
    /// 收到临时响应 (1xx)
    ProvisionalResponse {
        transaction_id: TransactionId,
        response: SipResponse,
    },
    /// 收到最终响应 (2xx-6xx)
    FinalResponse {
        transaction_id: TransactionId,
        response: SipResponse,
    },
    /// 收到请求（服务端事务）
    RequestReceived {
        transaction_id: TransactionId,
        request: SipRequest,
        source_addr: SocketAddr,
    },
    /// 事务超时
    Timeout { transaction_id: TransactionId },
    /// 传输错误
    TransportError {
        transaction_id: TransactionId,
        error: TransportError,
    },
    /// 事务终止
    Terminated {
        transaction_id: TransactionId,
        reason: TerminationReason,
    },
}

// ============================================================================
// TimerEvent - 定时器事件
// ============================================================================

/// 定时器事件
///
/// SIP 事务层定义的所有定时器（Timer A~K），
/// 按照 RFC 3261 Section 17 实现。
#[derive(Debug)]
pub enum TimerEvent {
    /// Timer A: INVITE 客户端请求重传定时器（仅 UDP）
    /// 初始值 T1，每次超时翻倍，最大 T2
    TimerA { transaction_id: TransactionId },
    /// Timer B: INVITE 客户端事务超时定时器
    /// 值 64*T1
    TimerB { transaction_id: TransactionId },
    /// Timer D: INVITE 客户端等待延迟响应定时器
    /// 值 32s (UDP) / 0s (可靠传输)
    TimerD { transaction_id: TransactionId },
    /// Timer E: 非 INVITE 客户端请求重传定时器（仅 UDP）
    /// 初始值 T1，每次超时翻倍，最大 T2
    TimerE { transaction_id: TransactionId },
    /// Timer F: 非 INVITE 客户端事务超时定时器
    /// 值 64*T1
    TimerF { transaction_id: TransactionId },
    /// Timer G: INVITE 服务端响应重传定时器
    /// 初始值 T1，每次超时翻倍，最大 T2
    TimerG { transaction_id: TransactionId },
    /// Timer H: INVITE 服务端等待 ACK 超时定时器
    /// 值 64*T1
    TimerH { transaction_id: TransactionId },
    /// Timer I: INVITE 服务端 Confirmed 状态等待定时器
    /// 值 T4 (UDP) / 0s (可靠传输)
    TimerI { transaction_id: TransactionId },
    /// Timer J: 非 INVITE 服务端事务超时定时器
    /// 值 64*T1 (UDP) / 0s (可靠传输)
    TimerJ { transaction_id: TransactionId },
    /// Timer K: 非 INVITE 客户端等待响应重传定时器
    /// 值 T4 (UDP) / 0s (可靠传输)
    TimerK { transaction_id: TransactionId },
}

// ============================================================================
// TransactionAction - 事务处理动作
// ============================================================================

/// 事务处理产生的动作
///
/// 状态机处理事件后产生的动作列表，由 TransactionManager 执行。
#[derive(Debug)]
pub enum TransactionAction {
    /// 发送 SIP 消息
    SendMessage {
        message: siprs_message::SipMessage,
        addr: SocketAddr,
        transport: TransportProtocol,
    },
    /// 向 TU 层发送事件
    EmitEvent(TransactionEvent),
    /// 启动定时器
    StartTimer { timer: TimerEvent, delay_ms: u64 },
    /// 启动重传定时器（指数退避）
    StartRetransmitTimer {
        timer: TimerEvent,
        initial_delay_ms: u64,
        max_delay_ms: u64,
    },
    /// 取消指定事务的所有定时器
    CancelTimers { transaction_id: TransactionId },
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_id_from_branch_and_method() {
        let branch = BranchId("z9hG4bK-test".to_string());
        let method = Method::Invite;
        let id = TransactionId::from_branch_and_method(&branch, &method);
        assert_eq!(id.0, "z9hG4bK-test:INVITE");
    }

    #[test]
    fn test_transaction_id_display() {
        let id = TransactionId("test-id".to_string());
        assert_eq!(id.to_string(), "test-id");
    }

    #[test]
    fn test_transaction_key_new() {
        let branch = BranchId("z9hG4bK-abc".to_string());
        let key = TransactionKey::new(
            branch.clone(),
            Method::Invite,
            "example.com:5060".to_string(),
        );
        assert_eq!(key.branch_id, branch);
        assert_eq!(key.method, Method::Invite);
        assert_eq!(key.sent_by, "example.com:5060");
    }

    #[test]
    fn test_invite_client_state_display() {
        assert_eq!(InviteClientState::Calling.to_string(), "Calling");
        assert_eq!(InviteClientState::Proceeding.to_string(), "Proceeding");
        assert_eq!(InviteClientState::Completed.to_string(), "Completed");
        assert_eq!(InviteClientState::Terminated.to_string(), "Terminated");
    }

    #[test]
    fn test_non_invite_client_state_display() {
        assert_eq!(NonInviteClientState::Trying.to_string(), "Trying");
        assert_eq!(NonInviteClientState::Proceeding.to_string(), "Proceeding");
        assert_eq!(NonInviteClientState::Completed.to_string(), "Completed");
        assert_eq!(NonInviteClientState::Terminated.to_string(), "Terminated");
    }

    #[test]
    fn test_invite_server_state_display() {
        assert_eq!(InviteServerState::Proceeding.to_string(), "Proceeding");
        assert_eq!(InviteServerState::Confirmed.to_string(), "Confirmed");
        assert_eq!(InviteServerState::Completed.to_string(), "Completed");
        assert_eq!(InviteServerState::Terminated.to_string(), "Terminated");
    }

    #[test]
    fn test_non_invite_server_state_display() {
        assert_eq!(NonInviteServerState::Trying.to_string(), "Trying");
        assert_eq!(NonInviteServerState::Proceeding.to_string(), "Proceeding");
        assert_eq!(NonInviteServerState::Completed.to_string(), "Completed");
        assert_eq!(NonInviteServerState::Terminated.to_string(), "Terminated");
    }

    #[test]
    fn test_termination_reason_display() {
        assert_eq!(TerminationReason::Completed.to_string(), "Completed");
        assert_eq!(TerminationReason::Timeout.to_string(), "Timeout");
        assert_eq!(
            TerminationReason::TransportError.to_string(),
            "TransportError"
        );
        assert_eq!(TerminationReason::UserCancel.to_string(), "UserCancel");
    }
}
