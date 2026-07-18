//! SIP UAS（User Agent Server）呼入功能
//!
//! 提供 UAS 侧的呼叫控制功能，包括：
//! - 来电通知
//! - 接听（200 OK）
//! - 振铃（180 Ringing）
//! - 拒绝（486 Busy Here 等）
//! - CANCEL 处理

use std::collections::HashMap;
use std::sync::Arc;

use sip_core::config::SipConfig;
use sip_core::metrics::SipMetrics;
use sip_core::{SipVersion, StatusCode};
use sip_message::{
    Body, ContactHeader, HeaderCollection, HeaderName, HeaderValue, SipRequest, SipResponse,
    SipUri, Tag,
};
use tokio::sync::Mutex;

use crate::config::UaConfig;
use crate::event::{CallTerminationReason, SipEvent};

// ============================================================================
// IncomingCall - 来电信息
// ============================================================================

/// 来电信息
#[derive(Debug, Clone)]
pub struct IncomingCall {
    /// 原始 INVITE 请求
    pub invite: SipRequest,
    /// Call-ID
    pub call_id: String,
    /// 主叫方地址
    pub from: String,
    /// 被叫方地址
    pub to: String,
    /// 会话描述
    pub body: Option<Vec<u8>>,
    /// 内容类型
    pub content_type: Option<String>,
    /// 本端生成的 Tag
    pub local_tag: Tag,
    /// 是否已取消
    pub cancelled: bool,
}

// ============================================================================
// Uas - UAS 呼入管理
// ============================================================================

/// UAS 呼入管理器
///
/// 管理 UAS 侧的呼入呼叫状态，跟踪待应答的来电，
/// 提供应答、振铃和拒绝功能。
pub struct Uas {
    /// SIP 配置
    config: SipConfig,
    /// UA 配置
    ua_config: UaConfig,
    /// 来电列表（Call-ID → IncomingCall）
    incoming_calls: Arc<Mutex<HashMap<String, IncomingCall>>>,
    /// 运行指标
    metrics: Arc<SipMetrics>,
}

impl Uas {
    /// 创建新的 UAS 管理器
    ///
    /// # 参数
    ///
    /// - `config` - SIP 配置
    /// - `ua_config` - UA 配置
    /// - `metrics` - 运行指标
    pub fn new(config: SipConfig, ua_config: UaConfig, metrics: Arc<SipMetrics>) -> Self {
        Self {
            config,
            ua_config,
            incoming_calls: Arc::new(Mutex::new(HashMap::new())),
            metrics,
        }
    }

    /// 处理收到的 INVITE 请求
    ///
    /// 解析 INVITE 请求，记录来电信息，生成 IncomingCall 事件。
    ///
    /// # 参数
    ///
    /// - `invite` - 收到的 INVITE 请求
    ///
    /// # 返回
    ///
    /// 返回生成的 SipEvent。
    pub async fn handle_invite(&self, invite: &SipRequest) -> SipEvent {
        let call_id = invite
            .headers
            .get(&HeaderName::CallId)
            .and_then(|v| v.as_call_id())
            .map(|cid| cid.0.clone())
            .unwrap_or_default();

        let from = invite
            .headers
            .get(&HeaderName::From)
            .and_then(|v| v.as_from_to())
            .map(|ft| ft.uri.to_string())
            .unwrap_or_default();

        let to = invite
            .headers
            .get(&HeaderName::To)
            .and_then(|v| v.as_from_to())
            .map(|ft| ft.uri.to_string())
            .unwrap_or_default();

        let content_type = invite.headers.get(&HeaderName::ContentType).and_then(|v| {
            if let HeaderValue::ContentType(ct) = v {
                Some(ct.clone())
            } else {
                None
            }
        });

        let local_tag = Tag::new();

        // 提取消息体内容
        let body_content = invite.body.as_ref().map(|b| b.content.clone());

        // 存储来电信息
        let incoming = IncomingCall {
            invite: invite.clone(),
            call_id: call_id.clone(),
            from: from.clone(),
            to: to.clone(),
            body: body_content.clone(),
            content_type: content_type.clone(),
            local_tag: local_tag.clone(),
            cancelled: false,
        };

        self.incoming_calls
            .lock()
            .await
            .insert(call_id.clone(), incoming);

        self.metrics.inc_active_server_transactions();

        tracing::info!("Uas: incoming call from {} (call_id={})", from, call_id);

        SipEvent::IncomingCall {
            call_id,
            from,
            to,
            body: body_content,
            content_type,
        }
    }

