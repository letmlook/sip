//! 对话标识与状态类型
//!
//! 定义 SIP 对话层的核心类型，包括对话标识、对话状态、对话信息和对话事件。
//! 按照 RFC 3261 Section 12 实现。

use std::fmt;

use sip_core::{CSeqNumber, CallId, Tag};
use sip_message::{RouteHeader, SipUri};

// ============================================================================
// DialogId - 对话标识
// ============================================================================

/// 对话标识（Call-ID + LocalTag + RemoteTag 三元组）
///
/// 按照 RFC 3261 Section 12 的定义，对话由 Call-ID、本地 Tag 和远端 Tag
/// 三部分唯一标识。在 UAC 侧，本地 Tag 来自 From 头部，远端 Tag 来自 To 头部；
/// 在 UAS 侧，本地 Tag 来自 To 头部，远端 Tag 来自 From 头部。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DialogId {
    /// Call-ID 头部值
    pub call_id: CallId,
    /// 本地 Tag（UAC 侧为 From Tag，UAS 侧为 To Tag）
    pub local_tag: Tag,
    /// 远端 Tag（UAC 侧为 To Tag，UAS 侧为 From Tag）
    pub remote_tag: Tag,
}

impl DialogId {
    /// 创建新的对话标识
    pub fn new(call_id: CallId, local_tag: Tag, remote_tag: Tag) -> Self {
        Self {
            call_id,
            local_tag,
            remote_tag,
        }
    }
}

impl fmt::Display for DialogId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.call_id, self.local_tag, self.remote_tag)
    }
}

// ============================================================================
// DialogState - 对话状态
// ============================================================================

/// 对话状态
///
/// 按照 RFC 3261 Section 12，对话可以处于以下三种状态之一：
/// - **Early**：早期对话，在收到/发送临时响应（1xx，含 To Tag）后创建
/// - **Confirmed**：确认对话，在收到/发送成功响应（2xx）后创建或升级
/// - **Terminated**：已终止对话，在发送/收到 BYE 后转换
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogState {
    /// 早期对话
    Early,
    /// 确认对话
    Confirmed,
    /// 已终止对话
    Terminated,
}

impl fmt::Display for DialogState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Early => write!(f, "Early"),
            Self::Confirmed => write!(f, "Confirmed"),
            Self::Terminated => write!(f, "Terminated"),
        }
    }
}

// ============================================================================
// DialogInfo - 对话信息
// ============================================================================

/// 对话信息
///
/// 包含对话的完整状态信息，按照 RFC 3261 Section 12 定义。
/// 包括对话标识、状态、URI、序列号、远端目标、路由集等。
#[derive(Debug, Clone)]
pub struct DialogInfo {
    /// 对话标识
    pub id: DialogId,
    /// 对话状态
    pub state: DialogState,
    /// 本地 URI（UAC 侧为 From URI，UAS 侧为 To URI）
    pub local_uri: SipUri,
    /// 远端 URI（UAC 侧为 To URI，UAS 侧为 From URI）
    pub remote_uri: SipUri,
    /// 本地 CSeq 序列号
    pub local_cseq: CSeqNumber,
    /// 远端 CSeq 序列号
    pub remote_cseq: CSeqNumber,
    /// 远端目标（从 Contact 头部获取，用于后续请求的 Request-URI）
    pub remote_target: Option<SipUri>,
    /// 路由集（UAC 侧为 2xx 响应中 Record-Route 的逆序，UAS 侧为 INVITE 中 Record-Route 的正序）
    pub route_set: Vec<RouteHeader>,
    /// 是否为 UAC 侧对话
    pub is_uac: bool,
}

impl DialogInfo {
    /// 递增本地 CSeq 序列号
    ///
    /// 在对话内发送新请求时调用，返回递增后的序列号。
    pub fn increment_local_cseq(&mut self) -> CSeqNumber {
        self.local_cseq.0 = self.local_cseq.0.saturating_add(1);
        self.local_cseq
    }

    /// 更新远端 CSeq 序列号
    ///
    /// 收到对话内请求时调用，仅在新的 CSeq 大于当前值时更新。
    /// 返回是否成功更新。
    pub fn update_remote_cseq(&mut self, new_cseq: CSeqNumber) -> bool {
        if new_cseq.0 > self.remote_cseq.0 {
            self.remote_cseq = new_cseq;
            true
        } else {
            false
        }
    }

