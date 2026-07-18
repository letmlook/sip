//! SIP UA 事件类型
//!
//! 定义 UA 层向上层应用通知的事件类型，包括来电、呼出进展、
//! 呼叫建立、呼叫终止、注册结果和错误等。

use sip_core::SipError;

// ============================================================================
// SipEvent - SIP 事件
// ============================================================================

/// SIP 事件（向上层应用通知）
#[derive(Debug)]
pub enum SipEvent {
    /// 来电通知
    IncomingCall {
        /// 呼叫标识（Call-ID）
        call_id: String,
        /// 主叫方地址
        from: String,
        /// 被叫方地址
        to: String,
        /// 会话描述（如 SDP）
        body: Option<Vec<u8>>,
        /// 内容类型（如 application/sdp）
        content_type: Option<String>,
    },

    /// 呼出进展（如 180 Ringing）
    CallProgress {
        /// 呼叫标识
        call_id: String,
        /// SIP 状态码
        status_code: u16,
        /// 原因短语
        reason_phrase: String,
    },

    /// 呼叫建立
    CallEstablished {
        /// 呼叫标识
        call_id: String,
        /// 对话标识
        dialog_id: String,
        /// 会话描述
        body: Option<Vec<u8>>,
        /// 内容类型
        content_type: Option<String>,
    },

    /// 呼叫终止
    CallTerminated {
        /// 呼叫标识
        call_id: String,
        /// 终止原因
        reason: CallTerminationReason,
    },

    /// 注册结果
    RegistrationResult {
        /// 注册标识
        registration_id: String,
        /// 注册结果
        result: Result<(), String>,
    },

    /// 对话事件（来自对话层）
    DialogEvent(sip_dialog::DialogEvent),

    /// 注册事件（来自注册层）
    RegistrationEvent(sip_registration::RegistrationEvent),

    /// 错误
    Error(SipError),
}

// ============================================================================
// CallTerminationReason - 呼叫终止原因
// ============================================================================

/// 呼叫终止原因
#[derive(Debug, Clone)]
pub enum CallTerminationReason {
    /// 正常挂断（BYE）
    NormalBye,
    /// 超时（Timer B）
    Timeout,
    /// 网络不可达
    NetworkUnreachable,
    /// 对方忙线
    RemoteBusy,
    /// 重定向
    Redirected,
    /// 认证失败
    AuthenticationFailed,
    /// 已取消
    Cancelled,
    /// 其他错误
    Error(String),
}

impl std::fmt::Display for CallTerminationReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NormalBye => write!(f, "Normal BYE"),
            Self::Timeout => write!(f, "Timeout"),
            Self::NetworkUnreachable => write!(f, "Network unreachable"),
            Self::RemoteBusy => write!(f, "Remote busy"),
            Self::Redirected => write!(f, "Redirected"),
            Self::AuthenticationFailed => write!(f, "Authentication failed"),
            Self::Cancelled => write!(f, "Cancelled"),
            Self::Error(e) => write!(f, "Error: {}", e),
        }
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_termination_reason_display() {
        assert_eq!(CallTerminationReason::NormalBye.to_string(), "Normal BYE");
        assert_eq!(CallTerminationReason::Timeout.to_string(), "Timeout");
        assert_eq!(
            CallTerminationReason::NetworkUnreachable.to_string(),
            "Network unreachable"
        );
        assert_eq!(CallTerminationReason::RemoteBusy.to_string(), "Remote busy");
        assert_eq!(CallTerminationReason::Redirected.to_string(), "Redirected");
        assert_eq!(
            CallTerminationReason::AuthenticationFailed.to_string(),
            "Authentication failed"
        );
        assert_eq!(CallTerminationReason::Cancelled.to_string(), "Cancelled");
        assert_eq!(
            CallTerminationReason::Error("test".to_string()).to_string(),
            "Error: test"
        );
    }

    #[test]
    fn test_sip_event_incoming_call() {
        let event = SipEvent::IncomingCall {
            call_id: "call-123".to_string(),
            from: "sip:alice@example.com".to_string(),
            to: "sip:bob@example.com".to_string(),
            body: Some(b"SDP".to_vec()),
            content_type: Some("application/sdp".to_string()),
        };

        if let SipEvent::IncomingCall {
            call_id,
            from,
            to,
            body,
            content_type,
        } = event
        {
            assert_eq!(call_id, "call-123");
            assert_eq!(from, "sip:alice@example.com");
            assert_eq!(to, "sip:bob@example.com");
            assert_eq!(body.unwrap(), b"SDP".to_vec());
            assert_eq!(content_type.unwrap(), "application/sdp");
        } else {
            panic!("Expected IncomingCall event");
        }
    }

    #[test]
    fn test_sip_event_call_progress() {
        let event = SipEvent::CallProgress {
            call_id: "call-123".to_string(),
            status_code: 180,
            reason_phrase: "Ringing".to_string(),
        };

        if let SipEvent::CallProgress {
            call_id,
            status_code,
            reason_phrase,
        } = event
        {
            assert_eq!(call_id, "call-123");
            assert_eq!(status_code, 180);
            assert_eq!(reason_phrase, "Ringing");
        } else {
            panic!("Expected CallProgress event");
        }
    }

    #[test]
    fn test_sip_event_call_established() {
        let event = SipEvent::CallEstablished {
            call_id: "call-123".to_string(),
            dialog_id: "dialog-456".to_string(),
            body: None,
            content_type: None,
        };

        if let SipEvent::CallEstablished {
            call_id, dialog_id, ..
        } = event
        {
            assert_eq!(call_id, "call-123");
            assert_eq!(dialog_id, "dialog-456");
        } else {
            panic!("Expected CallEstablished event");
        }
    }

    #[test]
    fn test_sip_event_call_terminated() {
        let event = SipEvent::CallTerminated {
            call_id: "call-123".to_string(),
            reason: CallTerminationReason::NormalBye,
        };

        if let SipEvent::CallTerminated { call_id, reason } = event {
            assert_eq!(call_id, "call-123");
            assert!(matches!(reason, CallTerminationReason::NormalBye));
        } else {
            panic!("Expected CallTerminated event");
        }
    }

    #[test]
    fn test_sip_event_registration_result() {
        let event = SipEvent::RegistrationResult {
            registration_id: "reg-789".to_string(),
            result: Ok(()),
        };

        if let SipEvent::RegistrationResult {
            registration_id,
            result,
        } = event
        {
            assert_eq!(registration_id, "reg-789");
            assert!(result.is_ok());
        } else {
            panic!("Expected RegistrationResult event");
        }
    }
}