    /// 接听来电
    ///
    /// 发送 200 OK 响应。
    ///
    /// # 参数
    ///
    /// - `call_id` - 呼叫标识
    /// - `body` - 会话描述（可选）
    /// - `content_type` - 内容类型（可选）
    ///
    /// # 返回
    ///
    /// 返回 200 OK 响应，如果呼叫不存在返回 None。
    pub async fn answer_call(
        &self,
        call_id: &str,
        body: Option<Vec<u8>>,
        content_type: Option<&str>,
    ) -> Option<SipResponse> {
        let mut incoming_calls = self.incoming_calls.lock().await;
        let incoming = incoming_calls.remove(call_id)?;
        self.metrics.dec_active_server_transactions();

        if incoming.cancelled {
            tracing::warn!("Uas: cannot answer cancelled call {}", call_id);
            return None;
        }

        let response =
            self.build_ok_response(&incoming.invite, &incoming.local_tag, body, content_type);

        tracing::info!("Uas: answered call {}", call_id);

        Some(response)
    }

    /// 发送振铃响应
    ///
    /// 发送 180 Ringing 响应。
    ///
    /// # 参数
    ///
    /// - `call_id` - 呼叫标识
    ///
    /// # 返回
    ///
    /// 返回 180 Ringing 响应，如果呼叫不存在返回 None。
    pub async fn ring_call(&self, call_id: &str) -> Option<SipResponse> {
        let incoming_calls = self.incoming_calls.lock().await;
        let incoming = incoming_calls.get(call_id)?;

        if incoming.cancelled {
            return None;
        }

        let response = self.build_ringing_response(&incoming.invite, &incoming.local_tag);

        tracing::debug!("Uas: ringing call {}", call_id);

        Some(response)
    }

    /// 拒绝来电
    ///
    /// 发送拒绝响应（默认 486 Busy Here）。
    ///
    /// # 参数
    ///
    /// - `call_id` - 呼叫标识
    /// - `status_code` - 拒绝状态码（可选，默认 486）
    /// - `reason` - 原因短语（可选）
    ///
    /// # 返回
    ///
    /// 返回拒绝响应，如果呼叫不存在返回 None。
    pub async fn reject_call(
        &self,
        call_id: &str,
        status_code: Option<u16>,
        reason: Option<&str>,
    ) -> Option<SipResponse> {
        let mut incoming_calls = self.incoming_calls.lock().await;
        let incoming = incoming_calls.remove(call_id)?;
        self.metrics.dec_active_server_transactions();

        let code = status_code.unwrap_or(self.ua_config.default_reject_code);
        let reason_str = reason.unwrap_or(match code {
            486 => "Busy Here",
            480 => "Temporarily Unavailable",
            603 => "Decline",
            _ => "Rejected",
        });

        let response = self.build_reject_response(&incoming.invite, code, reason_str);

        tracing::info!(
            "Uas: rejected call {} with {} {}",
            call_id,
            code,
            reason_str
        );

        Some(response)
    }

    /// 处理 CANCEL 请求
    ///
    /// 收到 CANCEL 后标记来电为已取消，生成 CallTerminated 事件。
    ///
    /// # 参数
    ///
    /// - `call_id` - 呼叫标识
    ///
    /// # 返回
    ///
    /// 返回生成的 SipEvent 和 200 OK (CANCEL) 响应。
    pub async fn handle_cancel(&self, call_id: &str) -> Option<(SipEvent, SipResponse)> {
        let mut incoming_calls = self.incoming_calls.lock().await;
        let incoming = incoming_calls.get_mut(call_id)?;

        incoming.cancelled = true;

        // 构建 200 OK (CANCEL) 响应
        let cancel_ok = self.build_cancel_ok_response(&incoming.invite);

        let event = SipEvent::CallTerminated {
            call_id: call_id.to_string(),
            reason: CallTerminationReason::Cancelled,
        };

        // 从来电列表中移除
        incoming_calls.remove(call_id);
        self.metrics.dec_active_server_transactions();

        tracing::info!("Uas: call {} cancelled", call_id);

        Some((event, cancel_ok))
    }

    /// 检查是否有来电
    pub async fn has_incoming_call(&self, call_id: &str) -> bool {
        self.incoming_calls.lock().await.contains_key(call_id)
    }

    /// 获取来电信息
    pub async fn get_incoming_call(&self, call_id: &str) -> Option<IncomingCall> {
        self.incoming_calls.lock().await.get(call_id).cloned()
    }

    // ========================================================================
    // 响应构建
    // ========================================================================