    /// 更新远端目标
    ///
    /// 从 2xx 响应的 Contact 头部获取新的远端目标。
    pub fn update_remote_target(&mut self, target: SipUri) {
        self.remote_target = Some(target);
    }

    /// 获取请求的 Request-URI
    ///
    /// 根据路由集决定 Request-URI：
    /// - 路由集为空 → 使用远端目标
    /// - 路由集非空且首条路由含 lr 参数 → 使用远端目标
    /// - 路由集非空且首条路由不含 lr 参数 → 使用首条路由 URI
    pub fn request_uri(&self) -> Option<SipUri> {
        if self.route_set.is_empty() {
            self.remote_target.clone()
        } else {
            let first_route = &self.route_set[0];
            if first_route.uri.lr() {
                // 松散路由：Request-URI 为远端目标
                self.remote_target.clone()
            } else {
                // 严格路由：Request-URI 为首条路由 URI
                Some(first_route.uri.clone())
            }
        }
    }

    /// 获取请求的 Route 头部列表
    ///
    /// 根据路由集和路由类型决定 Route 头部：
    /// - 松散路由（lr）→ 路由集原样作为 Route 头部
    /// - 严格路由（无 lr）→ 去掉首条路由，远端目标追加到末尾
    pub fn route_headers_for_request(&self) -> Vec<RouteHeader> {
        if self.route_set.is_empty() {
            return Vec::new();
        }

        let first_route = &self.route_set[0];
        if first_route.uri.lr() {
            // 松散路由：路由集原样作为 Route 头部
            self.route_set.clone()
        } else {
            // 严格路由：去掉首条路由，远端目标追加到末尾
            let mut routes = self.route_set[1..].to_vec();
            if let Some(ref target) = self.remote_target {
                routes.push(RouteHeader::new(target.clone()));
            }
            routes
        }
    }
}

// ============================================================================
// DialogEvent - 对话事件
// ============================================================================

/// 对话事件
///
/// 对话层向 TU 层发送的事件通知，包括对话创建、确认、终止和更新。
#[derive(Debug)]
pub enum DialogEvent {
    /// 对话创建（早期或确认）
    DialogCreated {
        /// 对话标识
        dialog_id: DialogId,
    },
    /// 对话确认（早期对话升级为确认对话）
    DialogConfirmed {
        /// 对话标识
        dialog_id: DialogId,
    },
    /// 对话终止
    DialogTerminated {
        /// 对话标识
        dialog_id: DialogId,
        /// 终止原因
        reason: String,
    },
    /// 对话更新（远端目标或路由集变更）
    DialogUpdated {
        /// 对话标识
        dialog_id: DialogId,
    },
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialog_id_new() {
        let id = DialogId::new(
            "call-123".to_string(),
            "local-tag".to_string(),
            "remote-tag".to_string(),
        );
        assert_eq!(id.call_id, "call-123");
        assert_eq!(id.local_tag, "local-tag");
        assert_eq!(id.remote_tag, "remote-tag");
    }

    #[test]
    fn test_dialog_id_display() {
        let id = DialogId::new(
            "call-123".to_string(),
            "local-tag".to_string(),
            "remote-tag".to_string(),
        );
        assert_eq!(id.to_string(), "call-123:local-tag:remote-tag");
    }

