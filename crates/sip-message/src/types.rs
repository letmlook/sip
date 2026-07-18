//! SIP 消息类型定义
//!
//! 定义 SIP 方法枚举、BranchId、Tag、CallId 等消息层核心类型，
//! 以及请求/响应消息、消息体等完整消息类型。

use std::fmt;
use std::str::FromStr;

use sip_core::{ParseError, SipVersion, StatusCode};
use uuid::Uuid;

use crate::headers::{HeaderCollection, HeaderName, HeaderValue};
use crate::uri::SipUri;

// ============================================================================
// Method - SIP 方法枚举
// ============================================================================

/// SIP 方法
///
/// 定义 RFC 3261 核心方法和常用扩展方法，支持自定义扩展方法名。
/// 扩展方法名仅允许大写字母和数字。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Method {
    /// REGISTER 方法 - 用户注册
    Register,
    /// INVITE 方法 - 发起会话
    Invite,
    /// ACK 方法 - 确认邀请
    Ack,
    /// CANCEL 方法 - 取消待处理请求
    Cancel,
    /// BYE 方法 - 终止会话
    Bye,
    /// OPTIONS 方法 - 查询服务器能力
    Options,
    /// INFO 方法 - 传递会话中信息
    Info,
    /// UPDATE 方法 - 修改会话参数
    Update,
    /// PRACK 方法 - 确认临时响应
    Prack,
    /// MESSAGE 方法 - 传递即时消息
    Message,
    /// SUBSCRIBE 方法 - 订阅事件
    Subscribe,
    /// NOTIFY 方法 - 通知事件
    Notify,
    /// REFER 方法 - 转移引用
    Refer,
    /// 扩展方法
    Extension(String),
}

impl Method {
    /// 判断是否为扩展方法
    pub fn is_extension(&self) -> bool {
        matches!(self, Self::Extension(_))
    }

    /// 验证扩展方法名是否合法（仅允许大写字母和数字）
    fn is_valid_extension_name(name: &str) -> bool {
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Register => write!(f, "REGISTER"),
            Self::Invite => write!(f, "INVITE"),
            Self::Ack => write!(f, "ACK"),
            Self::Cancel => write!(f, "CANCEL"),
            Self::Bye => write!(f, "BYE"),
            Self::Options => write!(f, "OPTIONS"),
            Self::Info => write!(f, "INFO"),
            Self::Update => write!(f, "UPDATE"),
            Self::Prack => write!(f, "PRACK"),
            Self::Message => write!(f, "MESSAGE"),
            Self::Subscribe => write!(f, "SUBSCRIBE"),
            Self::Notify => write!(f, "NOTIFY"),
            Self::Refer => write!(f, "REFER"),
            Self::Extension(name) => write!(f, "{}", name),
        }
    }
}

impl FromStr for Method {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "REGISTER" => Ok(Self::Register),
            "INVITE" => Ok(Self::Invite),
            "ACK" => Ok(Self::Ack),
            "CANCEL" => Ok(Self::Cancel),
            "BYE" => Ok(Self::Bye),
            "OPTIONS" => Ok(Self::Options),
            "INFO" => Ok(Self::Info),
            "UPDATE" => Ok(Self::Update),
            "PRACK" => Ok(Self::Prack),
            "MESSAGE" => Ok(Self::Message),
            "SUBSCRIBE" => Ok(Self::Subscribe),
            "NOTIFY" => Ok(Self::Notify),
            "REFER" => Ok(Self::Refer),
            _ => {
                // 验证扩展方法名：仅允许大写字母和数字
                if Self::is_valid_extension_name(&s.to_uppercase()) {
                    Ok(Self::Extension(s.to_uppercase()))
                } else {
                    Err(ParseError::InvalidMethod {
                        method: s.to_string(),
                    })
                }
            }
        }
    }
}

// ============================================================================
// BranchId - Via 头部分支标识
// ============================================================================

/// Via 头部分支标识
///
/// 符合 RFC 3261 规范的分支标识，必须以 `z9hG4bK` 魔术 Cookie 开头。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BranchId(pub String);

impl BranchId {
    /// 生成以 `z9hG4bK` 开头的唯一分支标识
    pub fn new() -> Self {
        Self(format!("z9hG4bK-{}", Uuid::new_v4().simple()))
    }

    /// 验证分支标识格式是否符合 RFC 3261 规范
    ///
    /// 有效的分支标识必须以 `z9hG4bK` 开头。
    pub fn is_valid(&self) -> bool {
        self.0.starts_with("z9hG4bK")
    }
}