    /// 构建 200 OK 响应
    fn build_ok_response(
        &self,
        invite: &SipRequest,
        local_tag: &Tag,
        body: Option<Vec<u8>>,
        content_type: Option<&str>,
    ) -> SipResponse {
        let mut headers = self.copy_base_headers(invite, Some(local_tag));

        // Contact 头部
        if let Ok(contact_uri) = SipUri::parse(&self.config.contact) {
            headers.insert(
                HeaderName::Contact,
                HeaderValue::Contact(ContactHeader::new(contact_uri)),
            );
        }

        // 构建消息体（如果有）
        let sip_body = body.map(|content| {
            let ct = content_type.unwrap_or("application/sdp");
            Body::new(ct, content)
        });

        // Content-Type 和 Content-Length
        if let Some(ref body_data) = sip_body {
            headers.insert(
                HeaderName::ContentType,
                HeaderValue::ContentType(body_data.content_type.clone()),
            );
            headers.insert(
                HeaderName::ContentLength,
                HeaderValue::ContentLength(body_data.len()),
            );
        }

        SipResponse {
            status_line: sip_message::StatusLine {
                version: SipVersion,
                status_code: StatusCode::OK,
                reason_phrase: "OK".to_string(),
            },
            headers,
            body: sip_body,
        }
    }

    /// 构建 180 Ringing 响应
    fn build_ringing_response(&self, invite: &SipRequest, local_tag: &Tag) -> SipResponse {
        let headers = self.copy_base_headers(invite, Some(local_tag));

        SipResponse {
            status_line: sip_message::StatusLine {
                version: SipVersion,
                status_code: StatusCode::RINGING,
                reason_phrase: "Ringing".to_string(),
            },
            headers,
            body: None,
        }
    }

    /// 构建拒绝响应
    fn build_reject_response(
        &self,
        invite: &SipRequest,
        status_code: u16,
        reason: &str,
    ) -> SipResponse {
        let headers = self.copy_base_headers(invite, None);

        SipResponse {
            status_line: sip_message::StatusLine {
                version: SipVersion,
                status_code: StatusCode(status_code),
                reason_phrase: reason.to_string(),
            },
            headers,
            body: None,
        }
    }

    /// 构建 200 OK (CANCEL) 响应
    fn build_cancel_ok_response(&self, invite: &SipRequest) -> SipResponse {
        let headers = self.copy_base_headers(invite, None);

        SipResponse {
            status_line: sip_message::StatusLine {
                version: SipVersion,
                status_code: StatusCode::OK,
                reason_phrase: "OK".to_string(),
            },
            headers,
            body: None,
        }
    }

    /// 复制基础头部（Via、From、To、Call-ID、CSeq）
    fn copy_base_headers(&self, invite: &SipRequest, local_tag: Option<&Tag>) -> HeaderCollection {
        let mut headers = HeaderCollection::new();

        // Via
        if let Some(via) = invite.headers.get(&HeaderName::Via) {
            headers.insert(HeaderName::Via, via.clone());
        }

        // From
        if let Some(from) = invite.headers.get(&HeaderName::From) {
            headers.insert(HeaderName::From, from.clone());
        }

        // To（可能需要添加 Tag）
        if let Some(to_value) = invite.headers.get(&HeaderName::To) {
            if let Some(from_to) = to_value.as_from_to() {
                let mut from_to = from_to.clone();
                if let Some(tag) = local_tag {
                    from_to = from_to.with_tag(tag.clone());
                }
                headers.insert(HeaderName::To, HeaderValue::FromTo(from_to));
            } else {
                headers.insert(HeaderName::To, to_value.clone());
            }
        }

        // Call-ID
        if let Some(call_id) = invite.headers.get(&HeaderName::CallId) {
            headers.insert(HeaderName::CallId, call_id.clone());
        }

        // CSeq
        if let Some(cseq) = invite.headers.get(&HeaderName::CSeq) {
            headers.insert(HeaderName::CSeq, cseq.clone());
        }

        headers
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use sip_core::Host;
    use sip_message::{CSeqHeader, CallId, FromToHeader, Method, SipUri, ViaHeader};

    fn create_test_uas() -> Uas {
        let config = SipConfig::builder()
            .aor("sip:bob@example.com")
            .contact("sip:bob@192.168.1.2:5060")
            .sip_port(5060)
            .build()
            .unwrap();

        Uas::new(config, UaConfig::default(), Arc::new(SipMetrics::new()))
    }

    fn create_test_invite() -> SipRequest {
        let mut headers = HeaderCollection::new();

        headers.insert(
            HeaderName::Via,
            HeaderValue::Via(ViaHeader::new(
                sip_core::TransportProtocol::Udp,
                Host::Domain("192.168.1.1".to_string()),
                Some(5060),
            )),
        );

        let from_header = FromToHeader::new(SipUri::parse("sip:alice@example.com").unwrap())
            .with_tag(Tag("alice-tag".to_string()));
        headers.insert(HeaderName::From, HeaderValue::FromTo(from_header));

        headers.insert(
            HeaderName::To,
            HeaderValue::FromTo(FromToHeader::new(
                SipUri::parse("sip:bob@example.com").unwrap(),
            )),
        );

        headers.insert(
            HeaderName::CallId,
            HeaderValue::CallId(CallId("test-call-id@example.com".to_string())),
        );

        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(CSeqHeader::new(1, Method::Invite)),
        );

        headers.insert(
            HeaderName::Contact,
            HeaderValue::Contact(ContactHeader::new(
                SipUri::parse("sip:alice@192.168.1.1:5060").unwrap(),
            )),
        );

        headers.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

        SipRequest {
            request_line: sip_message::RequestLine {
                method: Method::Invite,
                request_uri: SipUri::parse("sip:bob@example.com").unwrap(),
                version: SipVersion,
            },
            headers,
            body: None,
        }
    }