    #[test]
    fn test_dialog_id_equality() {
        let id1 = DialogId::new("a".to_string(), "b".to_string(), "c".to_string());
        let id2 = DialogId::new("a".to_string(), "b".to_string(), "c".to_string());
        let id3 = DialogId::new("a".to_string(), "b".to_string(), "d".to_string());
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_dialog_id_hash() {
        use std::collections::HashMap;
        let id = DialogId::new("a".to_string(), "b".to_string(), "c".to_string());
        let mut map = HashMap::new();
        map.insert(id.clone(), 1);
        assert_eq!(map.get(&id), Some(&1));
    }

    #[test]
    fn test_dialog_state_display() {
        assert_eq!(DialogState::Early.to_string(), "Early");
        assert_eq!(DialogState::Confirmed.to_string(), "Confirmed");
        assert_eq!(DialogState::Terminated.to_string(), "Terminated");
    }

    #[test]
    fn test_dialog_state_ordering() {
        assert!(DialogState::Early != DialogState::Confirmed);
        assert!(DialogState::Confirmed != DialogState::Terminated);
    }

    #[test]
    fn test_dialog_info_increment_local_cseq() {
        let uri = SipUri::parse("sip:alice@example.com").unwrap();
        let mut info = create_test_dialog_info(uri.clone(), true);
        assert_eq!(info.local_cseq.0, 1);

        let cseq = info.increment_local_cseq();
        assert_eq!(cseq.0, 2);
        assert_eq!(info.local_cseq.0, 2);

        info.increment_local_cseq();
        assert_eq!(info.local_cseq.0, 3);
    }

    #[test]
    fn test_dialog_info_update_remote_cseq() {
        let uri = SipUri::parse("sip:alice@example.com").unwrap();
        let mut info = create_test_dialog_info(uri.clone(), false);
        assert_eq!(info.remote_cseq.0, 1);

        // 更新为更大的值 → 成功
        assert!(info.update_remote_cseq(CSeqNumber(2)));
        assert_eq!(info.remote_cseq.0, 2);

        // 更新为更小的值 → 失败
        assert!(!info.update_remote_cseq(CSeqNumber(1)));
        assert_eq!(info.remote_cseq.0, 2);

        // 更新为相等的值 → 失败
        assert!(!info.update_remote_cseq(CSeqNumber(2)));
        assert_eq!(info.remote_cseq.0, 2);
    }

    #[test]
    fn test_dialog_info_update_remote_target() {
        let uri = SipUri::parse("sip:alice@example.com").unwrap();
        let mut info = create_test_dialog_info(uri.clone(), true);
        assert!(info.remote_target.is_some());

        let new_target = SipUri::parse("sip:bob@newhost.com").unwrap();
        info.update_remote_target(new_target.clone());
        assert_eq!(info.remote_target.unwrap(), new_target);
    }

    #[test]
    fn test_dialog_info_request_uri_empty_route_set() {
        let uri = SipUri::parse("sip:alice@example.com").unwrap();
        let mut info = create_test_dialog_info(uri.clone(), true);
        info.route_set.clear();

        // 空路由集 → 使用远端目标
        let request_uri = info.request_uri();
        assert!(request_uri.is_some());
        assert_eq!(request_uri.unwrap(), info.remote_target.unwrap());
    }

    #[test]
    fn test_dialog_info_request_uri_loose_routing() {
        let uri = SipUri::parse("sip:alice@example.com").unwrap();
        let mut info = create_test_dialog_info(uri.clone(), true);

        // 添加含 lr 参数的路由
        let route_uri = SipUri::parse("sip:proxy.example.com;lr").unwrap();
        info.route_set.push(RouteHeader::new(route_uri));

        // 松散路由 → 使用远端目标
        let request_uri = info.request_uri();
        assert!(request_uri.is_some());
        assert_eq!(request_uri.unwrap(), info.remote_target.unwrap());
    }

    #[test]
    fn test_dialog_info_request_uri_strict_routing() {
        let uri = SipUri::parse("sip:alice@example.com").unwrap();
        let mut info = create_test_dialog_info(uri.clone(), true);

        // 添加不含 lr 参数的路由（严格路由）
        let route_uri = SipUri::parse("sip:proxy.example.com").unwrap();
        info.route_set.push(RouteHeader::new(route_uri.clone()));

        // 严格路由 → 使用首条路由 URI
        let request_uri = info.request_uri();
        assert!(request_uri.is_some());
        assert_eq!(request_uri.unwrap(), route_uri);
    }

    // ---- 辅助函数 ----

    fn create_test_dialog_info(uri: SipUri, is_uac: bool) -> DialogInfo {
        let remote_target = SipUri::parse("sip:bob@example.com").unwrap();
        DialogInfo {
            id: DialogId::new(
                "test-call-id".to_string(),
                "local-tag".to_string(),
                "remote-tag".to_string(),
            ),
            state: DialogState::Confirmed,
            local_uri: uri,
            remote_uri: SipUri::parse("sip:bob@example.com").unwrap(),
            local_cseq: CSeqNumber(1),
            remote_cseq: CSeqNumber(1),
            remote_target: Some(remote_target),
            route_set: Vec::new(),
            is_uac,
        }
    }
}
