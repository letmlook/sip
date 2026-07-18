//! SIP 注册状态类型定义
//!
//! 定义注册唯一标识、注册状态枚举、注册信息、联系地址信息、
//! 注册事件和注册失败原因等核心类型。

use std::fmt;
use std::time::Instant;

use sip_core::CSeqNumber;
use sip_message::CallId;

// ============================================================================
// RegistrationId - 注册唯一标识
// ============================================================================

/// 注册唯一标识
///
/// 每个注册会话拥有唯一的 `RegistrationId`，用于在注册管理器中
/// 标识和查找特定的注册记录。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RegistrationId(pub String);

impl RegistrationId {
    /// 生成随机的注册标识
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().simple().to_string())
    }
}

impl Default for RegistrationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RegistrationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ============================================================================
// RegistrationState - 注册状态
// ============================================================================

/// 注册状态
///
/// 表示注册的生命周期状态，遵循以下状态流转：
/// ```text
/// Unregistered → Registering → Registered → Unregistering → Unregistered
///                                     ↓
///                                  Expired
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationState {
    /// 未注册
    Unregistered,
    /// 正在注册中
    Registering,
    /// 已注册
    Registered,
    /// 正在注销中
    Unregistering,
    /// 注册已过期
    Expired,
}

impl fmt::Display for RegistrationState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unregistered => write!(f, "Unregistered"),
            Self::Registering => write!(f, "Registering"),
            Self::Registered => write!(f, "Registered"),
            Self::Unregistering => write!(f, "Unregistering"),
            Self::Expired => write!(f, "Expired"),
        }
    }
}

// ============================================================================
// ContactInfo - 联系地址信息
// ============================================================================

/// 联系地址信息
///
/// 描述注册的 Contact 头部信息，包含 URI 和可选的 expires 参数。
#[derive(Debug, Clone)]
pub struct ContactInfo {
    /// 联系地址 URI
    pub uri: String,
    /// expires 参数（秒），None 表示使用默认值
    pub expires: Option<u32>,
}

// ============================================================================
// RegistrationInfo - 注册信息
// ============================================================================

/// 注册信息
///
/// 保存一次注册会话的完整上下文信息，包括标识、状态、AOR、
/// 联系地址、Call-ID、CSeq 序列号、有效期和注册时间等。
#[derive(Debug, Clone)]
pub struct RegistrationInfo {
    /// 注册唯一标识
    pub id: RegistrationId,
    /// 当前注册状态
    pub state: RegistrationState,
    /// 地址记录（Address-of-Record），格式 sip:user@domain
    pub aor: String,
    /// 联系地址列表
    pub contacts: Vec<ContactInfo>,
    /// Call-ID，同一 AOR 的所有注册使用相同的 Call-ID
    pub call_id: CallId,
    /// CSeq 序列号，每次重新注册递增
    pub cseq: CSeqNumber,
    /// 注册有效期（秒）
    pub expires: u64,
    /// 注册成功时间
    pub registered_at: Option<Instant>,
    /// From 头部 tag
    pub from_tag: sip_message::Tag,
    /// 本端联系地址 URI
    pub local_contact: String,
    /// 注册服务器地址
    pub registrar: String,
    /// 是否已尝试过摘要认证
    pub auth_attempted: bool,
    /// nonce 计数（用于摘要认证）
    pub nonce_count: u32,
    /// 第三方注册时的注册者 AOR（From 头部地址）
    pub third_party_from: Option<String>,
}

impl RegistrationInfo {
    /// 计算注册剩余有效时间（秒）
    ///
    /// 如果尚未注册成功或已过期，返回 None。
    pub fn remaining_expires(&self) -> Option<u64> {
        self.registered_at.map(|at| {
            let elapsed = at.elapsed().as_secs();
            self.expires.saturating_sub(elapsed)
        })
    }

    /// 判断是否需要刷新注册
    ///
    /// 当注册有效期剩余不足 50% 时返回 true。
    pub fn needs_refresh(&self, threshold: f32) -> bool {
        if let Some(remaining) = self.remaining_expires() {
            let ratio = remaining as f32 / self.expires as f32;
            ratio < threshold
        } else {
            false
        }
    }
}

// ============================================================================
// RegistrationEvent - 注册事件
// ============================================================================

/// 注册事件
///
/// 注册管理器向 TU 层发送的事件通知，用于指示注册状态变化。
#[derive(Debug)]
pub enum RegistrationEvent {
    /// 注册成功
    Registered {
        /// 注册标识
        registration_id: RegistrationId,
    },
    /// 注册失败
    RegistrationFailed {
        /// 注册标识
        registration_id: RegistrationId,
        /// 失败原因
        reason: RegistrationFailureReason,
    },
    /// 注销成功
    Unregistered {
        /// 注册标识
        registration_id: RegistrationId,
    },
    /// 注册已过期
    Expired {
        /// 注册标识
        registration_id: RegistrationId,
    },
    /// 认证失败
    AuthenticationFailed {
        /// 注册标识
        registration_id: RegistrationId,
    },
}

// ============================================================================
// RegistrationFailureReason - 注册失败原因
// ============================================================================

/// 注册失败原因
#[derive(Debug, Clone)]
pub enum RegistrationFailureReason {
    /// 网络不可达
    NetworkUnreachable,
    /// 认证失败
    AuthenticationFailed,
    /// 服务器拒绝
    ServerRejected,
    /// 超时
    Timeout,
    /// 传输错误
    TransportError(String),
}

