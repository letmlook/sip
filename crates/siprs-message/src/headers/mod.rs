//! SIP 消息头类型定义与集合
//!
//! 定义 HeaderName 枚举、HeaderValue 枚举、HeaderCollection 集合类型，
//! 以及消息头的解析和构建功能。

pub mod auth;
pub mod contact;
pub mod cseq;
pub mod from_to;
pub mod route;
pub mod via;

use std::fmt;
use std::str::FromStr;

use siprs_core::ParseError;

use crate::types::{CallId, Method};

pub use auth::AuthHeader;
pub use contact::ContactHeader;
pub use cseq::CSeqHeader;
pub use from_to::FromToHeader;
pub use route::RouteHeader;
pub use via::{SentBy, ViaHeader};

// ============================================================================
// HeaderName - 头部名称枚举
// ============================================================================

/// SIP 头部名称
///
/// 涵盖 RFC 3261 定义的核心头部和常用扩展头部，
/// 支持自定义扩展头部名。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HeaderName {
    /// Via 头部
    Via,
    /// From 头部
    From,
    /// To 头部
    To,
    /// Call-ID 头部
    CallId,
    /// CSeq 头部
    CSeq,
    /// Contact 头部
    Contact,
    /// Max-Forwards 头部
    MaxForwards,
    /// Content-Type 头部
    ContentType,
    /// Content-Length 头部
    ContentLength,
    /// Route 头部
    Route,
    /// Record-Route 头部
    RecordRoute,
    /// Expires 头部
    Expires,
    /// Allow 头部
    Allow,
    /// Supported 头部
    Supported,
    /// Require 头部
    Require,
    /// Authorization 头部
    Authorization,
    /// Proxy-Authorization 头部
    ProxyAuthorization,
    /// WWW-Authenticate 头部
    WwwAuthenticate,
    /// Proxy-Authenticate 头部
    ProxyAuthenticate,
    /// Date 头部
    Date,
    /// Min-Expires 头部
    MinExpires,
    /// Subject 头部
    Subject,
    /// User-Agent 头部
    UserAgent,
    /// Server 头部
    Server,
    /// Reason 头部
    Reason,
    /// Warning 头部
    Warning,
    /// 扩展头部
    Extension(String),
}

impl HeaderName {
    /// 从字符串解析头部名称（大小写不敏感）
    pub fn from_str_case_insensitive(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "via" => Self::Via,
            "from" => Self::From,
            "to" => Self::To,
            "call-id" | "i" => Self::CallId,
            "cseq" => Self::CSeq,
            "contact" | "m" => Self::Contact,
            "max-forwards" => Self::MaxForwards,
            "content-type" | "c" => Self::ContentType,
            "content-length" | "l" => Self::ContentLength,
            "route" => Self::Route,
            "record-route" => Self::RecordRoute,
            "expires" => Self::Expires,
            "allow" => Self::Allow,
            "supported" | "k" => Self::Supported,
            "require" => Self::Require,
            "authorization" => Self::Authorization,
            "proxy-authorization" => Self::ProxyAuthorization,
            "www-authenticate" => Self::WwwAuthenticate,
            "proxy-authenticate" => Self::ProxyAuthenticate,
            "date" => Self::Date,
            "min-expires" => Self::MinExpires,
            "subject" | "s" => Self::Subject,
            "user-agent" => Self::UserAgent,
            "server" => Self::Server,
            "reason" => Self::Reason,
            "warning" => Self::Warning,
            _ => Self::Extension(s.to_string()),
        }
    }
}