    #[tokio::test]
    async fn test_handle_invite() {
        let uas = create_test_uas();
        let invite = create_test_invite();

        let event = uas.handle_invite(&invite).await;

        if let SipEvent::IncomingCall {
            call_id, from, to, ..
        } = event
        {
            assert_eq!(call_id, "test-call-id@example.com");
            assert_eq!(from, "sip:alice@example.com");
            assert_eq!(to, "sip:bob@example.com");
            assert!(uas.has_incoming_call(&call_id).await);
        } else {
            panic!("Expected IncomingCall event");
        }
    }

    #[tokio::test]
    async fn test_answer_call() {
        let uas = create_test_uas();
        let invite = create_test_invite();

        let event = uas.handle_invite(&invite).await;
        let call_id = if let SipEvent::IncomingCall { call_id, .. } = &event {
            call_id.clone()
        } else {
            panic!("Expected IncomingCall");
        };

        let response = uas.answer_call(&call_id, None, None).await;
        assert!(response.is_some());

        let response = response.unwrap();
        assert_eq!(response.status_line.status_code.0, 200);
        assert_eq!(response.status_line.reason_phrase, "OK");

        // 呼叫应已从来电列表中移除
        assert!(!uas.has_incoming_call(&call_id).await);
    }

    #[tokio::test]
    async fn test_ring_call() {
        let uas = create_test_uas();
        let invite = create_test_invite();

        let event = uas.handle_invite(&invite).await;
        let call_id = if let SipEvent::IncomingCall { call_id, .. } = &event {
            call_id.clone()
        } else {
            panic!("Expected IncomingCall");
        };

        let response = uas.ring_call(&call_id).await;
        assert!(response.is_some());

        let response = response.unwrap();
        assert_eq!(response.status_line.status_code.0, 180);
        assert_eq!(response.status_line.reason_phrase, "Ringing");

        // 呼叫应仍在来电列表中
        assert!(uas.has_incoming_call(&call_id).await);
    }

    #[tokio::test]
    async fn test_reject_call() {
        let uas = create_test_uas();
        let invite = create_test_invite();

        let event = uas.handle_invite(&invite).await;
        let call_id = if let SipEvent::IncomingCall { call_id, .. } = &event {
            call_id.clone()
        } else {
            panic!("Expected IncomingCall");
        };

        let response = uas.reject_call(&call_id, None, None).await;
        assert!(response.is_some());

        let response = response.unwrap();
        assert_eq!(response.status_line.status_code.0, 486);
        assert_eq!(response.status_line.reason_phrase, "Busy Here");

        // 呼叫应已从来电列表中移除
        assert!(!uas.has_incoming_call(&call_id).await);
    }

    #[tokio::test]
    async fn test_reject_call_custom_code() {
        let uas = create_test_uas();
        let invite = create_test_invite();

        let event = uas.handle_invite(&invite).await;
        let call_id = if let SipEvent::IncomingCall { call_id, .. } = &event {
            call_id.clone()
        } else {
            panic!("Expected IncomingCall");
        };

        let response = uas.reject_call(&call_id, Some(603), Some("Decline")).await;
        assert!(response.is_some());

        let response = response.unwrap();
        assert_eq!(response.status_line.status_code.0, 603);
        assert_eq!(response.status_line.reason_phrase, "Decline");
    }

    #[tokio::test]
    async fn test_handle_cancel() {
        let uas = create_test_uas();
        let invite = create_test_invite();

        let event = uas.handle_invite(&invite).await;
        let call_id = if let SipEvent::IncomingCall { call_id, .. } = &event {
            call_id.clone()
        } else {
            panic!("Expected IncomingCall");
        };

        let result = uas.handle_cancel(&call_id).await;
        assert!(result.is_some());

        let (event, response) = result.unwrap();
        assert!(matches!(
            event,
            SipEvent::CallTerminated {
                reason: CallTerminationReason::Cancelled,
                ..
            }
        ));
        assert_eq!(response.status_line.status_code.0, 200);

        // 呼叫应已从来电列表中移除
        assert!(!uas.has_incoming_call(&call_id).await);
    }

    #[tokio::test]
    async fn test_answer_nonexistent_call() {
        let uas = create_test_uas();

        let result = uas.answer_call("nonexistent", None, None).await;
        assert!(result.is_none());
    }
}