impl Default for BranchId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for BranchId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for BranchId {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

// ============================================================================
// Tag - From/To 头部标签
// ============================================================================

/// From/To 头部标签参数
///
/// 用于对话标识，UAC 在 From 中设置本地 Tag，UAS 在 To 中设置远端 Tag。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Tag(pub String);

impl Tag {
    /// 生成随机标签
    pub fn new() -> Self {
        Self(Uuid::new_v4().simple().to_string())
    }
}

impl Default for Tag {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Tag {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

// ============================================================================
// CallId - Call-ID 头部值
// ============================================================================

/// Call-ID 头部值
///
/// 唯一标识一组消息（一个对话），由本地 ID@host 格式组成。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CallId(pub String);

impl CallId {
    /// 生成随机的 Call-ID
    pub fn new() -> Self {
        Self(format!("{}@{}", Uuid::new_v4().simple(), "sip-rs"))
    }

    /// 使用指定主机生成 Call-ID
    pub fn with_host(host: &str) -> Self {
        Self(format!("{}@{}", Uuid::new_v4().simple(), host))
    }
}

impl Default for CallId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CallId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for CallId {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

// ============================================================================
// Body - SIP 消息体
// ============================================================================

/// SIP 消息体（透传处理）
///
/// 消息体以原始字节存储，不进行内容解析（如 SDP 解析）。
/// 通过 `content_type` 字段标识消息体的 MIME 类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Body {
    /// 消息体内容（原始字节）
    pub content: Vec<u8>,
    /// Content-Type 头部值
    pub content_type: String,
}

impl Body {
    /// 创建新的消息体
    ///
    /// # 参数
    ///
    /// - `content_type` - MIME 类型，如 "application/sdp"
    /// - `content` - 消息体原始字节
    pub fn new(content_type: impl Into<String>, content: Vec<u8>) -> Self {
        Self {
            content,
            content_type: content_type.into(),
        }
    }

    /// 返回消息体字节长度
    pub fn len(&self) -> usize {
        self.content.len()
    }

    /// 判断消息体是否为空
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }
}

// ============================================================================
// RequestLine - 请求行
// ============================================================================

/// SIP 请求行
///
/// 格式：`METHOD Request-URI SIP-Version CRLF`
///
/// 例如：`INVITE sip:bob@example.com SIP/2.0\r\n`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestLine {
    /// 请求方法
    pub method: Method,
    /// 请求 URI（Request-URI）
    pub request_uri: SipUri,
    /// SIP 协议版本
    pub version: SipVersion,
}

impl fmt::Display for RequestLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.method, self.request_uri, self.version)
    }
}

// ============================================================================
// StatusLine - 状态行
// ============================================================================

/// SIP 状态行
///
/// 格式：`SIP-Version Status-Code Reason-Phrase CRLF`
///
/// 例如：`SIP/2.0 200 OK\r\n`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusLine {
    /// SIP 协议版本
    pub version: SipVersion,
    /// 状态码
    pub status_code: StatusCode,
    /// 原因短语
    pub reason_phrase: String,
}

impl fmt::Display for StatusLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {}",
            self.version, self.status_code, self.reason_phrase
        )
    }
}

// ============================================================================
// SipRequest - SIP 请求消息
// ============================================================================

/// SIP 请求消息
///
/// 由请求行、消息头集合和可选的消息体组成。
#[derive(Debug, Clone)]
pub struct SipRequest {
    /// 请求行
    pub request_line: RequestLine,
    /// 消息头集合
    pub headers: HeaderCollection,
    /// 消息体
    pub body: Option<Body>,
}

impl SipRequest {
    /// 获取消息头集合的引用
    pub fn headers(&self) -> &HeaderCollection {
        &self.headers
    }

    /// 获取消息头集合的可变引用
    pub fn headers_mut(&mut self) -> &mut HeaderCollection {
        &mut self.headers
    }

    /// 获取消息体的引用
    pub fn body(&self) -> &Option<Body> {
        &self.body
    }

    /// 获取消息体的可变引用
    pub fn body_mut(&mut self) -> &mut Option<Body> {
        &mut self.body
    }
}

// ============================================================================
// SipResponse - SIP 响应消息
// ============================================================================

/// SIP 响应消息
///
/// 由状态行、消息头集合和可选的消息体组成。
#[derive(Debug, Clone)]
pub struct SipResponse {
    /// 状态行
    pub status_line: StatusLine,
    /// 消息头集合
    pub headers: HeaderCollection,
    /// 消息体
    pub body: Option<Body>,
}

impl SipResponse {
    /// 获取消息头集合的引用
    pub fn headers(&self) -> &HeaderCollection {
        &self.headers
    }

