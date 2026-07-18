//! SIP UA 配置辅助
//!
//! 提供 UA 层的配置类型和辅助函数，包括呼叫配置、
//! 默认 SIP 消息头部构建等。

use sip_core::config::SipConfig;
use sip_core::{Host, SipVersion};
use sip_message::{
    Body, CSeqHeader, CallId, ContactHeader, FromToHeader, HeaderCollection, HeaderName,
    HeaderValue, Method, RequestLine, SipUri, Tag, ViaHeader,
};

// ============================================================================
// UaConfig - UA 配置
// ============================================================================

/// UA 层配置
///
/// 包含 SIP UA 的运行时配置参数，与 SipConfig 互补，
/// 专注于呼叫控制行为而非协议栈配置。
#[derive(Debug, Clone)]
pub struct UaConfig {
    /// 呼叫超时（秒），默认 32 秒（64*T1，T1=500ms）
    pub call_timeout_secs: u64,
    /// 最大重定向次数，默认 5
    pub max_redirects: u32,
    /// 最大认证重试次数，默认 3
    pub max_auth_retries: u32,
    /// 是否自动发送 180 Ringing，默认 true
    pub auto_ringing: bool,
    /// 默认拒绝状态码，默认 486
    pub default_reject_code: u16,
    /// Glare 重试最大延迟（毫秒），默认 2000
    pub glare_retry_max_delay_ms: u64,
}

impl Default for UaConfig {
    fn default() -> Self {
        Self {
            call_timeout_secs: 32,
            max_redirects: 5,
            max_auth_retries: 3,
            auto_ringing: true,
            default_reject_code: 486,
            glare_retry_max_delay_ms: 2000,
        }
    }
}

// ============================================================================
// INVITE 请求构建辅助
// ============================================================================

/// 构建 INVITE 请求
///
/// 按照 RFC 3261 Section 13 构建符合规范的 INVITE 请求：
/// - Request-URI 为被叫方地址
/// - From 含 Tag
/// - To 为被叫方 AOR（无 Tag）
/// - Contact 含本端联系地址
/// - 包含 Via、Call-ID、CSeq、Max-Forwards 等必要头部
///
/// # 参数
///
/// - `config` - SIP 配置
/// - `target` - 被叫方 URI
/// - `body` - 会话描述（可选）
/// - `content_type` - 内容类型（可选）
///
/// # 返回
///
/// 返回构建的 INVITE 请求和生成的 Call-ID。
pub fn build_invite_request(
    config: &SipConfig,
    target: &str,
    body: Option<Vec<u8>>,
    content_type: Option<&str>,
) -> Result<(sip_message::SipRequest, CallId), String> {
    // 解析目标 URI
    let request_uri = SipUri::parse(target).map_err(|e| format!("invalid target URI: {}", e))?;

    // 解析本端 AOR URI
    let from_uri = SipUri::parse(&config.aor).map_err(|e| format!("invalid AOR URI: {}", e))?;

    // 解析被叫方 URI（To 头部）
    let to_uri = SipUri::parse(target).map_err(|e| format!("invalid target URI: {}", e))?;

    // 解析 Contact URI
    let contact_uri =
        SipUri::parse(&config.contact).map_err(|e| format!("invalid contact URI: {}", e))?;

    // 生成 Call-ID 和 From Tag
    let call_id = CallId::new();
    let from_tag = Tag::new();

    // 构建 Via 头部
    let via_host = extract_host_from_contact(&config.contact);
    let via = ViaHeader::new(config.transport, via_host, Some(config.sip_port));

    // 构建 From 头部
    let from_header = FromToHeader::new(from_uri).with_tag(from_tag);

    // 构建 To 头部（无 Tag）
    let to_header = FromToHeader::new(to_uri);

    // 构建 Contact 头部
    let contact_header = ContactHeader::new(contact_uri);

    // 构建 CSeq 头部
    let cseq = CSeqHeader::new(1, Method::Invite);

    // 组装头部
    let mut headers = HeaderCollection::new();
    headers.insert(HeaderName::Via, HeaderValue::Via(via));
    headers.insert(HeaderName::From, HeaderValue::FromTo(from_header));
    headers.insert(HeaderName::To, HeaderValue::FromTo(to_header));
    headers.insert(HeaderName::CallId, HeaderValue::CallId(call_id.clone()));
    headers.insert(HeaderName::CSeq, HeaderValue::CSeq(cseq));
    headers.insert(HeaderName::Contact, HeaderValue::Contact(contact_header));
    headers.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

    // 构建消息体（如果有）
    let sip_body = body.map(|content| {
        let ct = content_type.unwrap_or("application/sdp");
        Body::new(ct, content)
    });

    // 添加 Content-Type 和 Content-Length（如果有消息体）
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

    // 构建请求
    let request = sip_message::SipRequest {
        request_line: RequestLine {
            method: Method::Invite,
            request_uri,
            version: SipVersion,
        },
        headers,
        body: sip_body,
    };

    Ok((request, call_id))
}