impl fmt::Display for HeaderName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Via => write!(f, "Via"),
            Self::From => write!(f, "From"),
            Self::To => write!(f, "To"),
            Self::CallId => write!(f, "Call-ID"),
            Self::CSeq => write!(f, "CSeq"),
            Self::Contact => write!(f, "Contact"),
            Self::MaxForwards => write!(f, "Max-Forwards"),
            Self::ContentType => write!(f, "Content-Type"),
            Self::ContentLength => write!(f, "Content-Length"),
            Self::Route => write!(f, "Route"),
            Self::RecordRoute => write!(f, "Record-Route"),
            Self::Expires => write!(f, "Expires"),
            Self::Allow => write!(f, "Allow"),
            Self::Supported => write!(f, "Supported"),
            Self::Require => write!(f, "Require"),
            Self::Authorization => write!(f, "Authorization"),
            Self::ProxyAuthorization => write!(f, "Proxy-Authorization"),
            Self::WwwAuthenticate => write!(f, "WWW-Authenticate"),
            Self::ProxyAuthenticate => write!(f, "Proxy-Authenticate"),
            Self::Date => write!(f, "Date"),
            Self::MinExpires => write!(f, "Min-Expires"),
            Self::Subject => write!(f, "Subject"),
            Self::UserAgent => write!(f, "User-Agent"),
            Self::Server => write!(f, "Server"),
            Self::Reason => write!(f, "Reason"),
            Self::Warning => write!(f, "Warning"),
            Self::Extension(name) => write!(f, "{}", name),
        }
    }
}

// ============================================================================
// HeaderValue - 头部值枚举
// ============================================================================

/// SIP 头部值
///
/// 每种头部类型对应一个强类型的变体，无法识别的头部值使用 Raw 变体。
#[derive(Debug, Clone)]
pub enum HeaderValue {
    /// Via 头部值
    Via(ViaHeader),
    /// From/To 头部值
    FromTo(FromToHeader),
    /// Call-ID 头部值
    CallId(CallId),
    /// CSeq 头部值
    CSeq(CSeqHeader),
    /// Contact 头部值
    Contact(ContactHeader),
    /// Max-Forwards 头部值
    MaxForwards(u32),
    /// Content-Type 头部值
    ContentType(String),
    /// Content-Length 头部值
    ContentLength(usize),
    /// Route 头部值
    Route(RouteHeader),
    /// Expires 头部值
    Expires(u32),
    /// Allow 头部值
    Allow(Vec<Method>),
    /// Authorization 相关头部值
    Auth(AuthHeader),
    /// 原始字符串值
    Raw(String),
}

impl HeaderValue {
    /// 获取 Via 头部值的引用
    pub fn as_via(&self) -> Option<&ViaHeader> {
        match self {
            Self::Via(v) => Some(v),
            _ => None,
        }
    }

    /// 获取 FromTo 头部值的引用
    pub fn as_from_to(&self) -> Option<&FromToHeader> {
        match self {
            Self::FromTo(v) => Some(v),
            _ => None,
        }
    }

    /// 获取 CallId 头部值的引用
    pub fn as_call_id(&self) -> Option<&CallId> {
        match self {
            Self::CallId(v) => Some(v),
            _ => None,
        }
    }

    /// 获取 CSeq 头部值的引用
    pub fn as_cseq(&self) -> Option<&CSeqHeader> {
        match self {
            Self::CSeq(v) => Some(v),
            _ => None,
        }
    }

    /// 获取 Contact 头部值的引用
    pub fn as_contact(&self) -> Option<&ContactHeader> {
        match self {
            Self::Contact(v) => Some(v),
            _ => None,
        }
    }

    /// 获取 MaxForwards 头部值
    pub fn as_max_forwards(&self) -> Option<u32> {
        match self {
            Self::MaxForwards(v) => Some(*v),
            _ => None,
        }
    }

    /// 获取 Content-Length 头部值
    pub fn as_content_length(&self) -> Option<usize> {
        match self {
            Self::ContentLength(v) => Some(*v),
            _ => None,
        }
    }

    /// 获取 Auth 头部值的引用
    pub fn as_auth(&self) -> Option<&AuthHeader> {
        match self {
            Self::Auth(v) => Some(v),
            _ => None,
        }
    }
}

