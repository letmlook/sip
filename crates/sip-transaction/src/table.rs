//! 事务匹配与 ACK 处理
//!
//! 实现基于 Branch ID + Method 的事务匹配，
//! 以及 INVITE 事务的 ACK 处理逻辑。

use std::collections::HashMap;

use sip_message::{Method, SipRequest, SipResponse};

use crate::event::{TransactionId, TransactionKey};
use crate::invite_client::InviteClientTransaction;
use crate::invite_server::InviteServerTransaction;
use crate::non_invite_client::NonInviteClientTransaction;
use crate::non_invite_server::NonInviteServerTransaction;

// ============================================================================
// ClientTransaction - 客户端事务枚举
// ============================================================================

/// 客户端事务（INVITE 或非 INVITE）
pub enum ClientTransaction {
    /// INVITE 客户端事务
    Invite(InviteClientTransaction),
    /// 非 INVITE 客户端事务
    NonInvite(NonInviteClientTransaction),
}

impl ClientTransaction {
    /// 获取事务 ID
    pub fn id(&self) -> &TransactionId {
        match self {
            Self::Invite(tx) => tx.id(),
            Self::NonInvite(tx) => tx.id(),
        }
    }

    /// 获取事务匹配键
    pub fn key(&self) -> &TransactionKey {
        match self {
            Self::Invite(tx) => tx.key(),
            Self::NonInvite(tx) => tx.key(),
        }
    }

    /// 判断是否已终止
    pub fn is_terminated(&self) -> bool {
        match self {
            Self::Invite(tx) => tx.state() == crate::event::InviteClientState::Terminated,
            Self::NonInvite(tx) => tx.state() == crate::event::NonInviteClientState::Terminated,
        }
    }
}

// ============================================================================
// ServerTransaction - 服务端事务枚举
// ============================================================================

/// 服务端事务（INVITE 或非 INVITE）
pub enum ServerTransaction {
    /// INVITE 服务端事务
    Invite(InviteServerTransaction),
    /// 非 INVITE 服务端事务
    NonInvite(NonInviteServerTransaction),
}

impl ServerTransaction {
    /// 获取事务 ID
    pub fn id(&self) -> &TransactionId {
        match self {
            Self::Invite(tx) => tx.id(),
            Self::NonInvite(tx) => tx.id(),
        }
    }

    /// 获取事务匹配键
    pub fn key(&self) -> &TransactionKey {
        match self {
            Self::Invite(tx) => tx.key(),
            Self::NonInvite(tx) => tx.key(),
        }
    }

    /// 判断是否已终止
    pub fn is_terminated(&self) -> bool {
        match self {
            Self::Invite(tx) => tx.state() == crate::event::InviteServerState::Terminated,
            Self::NonInvite(tx) => tx.state() == crate::event::NonInviteServerState::Terminated,
        }
    }
}

// ============================================================================
// TransactionTable - 事务匹配表
// ============================================================================

/// 事务匹配表
///
/// 维护客户端和服务端事务的映射关系，
/// 支持基于 Branch ID + Method 的事务匹配。
pub struct TransactionTable {
    /// 客户端事务（按 TransactionKey 索引）
    pub client_transactions: HashMap<TransactionKey, ClientTransaction>,
    /// 服务端事务（按 TransactionKey 索引）
    pub server_transactions: HashMap<TransactionKey, ServerTransaction>,
    /// 客户端事务 ID 到 Key 的映射
    pub client_id_to_key: HashMap<TransactionId, TransactionKey>,
    /// 服务端事务 ID 到 Key 的映射
    pub server_id_to_key: HashMap<TransactionId, TransactionKey>,
}

impl TransactionTable {
    /// 创建空的事务匹配表
    pub fn new() -> Self {
        Self {
            client_transactions: HashMap::new(),
            server_transactions: HashMap::new(),
            client_id_to_key: HashMap::new(),
            server_id_to_key: HashMap::new(),
        }
    }

    /// 添加客户端事务
    pub fn insert_client(&mut self, transaction: ClientTransaction) {
        let key = transaction.key().clone();
        let id = transaction.id().clone();
        self.client_id_to_key.insert(id, key.clone());
        self.client_transactions.insert(key, transaction);
    }

    /// 添加服务端事务
    pub fn insert_server(&mut self, transaction: ServerTransaction) {
        let key = transaction.key().clone();
        let id = transaction.id().clone();
        self.server_id_to_key.insert(id, key.clone());
        self.server_transactions.insert(key, transaction);
    }

    /// 根据响应匹配客户端事务
    ///
    /// SIP 响应通过 Via 头部的 branch 参数和 CSeq 方法匹配客户端事务。
    pub fn match_client(&self, response: &SipResponse) -> Option<&ClientTransaction> {
        let key = TransactionKey::from_response(response)?;
        self.client_transactions.get(&key)
    }