    /// 获取消息头集合的可变引用
    pub fn headers_mut(&mut self) -> &mut HeaderCollection {
        &mut self.headers
    }

    /// 获取消息体的引用
    pub fn body(&self) -> &Option<Body> {
        &self.body
    }

    /// 获取消息体的可变引用
    pub fn body_mut(&mut self) -> &mut Option<Body> {
        &mut self.body
    }
}

// ============================================================================
// SipMessage - SIP 统一消息类型
// ============================================================================

/// SIP 消息（请求或响应）
///
/// 统一的 SIP 消息类型，可以是请求消息或响应消息。
#[derive(Debug, Clone)]
pub enum SipMessage {
    /// 请求消息
    Request(SipRequest),
    /// 响应消息
    Response(SipResponse),
}

impl SipMessage {
    /// 获取消息头集合的引用
    pub fn headers(&self) -> &HeaderCollection {
        match self {
            Self::Request(req) => &req.headers,
            Self::Response(resp) => &resp.headers,
        }
    }

    /// 获取消息头集合的可变引用
    pub fn headers_mut(&mut self) -> &mut HeaderCollection {
        match self {
            Self::Request(req) => &mut req.headers,
            Self::Response(resp) => &mut resp.headers,
        }
    }

    /// 获取消息体的引用
    pub fn body(&self) -> &Option<Body> {
        match self {
            Self::Request(req) => &req.body,
            Self::Response(resp) => &resp.body,
        }
    }

    /// 获取消息体的可变引用
    pub fn body_mut(&mut self) -> &mut Option<Body> {
        match self {
            Self::Request(req) => &mut req.body,
            Self::Response(resp) => &mut resp.body,
        }
    }

    /// 判断是否为请求消息
    pub fn is_request(&self) -> bool {
        matches!(self, Self::Request(_))
    }

    /// 判断是否为响应消息
    pub fn is_response(&self) -> bool {
        matches!(self, Self::Response(_))
    }

    /// 获取 Content-Length 头部值
    ///
    /// 返回消息头中声明的 Content-Length 值，如果不存在则返回 None。
    pub fn content_length(&self) -> Option<usize> {
        self.headers()
            .get(&HeaderName::ContentLength)
            .and_then(|v| v.as_content_length())
    }