impl fmt::Display for HeaderValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Via(v) => write!(f, "{}", v),
            Self::FromTo(v) => write!(f, "{}", v),
            Self::CallId(v) => write!(f, "{}", v),
            Self::CSeq(v) => write!(f, "{}", v),
            Self::Contact(v) => write!(f, "{}", v),
            Self::MaxForwards(v) => write!(f, "{}", v),
            Self::ContentType(v) => write!(f, "{}", v),
            Self::ContentLength(v) => write!(f, "{}", v),
            Self::Route(v) => write!(f, "{}", v),
            Self::Expires(v) => write!(f, "{}", v),
            Self::Allow(methods) => {
                let strs: Vec<String> = methods.iter().map(|m| m.to_string()).collect();
                write!(f, "{}", strs.join(", "))
            }
            Self::Auth(v) => write!(f, "{}", v),
            Self::Raw(v) => write!(f, "{}", v),
        }
    }
}

// ============================================================================
// HeaderCollection - 头部集合
// ============================================================================

/// SIP 消息头部集合
///
/// 有序的头部列表，支持按名称查询、添加、删除和遍历。
/// 同名头部（如多个 Via）按添加顺序保留。
#[derive(Debug, Clone)]
pub struct HeaderCollection {
    headers: Vec<(HeaderName, HeaderValue)>,
}

impl HeaderCollection {
    /// 创建空的头部集合
    pub fn new() -> Self {
        Self {
            headers: Vec::new(),
        }
    }

    /// 按名称查询第一个匹配的头部值
    pub fn get(&self, name: &HeaderName) -> Option<&HeaderValue> {
        self.headers.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    /// 获取所有同名头部的值
    pub fn get_all(&self, name: &HeaderName) -> Vec<&HeaderValue> {
        self.headers
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| v)
            .collect()
    }

    /// 添加头部
    pub fn insert(&mut self, name: HeaderName, value: HeaderValue) {
        self.headers.push((name, value));
    }

    /// 删除所有同名头部
    pub fn remove(&mut self, name: &HeaderName) {
        self.headers.retain(|(n, _)| n != name);
    }

    /// 遍历所有头部
    pub fn iter(&self) -> impl Iterator<Item = (&HeaderName, &HeaderValue)> {
        self.headers.iter().map(|(n, v)| (n, v))
    }

    /// 返回头部数量
    pub fn len(&self) -> usize {
        self.headers.len()
    }

    /// 判断头部集合是否为空
    pub fn is_empty(&self) -> bool {
        self.headers.is_empty()
    }

    /// 检查是否包含指定名称的头部
    pub fn contains(&self, name: &HeaderName) -> bool {
        self.headers.iter().any(|(n, _)| n == name)
    }
}

impl Default for HeaderCollection {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for HeaderCollection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, value) in &self.headers {
            write!(f, "{}: {}\r\n", name, value)?;
        }
        Ok(())
    }
}

// ============================================================================
// 头部解析
// ============================================================================

/// 解析警告
///
/// 对格式错误的头部标记为解析警告，保留原始值。
#[derive(Debug, Clone)]
pub struct ParseWarning {
    /// 头部名称
    pub name: String,
    /// 警告描述
    pub detail: String,
    /// 保留的原始值
    pub raw_value: String,
}

/// 解析头部行的结果
#[derive(Debug, Clone)]
pub struct ParseHeadersResult {
    /// 解析成功的头部集合
    pub headers: HeaderCollection,
    /// 解析警告列表
    pub warnings: Vec<ParseWarning>,
}

/// 从原始字节流解析消息头
///
/// 处理长行折叠（CRLF 后跟空格或制表符的行续接），
/// 将头部名称映射为 HeaderName 枚举，
/// 将头部值解析为对应的 HeaderValue 结构化类型。
pub fn parse_headers(raw: &[u8]) -> Result<ParseHeadersResult, ParseError> {
    let text = std::str::from_utf8(raw)?;
    parse_headers_from_str(text)
}