    /// 根据响应匹配客户端事务（可变引用）
    pub fn match_client_mut(&mut self, response: &SipResponse) -> Option<&mut ClientTransaction> {
        let key = TransactionKey::from_response(response)?;
        self.client_transactions.get_mut(&key)
    }

    /// 根据请求匹配服务端事务
    ///
    /// SIP 请求通过 Via 头部的 branch 参数和请求方法匹配服务端事务。
    /// 对于 ACK 请求，需要特殊处理：匹配对应的 INVITE 服务端事务。
    pub fn match_server(&self, request: &SipRequest) -> Option<&ServerTransaction> {
        let key = self.build_key_for_request(request)?;
        self.server_transactions.get(&key)
    }

    /// 根据请求匹配服务端事务（可变引用）
    pub fn match_server_mut(&mut self, request: &SipRequest) -> Option<&mut ServerTransaction> {
        let key = self.build_key_for_request(request)?;
        self.server_transactions.get_mut(&key)
    }

    /// 根据 TransactionId 获取客户端事务
    pub fn get_client_by_id(&self, id: &TransactionId) -> Option<&ClientTransaction> {
        let key = self.client_id_to_key.get(id)?;
        self.client_transactions.get(key)
    }

    /// 根据 TransactionId 获取客户端事务（可变引用）
    pub fn get_client_by_id_mut(&mut self, id: &TransactionId) -> Option<&mut ClientTransaction> {
        let key = self.client_id_to_key.get(id)?.clone();
        self.client_transactions.get_mut(&key)
    }

    /// 根据 TransactionId 获取服务端事务
    pub fn get_server_by_id(&self, id: &TransactionId) -> Option<&ServerTransaction> {
        let key = self.server_id_to_key.get(id)?;
        self.server_transactions.get(key)
    }

    /// 根据 TransactionId 获取服务端事务（可变引用）
    pub fn get_server_by_id_mut(&mut self, id: &TransactionId) -> Option<&mut ServerTransaction> {
        let key = self.server_id_to_key.get(id)?.clone();
        self.server_transactions.get_mut(&key)
    }

    /// 移除已终止的客户端事务
    pub fn remove_client(&mut self, id: &TransactionId) -> Option<ClientTransaction> {
        if let Some(key) = self.client_id_to_key.remove(id) {
            self.client_transactions.remove(&key)
        } else {
            None
        }
    }

    /// 移除已终止的服务端事务
    pub fn remove_server(&mut self, id: &TransactionId) -> Option<ServerTransaction> {
        if let Some(key) = self.server_id_to_key.remove(id) {
            self.server_transactions.remove(&key)
        } else {
            None
        }
    }

    /// 清理所有已终止的事务
    pub fn cleanup_terminated(&mut self) {
        // 清理客户端事务
        let terminated_client_keys: Vec<TransactionKey> = self
            .client_transactions
            .iter()
            .filter(|(_, tx)| tx.is_terminated())
            .map(|(k, _)| k.clone())
            .collect();

        for key in &terminated_client_keys {
            if let Some(tx) = self.client_transactions.remove(key) {
                self.client_id_to_key.remove(tx.id());
            }
        }

        // 清理服务端事务
        let terminated_server_keys: Vec<TransactionKey> = self
            .server_transactions
            .iter()
            .filter(|(_, tx)| tx.is_terminated())
            .map(|(k, _)| k.clone())
            .collect();

        for key in &terminated_server_keys {
            if let Some(tx) = self.server_transactions.remove(key) {
                self.server_id_to_key.remove(tx.id());
            }
        }
    }

    /// 获取客户端事务数量
    pub fn client_count(&self) -> usize {
        self.client_transactions.len()
    }

    /// 获取服务端事务数量
    pub fn server_count(&self) -> usize {
        self.server_transactions.len()
    }

    /// 清空所有事务
    pub fn clear(&mut self) {
        self.client_transactions.clear();
        self.server_transactions.clear();
        self.client_id_to_key.clear();
        self.server_id_to_key.clear();
    }

    // ========================================================================
    // 内部方法
    // ========================================================================

    /// 为请求构建匹配键
    ///
    /// 对于 ACK 请求，使用 INVITE 方法匹配对应的 INVITE 服务端事务。
    fn build_key_for_request(&self, request: &SipRequest) -> Option<TransactionKey> {
        let mut key = TransactionKey::from_request(request)?;

        // ACK 请求需要匹配 INVITE 服务端事务
        // 将方法从 ACK 替换为 INVITE
        if request.request_line.method == Method::Ack {
            key.method = Method::Invite;
        }

        Some(key)
    }
}