/// 构建 SIP 响应
///
/// 根据 INVITE 请求构建对应的 SIP 响应。
///
/// # 参数
///
/// - `request` - 原始 INVITE 请求
/// - `status_code` - 状态码
/// - `reason_phrase` - 原因短语
/// - `local_tag` - 本端 Tag（To 头部 Tag）
/// - `contact` - Contact URI
/// - `body` - 响应消息体
/// - `content_type` - 内容类型
pub fn build_response(
    request: &sip_message::SipRequest,
    status_code: u16,
    reason_phrase: &str,
    local_tag: Option<&Tag>,
    contact: Option<&str>,
    body: Option<Vec<u8>>,
    content_type: Option<&str>,
) -> sip_message::SipResponse {
    let status = sip_core::StatusCode(status_code);

    // 复制请求头部作为响应基础
    let mut headers = HeaderCollection::new();

    // 复制 Via 头部
    if let Some(via) = request.headers.get(&HeaderName::Via) {
        headers.insert(HeaderName::Via, via.clone());
    }

    // From 头部（与请求相同）
    if let Some(from) = request.headers.get(&HeaderName::From) {
        headers.insert(HeaderName::From, from.clone());
    }

    // To 头部（可能需要添加 Tag）
    if let Some(to_value) = request.headers.get(&HeaderName::To) {
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
    if let Some(call_id) = request.headers.get(&HeaderName::CallId) {
        headers.insert(HeaderName::CallId, call_id.clone());
    }

    // CSeq
    if let Some(cseq) = request.headers.get(&HeaderName::CSeq) {
        headers.insert(HeaderName::CSeq, cseq.clone());
    }

    // Contact 头部
    if let Some(contact_str) = contact {
        if let Ok(contact_uri) = SipUri::parse(contact_str) {
            headers.insert(
                HeaderName::Contact,
                HeaderValue::Contact(ContactHeader::new(contact_uri)),
            );
        }
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

    sip_message::SipResponse {
        status_line: sip_message::StatusLine {
            version: SipVersion,
            status_code: status,
            reason_phrase: reason_phrase.to_string(),
        },
        headers,
        body: sip_body,
    }
}

/// 从 Contact URI 字符串提取 Host 用于 Via 头部
fn extract_host_from_contact(contact: &str) -> Host {
    SipUri::parse(contact)
        .map(|uri| uri.host.clone())
        .unwrap_or_else(|_| Host::Domain("localhost".to_string()))
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use sip_core::config::SipConfig;

    fn create_test_config() -> SipConfig {
        SipConfig::builder()
            .aor("sip:alice@example.com")
            .contact("sip:alice@192.168.1.1:5060")
            .sip_port(5060)
            .build()
            .unwrap()
    }

    #[test]
    fn test_ua_config_default() {
        let config = UaConfig::default();
        assert_eq!(config.call_timeout_secs, 32);
        assert_eq!(config.max_redirects, 5);
        assert_eq!(config.max_auth_retries, 3);
        assert!(config.auto_ringing);
        assert_eq!(config.default_reject_code, 486);
        assert_eq!(config.glare_retry_max_delay_ms, 2000);
    }

    #[test]
    fn test_build_invite_request() {
        let config = create_test_config();
        let (request, call_id) =
            build_invite_request(&config, "sip:bob@example.com", None, None).unwrap();

        assert_eq!(request.request_line.method, Method::Invite);
        assert_eq!(
            request.request_line.request_uri.to_string(),
            "sip:bob@example.com"
        );

        // 验证必要头部存在
        assert!(request.headers.get(&HeaderName::Via).is_some());
        assert!(request.headers.get(&HeaderName::From).is_some());
        assert!(request.headers.get(&HeaderName::To).is_some());
        assert!(request.headers.get(&HeaderName::CallId).is_some());
        assert!(request.headers.get(&HeaderName::CSeq).is_some());
        assert!(request.headers.get(&HeaderName::Contact).is_some());
        assert!(request.headers.get(&HeaderName::MaxForwards).is_some());

        // 验证 Call-ID 非空
        assert!(!call_id.0.is_empty());

        // 验证 From 含 Tag
        let from = request.headers.get(&HeaderName::From).unwrap();
        let from_header = from.as_from_to().unwrap();
        assert!(from_header.tag.is_some());

        // 验证 To 无 Tag
        let to = request.headers.get(&HeaderName::To).unwrap();
        let to_header = to.as_from_to().unwrap();
        assert!(to_header.tag.is_none());
    }

    #[test]
    fn test_build_invite_request_with_body() {
        let config = create_test_config();
        let body = b"v=0\r\no=alice 123 456 IN IP4 192.168.1.1\r\n".to_vec();
        let (request, _) = build_invite_request(
            &config,
            "sip:bob@example.com",
            Some(body),
            Some("application/sdp"),
        )
        .unwrap();

        assert!(request.body.is_some());
        let body = request.body.unwrap();
        assert_eq!(body.content_type, "application/sdp");
        assert!(!body.content.is_empty());
        assert!(request.headers.get(&HeaderName::ContentType).is_some());
        assert!(request.headers.get(&HeaderName::ContentLength).is_some());
    }

    #[test]
    fn test_build_invite_request_invalid_target() {
        let config = create_test_config();
        let result = build_invite_request(&config, "not-a-uri", None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_response() {
        let config = create_test_config();
        let (invite, _) = build_invite_request(&config, "sip:bob@example.com", None, None).unwrap();

        let local_tag = Tag::new();
        let response = build_response(
            &invite,
            200,
            "OK",
            Some(&local_tag),
            Some("sip:alice@192.168.1.1:5060"),
            None,
            None,
        );

        assert_eq!(response.status_line.status_code.0, 200);
        assert_eq!(response.status_line.reason_phrase, "OK");

        // 验证必要头部
        assert!(response.headers.get(&HeaderName::Via).is_some());
        assert!(response.headers.get(&HeaderName::From).is_some());
        assert!(response.headers.get(&HeaderName::To).is_some());
        assert!(response.headers.get(&HeaderName::CallId).is_some());
        assert!(response.headers.get(&HeaderName::CSeq).is_some());
        assert!(response.headers.get(&HeaderName::Contact).is_some());

        // 验证 To 含 Tag
        let to = response.headers.get(&HeaderName::To).unwrap();
        let to_header = to.as_from_to().unwrap();
        assert!(to_header.tag.is_some());
    }

    #[test]
    fn test_build_response_180() {
        let config = create_test_config();
        let (invite, _) = build_invite_request(&config, "sip:bob@example.com", None, None).unwrap();

        let local_tag = Tag::new();
        let response = build_response(
            &invite,
            180,
            "Ringing",
            Some(&local_tag),
            Some("sip:alice@192.168.1.1:5060"),
            None,
            None,
        );

        assert_eq!(response.status_line.status_code.0, 180);
        assert_eq!(response.status_line.reason_phrase, "Ringing");
    }

    #[test]
    fn test_build_response_486() {
        let config = create_test_config();
        let (invite, _) = build_invite_request(&config, "sip:bob@example.com", None, None).unwrap();

        let response = build_response(&invite, 486, "Busy Here", None, None, None, None);

        assert_eq!(response.status_line.status_code.0, 486);
        assert_eq!(response.status_line.reason_phrase, "Busy Here");
    }
}