/// 从字符串解析消息头
fn parse_headers_from_str(text: &str) -> Result<ParseHeadersResult, ParseError> {
    let mut headers = HeaderCollection::new();
    let mut warnings = Vec::new();

    // 处理长行折叠：CRLF 后跟空格或制表符的行续接
    let unfolded = unfold_headers(text);

    for line in unfolded.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // 解析头部名称和值
        let colon_pos = match line.find(':') {
            Some(pos) => pos,
            None => {
                warnings.push(ParseWarning {
                    name: line.to_string(),
                    detail: "missing colon in header line".to_string(),
                    raw_value: line.to_string(),
                });
                continue;
            }
        };

        let name_str = line[..colon_pos].trim();
        let value_str = line[colon_pos + 1..].trim();

        let header_name = HeaderName::from_str_case_insensitive(name_str);

        let header_value = parse_header_value(&header_name, value_str, &mut warnings);

        headers.insert(header_name, header_value);
    }

    Ok(ParseHeadersResult { headers, warnings })
}

/// 处理长行折叠
///
/// SIP 消息中，如果一个行以 CRLF 后跟空格或制表符开头，
/// 则该行是前一行的续行，需要合并。
fn unfold_headers(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\r' {
            // 检查 CRLF 后是否跟空格或制表符
            if chars.peek() == Some(&'\n') {
                chars.next(); // 消费 \n
                if chars.peek() == Some(&' ') || chars.peek() == Some(&'\t') {
                    // 折叠行：跳过 CRLF 和空白，继续
                    // 消费连续的空格或制表符
                    while chars.peek() == Some(&' ') || chars.peek() == Some(&'\t') {
                        chars.next();
                    }
                    // 添加一个空格作为分隔
                    result.push(' ');
                } else {
                    // 非折叠行：保留 CRLF
                    result.push_str("\r\n");
                }
            } else {
                result.push(c);
            }
        } else if c == '\n' {
            // 处理单独的 LF（非标准但容错）
            if chars.peek() == Some(&' ') || chars.peek() == Some(&'\t') {
                while chars.peek() == Some(&' ') || chars.peek() == Some(&'\t') {
                    chars.next();
                }
                result.push(' ');
            } else {
                result.push('\n');
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// 解析单个头部值
fn parse_header_value(
    name: &HeaderName,
    value: &str,
    warnings: &mut Vec<ParseWarning>,
) -> HeaderValue {
    match name {
        HeaderName::Via => match ViaHeader::from_str(value) {
            Ok(via) => HeaderValue::Via(via),
            Err(e) => {
                warnings.push(ParseWarning {
                    name: name.to_string(),
                    detail: e.to_string(),
                    raw_value: value.to_string(),
                });
                HeaderValue::Raw(value.to_string())
            }
        },
        HeaderName::From | HeaderName::To => match FromToHeader::from_str(value) {
            Ok(ft) => HeaderValue::FromTo(ft),
            Err(e) => {
                warnings.push(ParseWarning {
                    name: name.to_string(),
                    detail: e.to_string(),
                    raw_value: value.to_string(),
                });
                HeaderValue::Raw(value.to_string())
            }
        },
        HeaderName::CallId => HeaderValue::CallId(CallId(value.to_string())),
        HeaderName::CSeq => match CSeqHeader::from_str(value) {
            Ok(cseq) => HeaderValue::CSeq(cseq),
            Err(e) => {
                warnings.push(ParseWarning {
                    name: name.to_string(),
                    detail: e.to_string(),
                    raw_value: value.to_string(),
                });
                HeaderValue::Raw(value.to_string())
            }
        },
        HeaderName::Contact => match ContactHeader::from_str(value) {
            Ok(contact) => HeaderValue::Contact(contact),
            Err(e) => {
                warnings.push(ParseWarning {
                    name: name.to_string(),
                    detail: e.to_string(),
                    raw_value: value.to_string(),
                });
                HeaderValue::Raw(value.to_string())
            }
        },
        HeaderName::MaxForwards => match value.trim().parse::<u32>() {
            Ok(v) => HeaderValue::MaxForwards(v),
            Err(_) => {
                warnings.push(ParseWarning {
                    name: name.to_string(),
                    detail: "invalid Max-Forwards value".to_string(),
                    raw_value: value.to_string(),
                });
                HeaderValue::Raw(value.to_string())
            }
        },
        HeaderName::ContentType => HeaderValue::ContentType(value.trim().to_string()),
        HeaderName::ContentLength => match value.trim().parse::<usize>() {
            Ok(v) => HeaderValue::ContentLength(v),
            Err(_) => {
                warnings.push(ParseWarning {
                    name: name.to_string(),
                    detail: "invalid Content-Length value".to_string(),
                    raw_value: value.to_string(),
                });
                HeaderValue::Raw(value.to_string())
            }
        },
        HeaderName::Route | HeaderName::RecordRoute => match RouteHeader::from_str(value) {
            Ok(route) => HeaderValue::Route(route),
            Err(e) => {
                warnings.push(ParseWarning {
                    name: name.to_string(),
                    detail: e.to_string(),
                    raw_value: value.to_string(),
                });
                HeaderValue::Raw(value.to_string())
            }
        },
        HeaderName::Expires => match value.trim().parse::<u32>() {
            Ok(v) => HeaderValue::Expires(v),
            Err(_) => {
                warnings.push(ParseWarning {
                    name: name.to_string(),
                    detail: "invalid Expires value".to_string(),
                    raw_value: value.to_string(),
                });
                HeaderValue::Raw(value.to_string())
            }
        },
        HeaderName::Allow => {
            let methods: Vec<Method> = value
                .split(',')
                .filter_map(|s| s.trim().parse::<Method>().ok())
                .collect();
            if methods.is_empty() {
                warnings.push(ParseWarning {
                    name: name.to_string(),
                    detail: "no valid methods in Allow header".to_string(),
                    raw_value: value.to_string(),
                });
                HeaderValue::Raw(value.to_string())
            } else {
                HeaderValue::Allow(methods)
            }
        }
        HeaderName::Authorization | HeaderName::ProxyAuthorization => {
            match AuthHeader::from_str(value) {
                Ok(auth) => HeaderValue::Auth(auth),
                Err(e) => {
                    warnings.push(ParseWarning {
                        name: name.to_string(),
                        detail: e.to_string(),
                        raw_value: value.to_string(),
                    });
                    HeaderValue::Raw(value.to_string())
                }
            }
        }
        HeaderName::WwwAuthenticate | HeaderName::ProxyAuthenticate => {
            match AuthHeader::from_str(value) {
                Ok(auth) => HeaderValue::Auth(auth),
                Err(e) => {
                    warnings.push(ParseWarning {
                        name: name.to_string(),
                        detail: e.to_string(),
                        raw_value: value.to_string(),
                    });
                    HeaderValue::Raw(value.to_string())
                }
            }
        }
        // 其他头部暂存为 Raw
        _ => HeaderValue::Raw(value.to_string()),
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    use siprs_core::Host;

    // ---- HeaderName 测试 ----

    #[test]
    fn test_header_name_from_str_case_insensitive() {
        assert_eq!(
            HeaderName::from_str_case_insensitive("Via"),
            HeaderName::Via
        );
        assert_eq!(
            HeaderName::from_str_case_insensitive("via"),
            HeaderName::Via
        );
        assert_eq!(
            HeaderName::from_str_case_insensitive("VIA"),
            HeaderName::Via
        );
        assert_eq!(
            HeaderName::from_str_case_insensitive("Call-ID"),
            HeaderName::CallId
        );
        assert_eq!(
            HeaderName::from_str_case_insensitive("call-id"),
            HeaderName::CallId
        );
        assert_eq!(
            HeaderName::from_str_case_insensitive("Content-Type"),
            HeaderName::ContentType
        );
        assert_eq!(
            HeaderName::from_str_case_insensitive("X-Custom"),
            HeaderName::Extension("X-Custom".to_string())
        );
    }

    #[test]
    fn test_header_name_display() {
        assert_eq!(HeaderName::Via.to_string(), "Via");
        assert_eq!(HeaderName::CallId.to_string(), "Call-ID");
        assert_eq!(HeaderName::ContentLength.to_string(), "Content-Length");
        assert_eq!(
            HeaderName::Extension("X-Custom".to_string()).to_string(),
            "X-Custom"
        );
    }

    // ---- HeaderCollection 测试 ----

    #[test]
    fn test_header_collection_insert_and_get() {
        let mut collection = HeaderCollection::new();
        collection.insert(
            HeaderName::CallId,
            HeaderValue::CallId(CallId("test-call-id".to_string())),
        );
        collection.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

        let call_id = collection.get(&HeaderName::CallId);
        assert!(call_id.is_some());
        assert_eq!(call_id.unwrap().as_call_id().unwrap().0, "test-call-id");

        let max_forwards = collection.get(&HeaderName::MaxForwards);
        assert!(max_forwards.is_some());
        assert_eq!(max_forwards.unwrap().as_max_forwards().unwrap(), 70);
    }

    #[test]
    fn test_header_collection_get_all() {
        let mut collection = HeaderCollection::new();
        let via1 = ViaHeader::new(
            siprs_core::TransportProtocol::Udp,
            Host::Domain("proxy1.example.com".to_string()),
            Some(5060),
        );
        let via2 = ViaHeader::new(
            siprs_core::TransportProtocol::Tcp,
            Host::Domain("proxy2.example.com".to_string()),
            Some(5060),
        );
        collection.insert(HeaderName::Via, HeaderValue::Via(via1));
        collection.insert(HeaderName::Via, HeaderValue::Via(via2));

        let vias = collection.get_all(&HeaderName::Via);
        assert_eq!(vias.len(), 2);
    }

    #[test]
    fn test_header_collection_remove() {
        let mut collection = HeaderCollection::new();
        collection.insert(
            HeaderName::CallId,
            HeaderValue::CallId(CallId("test".to_string())),
        );
        assert!(collection.contains(&HeaderName::CallId));

        collection.remove(&HeaderName::CallId);
        assert!(!collection.contains(&HeaderName::CallId));
    }

    #[test]
    fn test_header_collection_iter() {
        let mut collection = HeaderCollection::new();
        collection.insert(
            HeaderName::CallId,
            HeaderValue::CallId(CallId("test".to_string())),
        );
        collection.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

        let count = collection.iter().count();
        assert_eq!(count, 2);
    }

    // ---- 头部解析测试 ----

    #[test]
    fn test_parse_headers_basic() {
        let raw = b"Via: SIP/2.0/UDP 192.168.1.1:5060;branch=z9hG4bK-abc123\r\n\
                     Call-ID: test-call-id@example.com\r\n\
                     CSeq: 1 INVITE\r\n\
                     Max-Forwards: 70\r\n\
                     Content-Length: 0\r\n";

        let result = parse_headers(raw).unwrap();
        assert!(result.warnings.is_empty());

        let headers = &result.headers;
        assert!(headers.contains(&HeaderName::Via));
        assert!(headers.contains(&HeaderName::CallId));
        assert!(headers.contains(&HeaderName::CSeq));
        assert!(headers.contains(&HeaderName::MaxForwards));
        assert!(headers.contains(&HeaderName::ContentLength));

        let max_forwards = headers
            .get(&HeaderName::MaxForwards)
            .unwrap()
            .as_max_forwards()
            .unwrap();
        assert_eq!(max_forwards, 70);

        let content_length = headers
            .get(&HeaderName::ContentLength)
            .unwrap()
            .as_content_length()
            .unwrap();
        assert_eq!(content_length, 0);
    }

    #[test]
    fn test_parse_headers_with_folding() {
        let raw = b"Via: SIP/2.0/UDP 192.168.1.1:5060;\r\n branch=z9hG4bK-abc123\r\n\
                     Call-ID: test@example.com\r\n";

        let result = parse_headers(raw).unwrap();
        let headers = &result.headers;

        assert!(headers.contains(&HeaderName::Via));
        let via = headers.get(&HeaderName::Via).unwrap().as_via().unwrap();
        assert!(via.branch.is_valid());
    }

    #[test]
    fn test_parse_headers_case_insensitive() {
        let raw = b"via: SIP/2.0/UDP 192.168.1.1:5060;branch=z9hG4bK-abc\r\n\
                    call-id: test@example.com\r\n\
                    cseq: 1 INVITE\r\n";

        let result = parse_headers(raw).unwrap();
        let headers = &result.headers;

        assert!(headers.contains(&HeaderName::Via));
        assert!(headers.contains(&HeaderName::CallId));
        assert!(headers.contains(&HeaderName::CSeq));
    }

    #[test]
    fn test_parse_headers_malformed_preserves_raw() {
        let raw = b"Via: invalid-via-format\r\n\
                     Call-ID: test@example.com\r\n";

        let result = parse_headers(raw).unwrap();
        // Via 解析失败时应该有警告，且保留原始值
        assert!(!result.warnings.is_empty());

        let headers = &result.headers;
        // 原始值应保留为 Raw
        let via_value = headers.get(&HeaderName::Via).unwrap();
        assert!(matches!(via_value, HeaderValue::Raw(_)));
    }

    #[test]
    fn test_parse_headers_extension_header() {
        let raw = b"X-Custom: custom-value\r\n\
                     Call-ID: test@example.com\r\n";

        let result = parse_headers(raw).unwrap();
        let headers = &result.headers;

        assert!(headers.contains(&HeaderName::Extension("X-Custom".to_string())));
        let value = headers
            .get(&HeaderName::Extension("X-Custom".to_string()))
            .unwrap();
        assert!(matches!(value, HeaderValue::Raw(_)));
    }

    // ---- HeaderValue Display 测试 ----

    #[test]
    fn test_header_value_display() {
        let call_id = HeaderValue::CallId(CallId("test@example.com".to_string()));
        assert_eq!(call_id.to_string(), "test@example.com");

        let max_forwards = HeaderValue::MaxForwards(70);
        assert_eq!(max_forwards.to_string(), "70");

        let content_length = HeaderValue::ContentLength(1024);
        assert_eq!(content_length.to_string(), "1024");

        let expires = HeaderValue::Expires(3600);
        assert_eq!(expires.to_string(), "3600");

        let content_type = HeaderValue::ContentType("application/sdp".to_string());
        assert_eq!(content_type.to_string(), "application/sdp");
    }

    // ---- HeaderCollection Display 测试 ----

    #[test]
    fn test_header_collection_display() {
        let mut collection = HeaderCollection::new();
        collection.insert(
            HeaderName::CallId,
            HeaderValue::CallId(CallId("test".to_string())),
        );
        collection.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

        let s = collection.to_string();
        assert!(s.contains("Call-ID: test\r\n"));
        assert!(s.contains("Max-Forwards: 70\r\n"));
    }

    // ---- 展开折叠行测试 ----

    #[test]
    fn test_unfold_headers() {
        let folded = "Via: SIP/2.0/UDP proxy.example.com;\r\n branch=z9hG4bK-abc";
        let unfolded = unfold_headers(folded);
        assert_eq!(
            unfolded,
            "Via: SIP/2.0/UDP proxy.example.com; branch=z9hG4bK-abc"
        );
    }

    #[test]
    fn test_unfold_headers_tab() {
        let folded = "Via: SIP/2.0/UDP proxy.example.com;\r\n\tbranch=z9hG4bK-abc";
        let unfolded = unfold_headers(folded);
        assert_eq!(
            unfolded,
            "Via: SIP/2.0/UDP proxy.example.com; branch=z9hG4bK-abc"
        );
    }

    #[test]
    fn test_unfold_headers_no_fold() {
        let normal = "Via: SIP/2.0/UDP proxy.example.com;branch=z9hG4bK-abc";
        let unfolded = unfold_headers(normal);
        assert_eq!(unfolded, normal);
    }
}