impl Default for TransactionTable {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// ACK 处理辅助函数
// ============================================================================

/// 判断响应是否需要事务层内发送 ACK
///
/// 对于 INVITE 事务收到 3xx-6xx 响应，ACK 在事务层内发送。
/// 对于 INVITE 事务收到 2xx 响应，ACK 由 TU 构造通过新事务发送。
pub fn is_ack_handled_by_transaction_layer(response: &SipResponse) -> bool {
    let status_code = response.status_line.status_code;
    !status_code.is_provisional() && !status_code.is_success()
}

/// 判断响应是否需要 TU 层发送 ACK
///
/// 对于 INVITE 事务收到 2xx 响应，ACK 由 TU 构造通过新事务发送。
pub fn is_ack_handled_by_tu(response: &SipResponse) -> bool {
    response.status_line.status_code.is_success()
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use sip_core::SipVersion;
    use sip_core::{Host, StatusCode, TransportProtocol};
    use sip_message::uri::SipUri;
    use sip_message::{
        BranchId, CSeqHeader, CallId, HeaderCollection, HeaderName, HeaderValue, Method,
        RequestLine, StatusLine, ViaHeader,
    };
    use std::net::SocketAddr;

    fn create_test_invite() -> sip_message::SipRequest {
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

        sip_message::SipRequest {
            request_line: RequestLine {
                method: Method::Invite,
                request_uri: uri,
                version: SipVersion,
            },
            headers,
            body: None,
        }
    }

    fn create_test_response(status_code: u16, branch: &BranchId) -> sip_message::SipResponse {
        let mut headers = HeaderCollection::new();
        let mut via = ViaHeader::new(
            TransportProtocol::Udp,
            Host::Domain("192.168.1.1".to_string()),
            Some(5060),
        );
        via.branch = branch.clone();
        headers.insert(HeaderName::Via, HeaderValue::Via(via));
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

        sip_message::SipResponse {
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
    fn test_transaction_table_new() {
        let table = TransactionTable::new();
        assert_eq!(table.client_count(), 0);
        assert_eq!(table.server_count(), 0);
    }

    #[test]
    fn test_insert_and_match_client() {
        let mut table = TransactionTable::new();
        let request = create_test_invite();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();

        let tx = InviteClientTransaction::new(request.clone(), dest, TransportProtocol::Udp);
        let branch = tx.key().branch_id.clone();
        table.insert_client(ClientTransaction::Invite(tx));

        assert_eq!(table.client_count(), 1);

        let response = create_test_response(180, &branch);
        let matched = table.match_client(&response);
        assert!(matched.is_some());
    }

    #[test]
    fn test_insert_and_match_server() {
        let mut table = TransactionTable::new();
        let request = create_test_invite();
        let source: SocketAddr = "192.168.1.1:5060".parse().unwrap();

        let tx = InviteServerTransaction::new(request.clone(), source, TransportProtocol::Udp);
        table.insert_server(ServerTransaction::Invite(tx));

        assert_eq!(table.server_count(), 1);

        let matched = table.match_server(&request);
        assert!(matched.is_some());
    }

    #[test]
    fn test_no_match_returns_none() {
        let table = TransactionTable::new();
        let branch = BranchId::new();
        let response = create_test_response(200, &branch);
        assert!(table.match_client(&response).is_none());
    }

    #[test]
    fn test_remove_client() {
        let mut table = TransactionTable::new();
        let request = create_test_invite();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();

        let tx = InviteClientTransaction::new(request, dest, TransportProtocol::Udp);
        let id = tx.id().clone();
        table.insert_client(ClientTransaction::Invite(tx));

        assert_eq!(table.client_count(), 1);

        let removed = table.remove_client(&id);
        assert!(removed.is_some());
        assert_eq!(table.client_count(), 0);
    }

    #[test]
    fn test_is_ack_handled_by_transaction_layer() {
        let branch = BranchId::new();
        let response_200 = create_test_response(200, &branch);
        let response_404 = create_test_response(404, &branch);

        assert!(!is_ack_handled_by_transaction_layer(&response_200));
        assert!(is_ack_handled_by_transaction_layer(&response_404));
    }

    #[test]
    fn test_is_ack_handled_by_tu() {
        let branch = BranchId::new();
        let response_200 = create_test_response(200, &branch);
        let response_404 = create_test_response(404, &branch);

        assert!(is_ack_handled_by_tu(&response_200));
        assert!(!is_ack_handled_by_tu(&response_404));
    }

    #[test]
    fn test_cleanup_terminated() {
        let mut table = TransactionTable::new();
        let request = create_test_invite();
        let dest: SocketAddr = "192.168.1.1:5060".parse().unwrap();

        let tx = InviteClientTransaction::new(request, dest, TransportProtocol::Udp);
        table.insert_client(ClientTransaction::Invite(tx));

        assert_eq!(table.client_count(), 1);

        // 手动标记为终止（通过清理方法）
        table.cleanup_terminated();
        // 事务未终止，不应被清理
        assert_eq!(table.client_count(), 1);
    }
}