    /// 获取 Content-Type 头部值
    ///
    /// 返回消息头中声明的 Content-Type 值，如果不存在则返回 None。
    pub fn content_type(&self) -> Option<&str> {
        self.headers().get(&HeaderName::ContentType).and_then(|v| {
            if let HeaderValue::ContentType(ct) = v {
                Some(ct.as_str())
            } else {
                None
            }
        })
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Method 测试 ----

    #[test]
    fn test_method_display() {
        assert_eq!(Method::Register.to_string(), "REGISTER");
        assert_eq!(Method::Invite.to_string(), "INVITE");
        assert_eq!(Method::Ack.to_string(), "ACK");
        assert_eq!(Method::Cancel.to_string(), "CANCEL");
        assert_eq!(Method::Bye.to_string(), "BYE");
        assert_eq!(Method::Options.to_string(), "OPTIONS");
        assert_eq!(Method::Info.to_string(), "INFO");
        assert_eq!(Method::Update.to_string(), "UPDATE");
        assert_eq!(Method::Prack.to_string(), "PRACK");
        assert_eq!(Method::Message.to_string(), "MESSAGE");
        assert_eq!(Method::Subscribe.to_string(), "SUBSCRIBE");
        assert_eq!(Method::Notify.to_string(), "NOTIFY");
        assert_eq!(Method::Refer.to_string(), "REFER");
    }

    #[test]
    fn test_method_from_str() {
        assert!(matches!("REGISTER".parse::<Method>(), Ok(Method::Register)));
        assert!(matches!("INVITE".parse::<Method>(), Ok(Method::Invite)));
        assert!(matches!("invite".parse::<Method>(), Ok(Method::Invite)));
        assert!(matches!("ACK".parse::<Method>(), Ok(Method::Ack)));
    }

    #[test]
    fn test_method_extension() {
        let ext = "CUSTOM1".parse::<Method>().unwrap();
        assert!(ext.is_extension());
        assert_eq!(ext.to_string(), "CUSTOM1");

        // 小写字母会被转为大写
        let ext2 = "Custom1".parse::<Method>().unwrap();
        assert!(ext2.is_extension());
        assert_eq!(ext2.to_string(), "CUSTOM1");

        // 无效扩展方法名（包含特殊字符）
        assert!("CUSTOM-1".parse::<Method>().is_err());
        // 空字符串
        assert!("".parse::<Method>().is_err());
    }

    // ---- BranchId 测试 ----

    #[test]
    fn test_branch_id_new() {
        let branch = BranchId::new();
        assert!(branch.is_valid());
        assert!(branch.0.starts_with("z9hG4bK-"));
    }

    #[test]
    fn test_branch_id_is_valid() {
        assert!(BranchId("z9hG4bK-abc123".to_string()).is_valid());
        assert!(BranchId("z9hG4bK".to_string()).is_valid());
        assert!(!BranchId("invalid-branch".to_string()).is_valid());
        assert!(!BranchId("Z9hG4bK-abc".to_string()).is_valid());
    }

    #[test]
    fn test_branch_id_uniqueness() {
        let b1 = BranchId::new();
        let b2 = BranchId::new();
        assert_ne!(b1, b2);
    }

    #[test]
    fn test_branch_id_default() {
        let branch = BranchId::default();
        assert!(branch.is_valid());
    }

    // ---- Tag 测试 ----

    #[test]
    fn test_tag_new() {
        let tag = Tag::new();
        assert!(!tag.0.is_empty());
    }

    #[test]
    fn test_tag_default() {
        let tag = Tag::default();
        assert!(!tag.0.is_empty());
    }

    // ---- CallId 测试 ----

    #[test]
    fn test_call_id_new() {
        let call_id = CallId::new();
        assert!(call_id.0.contains('@'));
        assert!(call_id.0.ends_with("@sip-rs"));
    }

    #[test]
    fn test_call_id_with_host() {
        let call_id = CallId::with_host("example.com");
        assert!(call_id.0.contains('@'));
        assert!(call_id.0.ends_with("@example.com"));
    }

    #[test]
    fn test_call_id_default() {
        let call_id = CallId::default();
        assert!(call_id.0.contains('@'));
    }

    // ---- Body 测试 ----

    #[test]
    fn test_body_new() {
        let body = Body::new("application/sdp", b"v=0\r\n".to_vec());
        assert_eq!(body.content_type, "application/sdp");
        assert_eq!(body.content, b"v=0\r\n");
    }

    #[test]
    fn test_body_len() {
        let body = Body::new("text/plain", b"hello".to_vec());
        assert_eq!(body.len(), 5);
    }

    #[test]
    fn test_body_is_empty() {
        let body = Body::new("text/plain", Vec::new());
        assert!(body.is_empty());

        let body = Body::new("text/plain", b"a".to_vec());
        assert!(!body.is_empty());
    }

    // ---- RequestLine 测试 ----

    #[test]
    fn test_request_line_display() {
        let uri = SipUri::parse("sip:bob@example.com").unwrap();
        let rl = RequestLine {
            method: Method::Invite,
            request_uri: uri,
            version: SipVersion,
        };
        assert_eq!(rl.to_string(), "INVITE sip:bob@example.com SIP/2.0");
    }

    // ---- StatusLine 测试 ----

    #[test]
    fn test_status_line_display() {
        let sl = StatusLine {
            version: SipVersion,
            status_code: StatusCode::OK,
            reason_phrase: "OK".to_string(),
        };
        assert_eq!(sl.to_string(), "SIP/2.0 200 OK");
    }

    // ---- SipMessage 测试 ----

    #[test]
    fn test_sip_message_is_request() {
        let uri = SipUri::parse("sip:bob@example.com").unwrap();
        let req = SipRequest {
            request_line: RequestLine {
                method: Method::Invite,
                request_uri: uri,
                version: SipVersion,
            },
            headers: HeaderCollection::new(),
            body: None,
        };
        let msg = SipMessage::Request(req);
        assert!(msg.is_request());
        assert!(!msg.is_response());
    }

    #[test]
    fn test_sip_message_is_response() {
        let resp = SipResponse {
            status_line: StatusLine {
                version: SipVersion,
                status_code: StatusCode::OK,
                reason_phrase: "OK".to_string(),
            },
            headers: HeaderCollection::new(),
            body: None,
        };
        let msg = SipMessage::Response(resp);
        assert!(msg.is_response());
        assert!(!msg.is_request());
    }

    #[test]
    fn test_sip_message_content_length() {
        let mut headers = HeaderCollection::new();
        headers.insert(HeaderName::ContentLength, HeaderValue::ContentLength(42));
        let resp = SipResponse {
            status_line: StatusLine {
                version: SipVersion,
                status_code: StatusCode::OK,
                reason_phrase: "OK".to_string(),
            },
            headers,
            body: None,
        };
        let msg = SipMessage::Response(resp);
        assert_eq!(msg.content_length(), Some(42));
    }
}
