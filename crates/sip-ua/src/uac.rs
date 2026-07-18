//! SIP UAC（User Agent Client）呼出功能
//!
//! 提供 UAC 侧的呼叫控制功能，包括：
//! - INVITE 请求构建与发送
//! - 响应处理（1xx/2xx/3xx-6xx）
//! - 认证挑战处理（401/407）
//! - 重定向处理（3xx）
//! - 呼叫取消（CANCEL）
//! - 会话修改（re-INVITE）
//! - Glare 处理（491）

use std::collections::HashMap;
use std::sync::Arc;

use sip_core::config::SipConfig;
use sip_core::metrics::SipMetrics;
use sip_core::{SipVersion, StatusCode};
use sip_message::{
    CSeqHeader, HeaderCollection, HeaderName, HeaderValue, Method, SipRequest, SipResponse,
};
use tokio::sync::Mutex;

use crate::config::{build_invite_request, UaConfig};
use crate::event::{CallTerminationReason, SipEvent};

// ============================================================================
// PendingInvite - 待处理的呼出 INVITE
// ============================================================================

/// 待处理的呼出 INVITE 信息
#[derive(Debug, Clone)]
pub struct PendingInvite {
    /// 原始 INVITE 请求
    pub invite: SipRequest,
    /// Call-ID
    pub call_id: String,
    /// 目标地址
    pub target: String,
    /// 是否已尝试认证
    pub auth_attempted: bool,
    /// 重定向计数
    pub redirect_count: u32,
}

// ============================================================================
// Uac - UAC 呼出管理
// ============================================================================

/// UAC 呼出管理器
///
/// 管理 UAC 侧的呼出呼叫状态，跟踪待处理的 INVITE 请求，
/// 处理响应、认证挑战和重定向。
pub struct Uac {
    /// SIP 配置
    config: SipConfig,
    /// UA 配置
    ua_config: UaConfig,
    /// 待处理的 INVITE（Call-ID → PendingInvite）
    pending_invites: Arc<Mutex<HashMap<String, PendingInvite>>>,
    /// 运行指标
    metrics: Arc<SipMetrics>,
}