impl fmt::Display for RegistrationFailureReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NetworkUnreachable => write!(f, "Network unreachable"),
            Self::AuthenticationFailed => write!(f, "Authentication failed"),
            Self::ServerRejected => write!(f, "Server rejected"),
            Self::Timeout => write!(f, "Timeout"),
            Self::TransportError(e) => write!(f, "Transport error: {}", e),
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
    fn test_registration_id_new() {
        let id1 = RegistrationId::new();
        let id2 = RegistrationId::new();
        assert_ne!(id1, id2);
        assert!(!id1.0.is_empty());
    }

    #[test]
    fn test_registration_id_default() {
        let id = RegistrationId::default();
        assert!(!id.0.is_empty());
    }

    #[test]
    fn test_registration_id_display() {
        let id = RegistrationId("test-id".to_string());
        assert_eq!(id.to_string(), "test-id");
    }

    #[test]
    fn test_registration_state_display() {
        assert_eq!(RegistrationState::Unregistered.to_string(), "Unregistered");
        assert_eq!(RegistrationState::Registering.to_string(), "Registering");
        assert_eq!(RegistrationState::Registered.to_string(), "Registered");
        assert_eq!(
            RegistrationState::Unregistering.to_string(),
            "Unregistering"
        );
        assert_eq!(RegistrationState::Expired.to_string(), "Expired");
    }

    #[test]
    fn test_registration_state_transitions() {
        // 正常流转：Unregistered → Registering → Registered → Unregistering → Unregistered
        let state = RegistrationState::Unregistered;
        assert_eq!(state, RegistrationState::Unregistered);

        let state = RegistrationState::Registering;
        assert_eq!(state, RegistrationState::Registering);

        let state = RegistrationState::Registered;
        assert_eq!(state, RegistrationState::Registered);

        let state = RegistrationState::Unregistering;
        assert_eq!(state, RegistrationState::Unregistering);

        let state = RegistrationState::Unregistered;
        assert_eq!(state, RegistrationState::Unregistered);
    }

    #[test]
    fn test_contact_info() {
        let contact = ContactInfo {
            uri: "sip:alice@192.168.1.1:5060".to_string(),
            expires: Some(3600),
        };
        assert_eq!(contact.uri, "sip:alice@192.168.1.1:5060");
        assert_eq!(contact.expires, Some(3600));
    }

    #[test]
    fn test_registration_info_remaining_expires() {
        let info = RegistrationInfo {
            id: RegistrationId::new(),
            state: RegistrationState::Registered,
            aor: "sip:alice@example.com".to_string(),
            contacts: vec![],
            call_id: CallId::new(),
            cseq: CSeqNumber(1),
            expires: 3600,
            registered_at: Some(Instant::now()),
            from_tag: sip_message::Tag::new(),
            local_contact: "sip:alice@192.168.1.1:5060".to_string(),
            registrar: "sip:example.com".to_string(),
            auth_attempted: false,
            nonce_count: 0,
            third_party_from: None,
        };

        // 刚注册，剩余时间应接近 3600
        let remaining = info.remaining_expires().unwrap();
        assert!(remaining <= 3600);
        assert!(remaining > 3500);
    }

    #[test]
    fn test_registration_info_needs_refresh() {
        let info = RegistrationInfo {
            id: RegistrationId::new(),
            state: RegistrationState::Registered,
            aor: "sip:alice@example.com".to_string(),
            contacts: vec![],
            call_id: CallId::new(),
            cseq: CSeqNumber(1),
            expires: 3600,
            registered_at: Some(Instant::now()),
            from_tag: sip_message::Tag::new(),
            local_contact: "sip:alice@192.168.1.1:5060".to_string(),
            registrar: "sip:example.com".to_string(),
            auth_attempted: false,
            nonce_count: 0,
            third_party_from: None,
        };

        // 刚注册，不需要刷新
        assert!(!info.needs_refresh(0.5));
    }

    #[test]
    fn test_registration_info_no_remaining_when_not_registered() {
        let info = RegistrationInfo {
            id: RegistrationId::new(),
            state: RegistrationState::Unregistered,
            aor: "sip:alice@example.com".to_string(),
            contacts: vec![],
            call_id: CallId::new(),
            cseq: CSeqNumber(1),
            expires: 3600,
            registered_at: None,
            from_tag: sip_message::Tag::new(),
            local_contact: "sip:alice@192.168.1.1:5060".to_string(),
            registrar: "sip:example.com".to_string(),
            auth_attempted: false,
            nonce_count: 0,
            third_party_from: None,
        };

        assert!(info.remaining_expires().is_none());
    }

    #[test]
    fn test_failure_reason_display() {
        assert_eq!(
            RegistrationFailureReason::NetworkUnreachable.to_string(),
            "Network unreachable"
        );
        assert_eq!(
            RegistrationFailureReason::AuthenticationFailed.to_string(),
            "Authentication failed"
        );
        assert_eq!(
            RegistrationFailureReason::ServerRejected.to_string(),
            "Server rejected"
        );
        assert_eq!(RegistrationFailureReason::Timeout.to_string(), "Timeout");
        assert_eq!(
            RegistrationFailureReason::TransportError("io error".to_string()).to_string(),
            "Transport error: io error"
        );
    }
}