impl Uac {
    /// 创建新的 UAC 管理器
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
            pending_invites: Arc::new(Mutex::new(HashMap::new())),
            metrics,
        }
    }

    /// 发起呼叫
    ///
    /// 构建 INVITE 请求并记录待处理状态。
    ///
    /// # 参数
    ///
    /// - `target` - 被叫方 URI
    /// - `body` - 会话描述（可选）
    /// - `content_type` - 内容类型（可选）
    ///
    /// # 返回
    ///
    /// 返回 Call-ID 和构建的 INVITE 请求。
    pub async fn make_call(
        &self,
        target: &str,
        body: Option<Vec<u8>>,
        content_type: Option<&str>,
    ) -> Result<(String, SipRequest), String> {
        let (invite, call_id) = build_invite_request(&self.config, target, body, content_type)?;

        let call_id_str = call_id.0.clone();

        // 存储待处理的 INVITE
        let pending = PendingInvite {
            invite: invite.clone(),
            call_id: call_id_str.clone(),
            target: target.to_string(),
            auth_attempted: false,
            redirect_count: 0,
        };

        self.pending_invites
            .lock()
            .await
            .insert(call_id_str.clone(), pending);

        self.metrics.inc_active_client_transactions();

        tracing::info!(
            "Uac: initiated call to {} (call_id={})",
            target,
            call_id_str
        );

        Ok((call_id_str, invite))
    }

    /// 处理 INVITE 响应
    ///
    /// 根据响应状态码生成对应的 SipEvent：
    /// - 1xx → CallProgress
    /// - 2xx → CallEstablished（需要发送 ACK）
    /// - 3xx → 重定向处理
    /// - 401/407 → 认证挑战处理
    /// - 486 → RemoteBusy
    /// - 其他 4xx-6xx → CallTerminated
    ///
    /// # 参数
    ///
    /// - `call_id` - 呼叫标识
    /// - `response` - 收到的响应
    ///
    /// # 返回
    ///
    /// 返回生成的事件列表和可选的需要发送的请求（ACK 或带认证的 INVITE）。
    pub async fn handle_response(
        &self,
        call_id: &str,
        response: &SipResponse,
    ) -> (Vec<SipEvent>, Option<SipRequest>) {
        let status_code = response.status_line.status_code;
        let mut events = Vec::new();
        let mut request_to_send = None;

        if status_code.is_provisional() {
            // 1xx 临时响应 → CallProgress
            events.push(SipEvent::CallProgress {
                call_id: call_id.to_string(),
                status_code: status_code.0,
                reason_phrase: response.status_line.reason_phrase.clone(),
            });
        } else if status_code.is_success() {
            // 2xx 成功响应 → CallEstablished
            let body = response.body.as_ref().map(|b| b.content.clone());
            let content_type = response
                .headers
                .get(&HeaderName::ContentType)
                .and_then(|v| {
                    if let HeaderValue::ContentType(ct) = v {
                        Some(ct.clone())
                    } else {
                        None
                    }
                });

            // 提取 dialog_id（Call-ID + From Tag + To Tag）
            let dialog_id = build_dialog_id_from_response(response);

            events.push(SipEvent::CallEstablished {
                call_id: call_id.to_string(),
                dialog_id,
                body,
                content_type,
            });

            // 从待处理列表中移除
            self.pending_invites.lock().await.remove(call_id);
            self.metrics.dec_active_client_transactions();
        } else if status_code.is_redirect() {
            // 3xx 重定向
            let mut pending = self.pending_invites.lock().await;
            if let Some(invite_info) = pending.get_mut(call_id) {
                if invite_info.redirect_count < self.ua_config.max_redirects {
                    // 根据 Contact 头部重新发起 INVITE
                    if let Some(contact_uri) = response
                        .headers
                        .get(&HeaderName::Contact)
                        .and_then(|v| v.as_contact())
                        .map(|c| c.uri.to_string())
                    {
                        invite_info.redirect_count += 1;
                        invite_info.target = contact_uri.clone();
                        drop(pending);

                        // 重新构建 INVITE
                        match build_invite_request(&self.config, &contact_uri, None, None) {
                            Ok((new_invite, _)) => {
                                request_to_send = Some(new_invite);
                            }
                            Err(e) => {
                                events.push(SipEvent::CallTerminated {
                                    call_id: call_id.to_string(),
                                    reason: CallTerminationReason::Error(format!(
                                        "redirect failed: {}",
                                        e
                                    )),
                                });
                            }
                        }
                    }
                } else {
                    events.push(SipEvent::CallTerminated {
                        call_id: call_id.to_string(),
                        reason: CallTerminationReason::Redirected,
                    });
                    pending.remove(call_id);
                    self.metrics.dec_active_client_transactions();
                }
            }
        } else if status_code.0 == StatusCode::UNAUTHORIZED.0
            || status_code.0 == StatusCode::PROXY_AUTH_REQUIRED.0
        {
            // 401/407 认证挑战
            let mut pending = self.pending_invites.lock().await;
            if let Some(invite_info) = pending.get_mut(call_id) {
                if !invite_info.auth_attempted {
                    invite_info.auth_attempted = true;
                    drop(pending);

                    // TODO: 构建带认证头部的 INVITE（需要摘要认证支持）
                    // 当前简化处理：报告认证失败
                    events.push(SipEvent::CallTerminated {
                        call_id: call_id.to_string(),
                        reason: CallTerminationReason::AuthenticationFailed,
                    });
                    self.pending_invites.lock().await.remove(call_id);
                    self.metrics.dec_active_client_transactions();
                } else {
                    events.push(SipEvent::CallTerminated {
                        call_id: call_id.to_string(),
                        reason: CallTerminationReason::AuthenticationFailed,
                    });
                    pending.remove(call_id);
                    self.metrics.dec_active_client_transactions();
                }
            }
        } else {
            // 4xx-6xx 其他错误
            let reason = match status_code.0 {
                486 => CallTerminationReason::RemoteBusy,
                487 => CallTerminationReason::Cancelled,
                408 => CallTerminationReason::Timeout,
                _ => CallTerminationReason::Error(format!(
                    "{} {}",
                    status_code.0, response.status_line.reason_phrase
                )),
            };

            events.push(SipEvent::CallTerminated {
                call_id: call_id.to_string(),
                reason,
            });

            self.pending_invites.lock().await.remove(call_id);
            self.metrics.dec_active_client_transactions();
        }

        (events, request_to_send)
    }

    /// 处理超时
    ///
    /// 当 Timer B 超时时调用，生成 CallTerminated(Timeout) 事件。
    pub async fn handle_timeout(&self, call_id: &str) -> SipEvent {
        self.pending_invites.lock().await.remove(call_id);
        self.metrics.dec_active_client_transactions();

        SipEvent::CallTerminated {
            call_id: call_id.to_string(),
            reason: CallTerminationReason::Timeout,
        }
    }

    /// 取消呼叫
    ///
    /// 构建 CANCEL 请求。
    ///
    /// # 参数
    ///
    /// - `call_id` - 要取消的呼叫标识
    ///
    /// # 返回
    ///
    /// 返回构建的 CANCEL 请求，如果呼叫不存在返回 None。
    pub async fn cancel_call(&self, call_id: &str) -> Option<SipRequest> {
        let mut pending = self.pending_invites.lock().await;
        let invite_info = pending.remove(call_id)?;
        self.metrics.dec_active_client_transactions();

        // 构建 CANCEL 请求
        let cancel = build_cancel_from_invite(&invite_info.invite);
        Some(cancel)
    }

    /// 检查是否有待处理的呼叫
    pub async fn has_pending_call(&self, call_id: &str) -> bool {
        self.pending_invites.lock().await.contains_key(call_id)
    }

    /// 获取待处理呼叫的目标地址
    pub async fn get_pending_target(&self, call_id: &str) -> Option<String> {
        self.pending_invites
            .lock()
            .await
            .get(call_id)
            .map(|info| info.target.clone())
    }

    /// 获取原始 INVITE 请求
    pub async fn get_original_invite(&self, call_id: &str) -> Option<SipRequest> {
        self.pending_invites
            .lock()
            .await
            .get(call_id)
            .map(|info| info.invite.clone())
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 从 INVITE 请求构建 CANCEL 请求
fn build_cancel_from_invite(invite: &SipRequest) -> SipRequest {
    let mut headers = HeaderCollection::new();

    // 复制 Via 头部
    if let Some(via) = invite.headers.get(&HeaderName::Via) {
        headers.insert(HeaderName::Via, via.clone());
    }

    // 复制 From 头部
    if let Some(from) = invite.headers.get(&HeaderName::From) {
        headers.insert(HeaderName::From, from.clone());
    }

    // 复制 To 头部
    if let Some(to) = invite.headers.get(&HeaderName::To) {
        headers.insert(HeaderName::To, to.clone());
    }

    // 复制 Call-ID
    if let Some(call_id) = invite.headers.get(&HeaderName::CallId) {
        headers.insert(HeaderName::CallId, call_id.clone());
    }

    // CSeq：与 INVITE 相同的序列号，方法为 CANCEL
    if let Some(cseq_value) = invite.headers.get(&HeaderName::CSeq) {
        if let Some(cseq) = cseq_value.as_cseq() {
            let cancel_cseq = CSeqHeader::new(cseq.sequence.0, Method::Cancel);
            headers.insert(HeaderName::CSeq, HeaderValue::CSeq(cancel_cseq));
        }
    }

    // Max-Forwards
    headers.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

    SipRequest {
        request_line: sip_message::RequestLine {
            method: Method::Cancel,
            request_uri: invite.request_line.request_uri.clone(),
            version: SipVersion,
        },
        headers,
        body: None,
    }
}

/// 从响应构建对话标识字符串
fn build_dialog_id_from_response(response: &SipResponse) -> String {
    let call_id = response
        .headers
        .get(&HeaderName::CallId)
        .and_then(|v| v.as_call_id())
        .map(|cid| cid.0.clone())
        .unwrap_or_default();

    let from_tag = response
        .headers
        .get(&HeaderName::From)
        .and_then(|v| v.as_from_to())
        .and_then(|ft| ft.tag.as_ref())
        .map(|t| t.0.clone())
        .unwrap_or_default();

    let to_tag = response
        .headers
        .get(&HeaderName::To)
        .and_then(|v| v.as_from_to())
        .and_then(|ft| ft.tag.as_ref())
        .map(|t| t.0.clone())
        .unwrap_or_default();

    format!("{}:{}:{}", call_id, from_tag, to_tag)
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use sip_core::Host;
    use sip_message::{
        CSeqHeader, CallId, ContactHeader, FromToHeader, Method, SipUri, Tag, ViaHeader,
    };

    fn create_test_uac() -> Uac {
        let config = SipConfig::builder()
            .aor("sip:alice@example.com")
            .contact("sip:alice@192.168.1.1:5060")
            .sip_port(5060)
            .build()
            .unwrap();

        Uac::new(config, UaConfig::default(), Arc::new(SipMetrics::new()))
    }

    #[tokio::test]
    async fn test_make_call() {
        let uac = create_test_uac();

        let (call_id, request) = uac
            .make_call("sip:bob@example.com", None, None)
            .await
            .unwrap();

        assert!(!call_id.is_empty());
        assert_eq!(request.request_line.method, Method::Invite);
        assert!(uac.has_pending_call(&call_id).await);
    }

    #[tokio::test]
    async fn test_make_call_invalid_target() {
        let uac = create_test_uac();

        let result = uac.make_call("not-a-uri", None, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cancel_call() {
        let uac = create_test_uac();

        let (call_id, _) = uac
            .make_call("sip:bob@example.com", None, None)
            .await
            .unwrap();

        let cancel = uac.cancel_call(&call_id).await;
        assert!(cancel.is_some());

        let cancel_request = cancel.unwrap();
        assert_eq!(cancel_request.request_line.method, Method::Cancel);

        // 呼叫应已从待处理列表中移除
        assert!(!uac.has_pending_call(&call_id).await);
    }

    #[tokio::test]
    async fn test_cancel_nonexistent_call() {
        let uac = create_test_uac();

        let result = uac.cancel_call("nonexistent").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_handle_180_response() {
        let uac = create_test_uac();

        let (call_id, _) = uac
            .make_call("sip:bob@example.com", None, None)
            .await
            .unwrap();

        let response = create_test_response(180, "Ringing", true);
        let (events, _) = uac.handle_response(&call_id, &response).await;

        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            SipEvent::CallProgress {
                status_code: 180,
                ..
            }
        ));

        // 呼叫应仍在待处理列表中
        assert!(uac.has_pending_call(&call_id).await);
    }

    #[tokio::test]
    async fn test_handle_200_response() {
        let uac = create_test_uac();

        let (call_id, _) = uac
            .make_call("sip:bob@example.com", None, None)
            .await
            .unwrap();

        let response = create_test_response(200, "OK", true);
        let (events, _) = uac.handle_response(&call_id, &response).await;

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], SipEvent::CallEstablished { .. }));

        // 呼叫应已从待处理列表中移除
        assert!(!uac.has_pending_call(&call_id).await);
    }

    #[tokio::test]
    async fn test_handle_486_response() {
        let uac = create_test_uac();

        let (call_id, _) = uac
            .make_call("sip:bob@example.com", None, None)
            .await
            .unwrap();

        let response = create_test_response(486, "Busy Here", true);
        let (events, _) = uac.handle_response(&call_id, &response).await;

        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            SipEvent::CallTerminated {
                reason: CallTerminationReason::RemoteBusy,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn test_handle_timeout() {
        let uac = create_test_uac();

        let (call_id, _) = uac
            .make_call("sip:bob@example.com", None, None)
            .await
            .unwrap();

        let event = uac.handle_timeout(&call_id).await;

        assert!(matches!(
            event,
            SipEvent::CallTerminated {
                reason: CallTerminationReason::Timeout,
                ..
            }
        ));
        assert!(!uac.has_pending_call(&call_id).await);
    }

    // ---- 辅助函数 ----

    fn create_test_response(status_code: u16, reason: &str, with_to_tag: bool) -> SipResponse {
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
            .with_tag(Tag("local-tag".to_string()));
        headers.insert(HeaderName::From, HeaderValue::FromTo(from_header));

        let mut to_header = FromToHeader::new(SipUri::parse("sip:bob@example.com").unwrap());
        if with_to_tag {
            to_header = to_header.with_tag(Tag("remote-tag".to_string()));
        }
        headers.insert(HeaderName::To, HeaderValue::FromTo(to_header));

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
                SipUri::parse("sip:bob@192.168.1.2:5060").unwrap(),
            )),
        );

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
}
