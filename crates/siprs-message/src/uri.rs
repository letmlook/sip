//! SIP URI 解析与构建
//!
//! 实现符合 RFC 3261 Section 19.1 的 SIP URI 解析和序列化。
//! 支持 `sip:` 和 `sips:` 两种 scheme，以及 RFC 7118 定义的
//! `sip+ws:` 和 `sip+wss:` WebSocket URI scheme。
//! 支持 IPv6 地址（方括号包裹），支持 URI 参数和头部参数。

use std::fmt;
use std::str::FromStr;

use siprs_core::{Host, ParseError};

// ============================================================================
// UriScheme - SIP URI scheme
// ============================================================================

/// SIP URI scheme
///
/// 支持 RFC 3261 定义的 `sip:` 和 `sips:` scheme，
/// 以及 RFC 7118 定义的 `sip+ws:` 和 `sip+wss:` WebSocket scheme。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UriScheme {
    /// `sip:` scheme
    Sip,
    /// `sips:` scheme（安全 SIP）
    Sips,
    /// `sip+ws:` scheme（SIP over WebSocket，RFC 7118）
    SipWs,
    /// `sip+wss:` scheme（SIP over WebSocket Secure，RFC 7118）
    SipWss,
}

impl fmt::Display for UriScheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sip => write!(f, "sip"),
            Self::Sips => write!(f, "sips"),
            Self::SipWs => write!(f, "sip+ws"),
            Self::SipWss => write!(f, "sip+wss"),
        }
    }
}

impl FromStr for UriScheme {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "sip" => Ok(Self::Sip),
            "sips" => Ok(Self::Sips),
            "sip+ws" => Ok(Self::SipWs),
            "sip+wss" => Ok(Self::SipWss),
            _ => Err(ParseError::InvalidUri {
                detail: format!("unknown URI scheme: {}", s),
            }),
        }
    }
}

// ============================================================================
// UserInfo - 用户信息组件
// ============================================================================

/// 用户信息组件
///
/// 包含用户名和可选密码，格式为 `user[:password]`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserInfo {
    /// 用户名
    pub user: String,
    /// 密码（可选）
    pub password: Option<String>,
}

impl fmt::Display for UserInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.user)?;
        if let Some(ref pw) = self.password {
            write!(f, ":{}", pw)?;
        }
        Ok(())
    }
}

// ============================================================================
// UriParams - URI 参数集合
// ============================================================================

/// URI 参数集合（分号分隔的 key-value 对）
///
/// SIP URI 中 `;` 分隔的参数，如 `;transport=tcp;lr`。
/// 参数可以有值（`key=value`）也可以无值（`key`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UriParams {
    params: Vec<(String, Option<String>)>,
}

impl UriParams {
    /// 创建空的参数集合
    pub fn new() -> Self {
        Self { params: Vec::new() }
    }

    /// 添加参数
    pub fn insert(&mut self, key: impl Into<String>, value: Option<String>) {
        self.params.push((key.into(), value));
    }

    /// 获取参数值
    pub fn get(&self, key: &str) -> Option<&Option<String>> {
        self.params
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    }

    /// 返回传输协议参数
    pub fn transport(&self) -> Option<&str> {
        self.get("transport")
            .and_then(|v| v.as_ref().map(|s| s.as_str()))
    }

    /// 返回松散路由标志（lr 参数）
    pub fn lr(&self) -> bool {
        self.get("lr").is_some()
    }

    /// 返回参数数量
    pub fn len(&self) -> usize {
        self.params.len()
    }

    /// 判断参数集合是否为空
    pub fn is_empty(&self) -> bool {
        self.params.is_empty()
    }

    /// 返回参数迭代器
    pub fn iter(&self) -> impl Iterator<Item = &(String, Option<String>)> {
        self.params.iter()
    }
}

impl Default for UriParams {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for UriParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (key, value) in &self.params {
            write!(f, ";{}", key)?;
            if let Some(ref v) = value {
                write!(f, "={}", v)?;
            }
        }
        Ok(())
    }
}

// ============================================================================
// UriHeaders - URI 头部参数集合
// ============================================================================

/// URI 头部参数集合（问号分隔的 key-value 对）
///
/// SIP URI 中 `?` 后的头部参数，如 `?header=value`。
/// 多个头部参数用 `&` 分隔。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UriHeaders {
    headers: Vec<(String, String)>,
}

impl UriHeaders {
    /// 创建空的头部参数集合
    pub fn new() -> Self {
        Self {
            headers: Vec::new(),
        }
    }

    /// 添加头部参数
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.headers.push((key.into(), value.into()));
    }

    /// 获取头部参数值
    pub fn get(&self, key: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    /// 返回头部参数数量
    pub fn len(&self) -> usize {
        self.headers.len()
    }

    /// 判断头部参数集合是否为空
    pub fn is_empty(&self) -> bool {
        self.headers.is_empty()
    }

    /// 返回头部参数迭代器
    pub fn iter(&self) -> impl Iterator<Item = &(String, String)> {
        self.headers.iter()
    }
}

impl Default for UriHeaders {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for UriHeaders {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.headers.is_empty() {
            return Ok(());
        }
        write!(f, "?")?;
        for (i, (key, value)) in self.headers.iter().enumerate() {
            if i > 0 {
                write!(f, "&")?;
            }
            write!(f, "{}={}", key, value)?;
        }
        Ok(())
    }
}

// ============================================================================
// SipUri - SIP URI
// ============================================================================

/// SIP URI
///
/// 符合 RFC 3261 Section 19.1 的 SIP URI 格式：
/// ```text
/// sip:user:password@host:port;uri-params?uri-headers
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SipUri {
    /// URI scheme（sip 或 sips）
    pub scheme: UriScheme,
    /// 用户信息（可选）
    pub user_info: Option<UserInfo>,
    /// 主机地址
    pub host: Host,
    /// 端口号（可选）
    pub port: Option<u16>,
    /// URI 参数
    pub params: UriParams,
    /// URI 头部参数
    pub headers: UriHeaders,
}

impl SipUri {
    /// 从字符串解析 SIP URI
    ///
    /// 支持格式：
    /// - `sip:alice@example.com`
    /// - `sip:alice:password@example.com:5060;transport=tcp?header=value`
    /// - `sips:bob@example.com`
    /// - `sip:user@[::1]:5060`
    pub fn parse(s: &str) -> Result<Self, ParseError> {
        let s = s.trim();

        // 1. 解析 scheme
        let colon_pos = s.find(':').ok_or_else(|| ParseError::InvalidUri {
            detail: "missing ':' after scheme".to_string(),
        })?;
        let scheme: UriScheme = s[..colon_pos].parse()?;
        let rest = &s[colon_pos + 1..];

        // 2. 分离 URI 头部参数（? 之后的部分）
        let (main_part, headers_str) = if let Some(q_pos) = rest.find('?') {
            (&rest[..q_pos], Some(&rest[q_pos + 1..]))
        } else {
            (rest, None)
        };

        // 3. 分离 URI 参数（; 之后的部分，但需要注意 IPv6 地址中的冒号）
        // 我们需要找到 host 部分，参数在 host:port 之后
        // 先分离用户信息部分和 host+params 部分
        let (user_info_str, host_params_str) = split_user_info_from_host(main_part);

        // 4. 解析用户信息
        let user_info = if let Some(ui_str) = user_info_str {
            Some(parse_user_info(ui_str)?)
        } else {
            None
        };

        // 5. 分离 host:port 和 URI 参数
        let (host_port_str, params_str) = split_params_from_host_port(host_params_str);

        // 6. 解析 host 和 port
        let (host, port) = parse_host_port(host_port_str)?;

        // 7. 解析 URI 参数
        let params = match params_str {
            Some(s) => parse_uri_params(s)?,
            None => UriParams::new(),
        };

        // 8. 解析 URI 头部参数
        let headers = if let Some(h_str) = headers_str {
            parse_uri_headers(h_str)?
        } else {
            UriHeaders::new()
        };

        Ok(Self {
            scheme,
            user_info,
            host,
            port,
            params,
            headers,
        })
    }

    /// 获取传输协议参数
    pub fn transport(&self) -> Option<&str> {
        self.params.transport()
    }

    /// 判断是否为松散路由
    pub fn lr(&self) -> bool {
        self.params.lr()
    }
}

impl fmt::Display for SipUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:", self.scheme)?;
        if let Some(ref ui) = self.user_info {
            write!(f, "{}@", ui)?;
        }
        write!(f, "{}", self.host)?;
        if let Some(port) = self.port {
            write!(f, ":{}", port)?;
        }
        write!(f, "{}{}", self.params, self.headers)
    }
}

impl FromStr for SipUri {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

// ============================================================================
// 内部解析辅助函数
// ============================================================================

/// 分离用户信息部分和 host+params 部分
///
/// 用户信息在 `@` 之前，host+params 在 `@` 之后。
/// 需要处理 IPv6 地址中的 `@` 不会出现在方括号内的情况。
fn split_user_info_from_host(s: &str) -> (Option<&str>, &str) {
    // 查找最后一个不在方括号内的 @
    let mut in_brackets = false;
    let mut last_at = None;

    for (i, c) in s.char_indices() {
        match c {
            '[' => in_brackets = true,
            ']' => in_brackets = false,
            '@' if !in_brackets => last_at = Some(i),
            _ => {}
        }
    }

    match last_at {
        Some(pos) => (Some(&s[..pos]), &s[pos + 1..]),
        None => (None, s),
    }
}

/// 分离 host:port 和 URI 参数
///
/// URI 参数以 `;` 开头，但需要忽略方括号内的分号（IPv6 地址）。
fn split_params_from_host_port(s: &str) -> (&str, Option<&str>) {
    let mut in_brackets = false;

    for (i, c) in s.char_indices() {
        match c {
            '[' => in_brackets = true,
            ']' => in_brackets = false,
            ';' if !in_brackets => return (&s[..i], Some(&s[i + 1..])),
            _ => {}
        }
    }

    (s, None)
}

/// 解析用户信息 `user[:password]`
fn parse_user_info(s: &str) -> Result<UserInfo, ParseError> {
    if s.is_empty() {
        return Err(ParseError::InvalidUri {
            detail: "empty user info".to_string(),
        });
    }

    if let Some(colon_pos) = s.find(':') {
        let user = s[..colon_pos].to_string();
        let password = s[colon_pos + 1..].to_string();
        if user.is_empty() {
            return Err(ParseError::InvalidUri {
                detail: "empty user name".to_string(),
            });
        }
        Ok(UserInfo {
            user,
            password: if password.is_empty() {
                None
            } else {
                Some(password)
            },
        })
    } else {
        if s.is_empty() {
            return Err(ParseError::InvalidUri {
                detail: "empty user name".to_string(),
            });
        }
        Ok(UserInfo {
            user: s.to_string(),
            password: None,
        })
    }
}

/// 解析 host:port
fn parse_host_port(s: &str) -> Result<(Host, Option<u16>), ParseError> {
    if s.is_empty() {
        return Err(ParseError::InvalidUri {
            detail: "empty host".to_string(),
        });
    }

    // 处理 IPv6 地址（方括号包裹）
    if s.starts_with('[') {
        // 找到结束方括号
        let bracket_end = s.find(']').ok_or_else(|| ParseError::InvalidUri {
            detail: "unclosed IPv6 bracket".to_string(),
        })?;
        let ipv6_str = &s[1..bracket_end];
        let host: Host = format!("[{}]", ipv6_str)
            .parse()
            .map_err(|_| ParseError::InvalidUri {
                detail: format!("invalid IPv6 address: {}", ipv6_str),
            })?;

        // 检查端口
        let rest = &s[bracket_end + 1..];
        let port = if let Some(port_str) = rest.strip_prefix(':') {
            if port_str.is_empty() {
                return Err(ParseError::InvalidUri {
                    detail: "empty port after colon".to_string(),
                });
            }
            Some(
                port_str
                    .parse::<u16>()
                    .map_err(|_| ParseError::InvalidUri {
                        detail: format!("invalid port: {}", port_str),
                    })?,
            )
        } else if rest.is_empty() {
            None
        } else {
            return Err(ParseError::InvalidUri {
                detail: format!("unexpected characters after IPv6 address: {}", rest),
            });
        };

        Ok((host, port))
    } else {
        // 域名或 IPv4 地址
        if let Some(colon_pos) = s.rfind(':') {
            let host_str = &s[..colon_pos];
            let port_str = &s[colon_pos + 1..];
            let host: Host = host_str.parse().map_err(|_| ParseError::InvalidUri {
                detail: format!("invalid host: {}", host_str),
            })?;
            let port = port_str
                .parse::<u16>()
                .map_err(|_| ParseError::InvalidUri {
                    detail: format!("invalid port: {}", port_str),
                })?;
            Ok((host, Some(port)))
        } else {
            let host: Host = s.parse().map_err(|_| ParseError::InvalidUri {
                detail: format!("invalid host: {}", s),
            })?;
            Ok((host, None))
        }
    }
}

/// 解析 URI 参数（分号分隔的 key[=value] 对）
fn parse_uri_params(s: &str) -> Result<UriParams, ParseError> {
    let mut params = UriParams::new();
    if s.is_empty() {
        return Ok(params);
    }

    for param in s.split(';') {
        let param = param.trim();
        if param.is_empty() {
            continue;
        }
        if let Some(eq_pos) = param.find('=') {
            let key = param[..eq_pos].to_string();
            let value = param[eq_pos + 1..].to_string();
            params.insert(key, Some(value));
        } else {
            params.insert(param.to_string(), None);
        }
    }

    Ok(params)
}

/// 解析 URI 头部参数（& 分隔的 key=value 对）
fn parse_uri_headers(s: &str) -> Result<UriHeaders, ParseError> {
    let mut headers = UriHeaders::new();
    if s.is_empty() {
        return Ok(headers);
    }

    for header in s.split('&') {
        let header = header.trim();
        if header.is_empty() {
            continue;
        }
        if let Some(eq_pos) = header.find('=') {
            let key = header[..eq_pos].to_string();
            let value = header[eq_pos + 1..].to_string();
            headers.insert(key, value);
        } else {
            // 无值的头部参数，用空字符串作为值
            headers.insert(header.to_string(), "");
        }
    }

    Ok(headers)
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- UriScheme 测试 ----

    #[test]
    fn test_uri_scheme_display() {
        assert_eq!(UriScheme::Sip.to_string(), "sip");
        assert_eq!(UriScheme::Sips.to_string(), "sips");
        assert_eq!(UriScheme::SipWs.to_string(), "sip+ws");
        assert_eq!(UriScheme::SipWss.to_string(), "sip+wss");
    }

    #[test]
    fn test_uri_scheme_from_str() {
        assert!(matches!("sip".parse::<UriScheme>(), Ok(UriScheme::Sip)));
        assert!(matches!("SIPS".parse::<UriScheme>(), Ok(UriScheme::Sips)));
        assert!(matches!("sip+ws".parse::<UriScheme>(), Ok(UriScheme::SipWs)));
        assert!(matches!("SIP+WSS".parse::<UriScheme>(), Ok(UriScheme::SipWss)));
        assert!("http".parse::<UriScheme>().is_err());
    }

    // ---- UserInfo 测试 ----

    #[test]
    fn test_user_info_display() {
        let ui = UserInfo {
            user: "alice".to_string(),
            password: None,
        };
        assert_eq!(ui.to_string(), "alice");

        let ui_with_pw = UserInfo {
            user: "alice".to_string(),
            password: Some("secret".to_string()),
        };
        assert_eq!(ui_with_pw.to_string(), "alice:secret");
    }

    // ---- UriParams 测试 ----

    #[test]
    fn test_uri_params_transport() {
        let mut params = UriParams::new();
        params.insert("transport", Some("tcp".to_string()));
        params.insert("lr", None);
        assert_eq!(params.transport(), Some("tcp"));
        assert!(params.lr());
    }

    #[test]
    fn test_uri_params_display() {
        let mut params = UriParams::new();
        params.insert("transport", Some("tcp".to_string()));
        params.insert("lr", None);
        assert_eq!(params.to_string(), ";transport=tcp;lr");
    }

    // ---- UriHeaders 测试 ----

    #[test]
    fn test_uri_headers_display() {
        let mut headers = UriHeaders::new();
        headers.insert("header", "value");
        assert_eq!(headers.to_string(), "?header=value");

        let mut headers2 = UriHeaders::new();
        headers2.insert("h1", "v1");
        headers2.insert("h2", "v2");
        assert_eq!(headers2.to_string(), "?h1=v1&h2=v2");
    }

    // ---- SipUri 解析测试 ----

    #[test]
    fn test_parse_simple_sip_uri() {
        let uri = SipUri::parse("sip:bob@example.com").unwrap();
        assert_eq!(uri.scheme, UriScheme::Sip);
        assert!(uri.user_info.is_some());
        assert_eq!(uri.user_info.as_ref().unwrap().user, "bob");
        assert_eq!(uri.host, Host::Domain("example.com".to_string()));
        assert!(uri.port.is_none());
    }

    #[test]
    fn test_parse_sips_uri() {
        let uri = SipUri::parse("sips:bob@example.com").unwrap();
        assert_eq!(uri.scheme, UriScheme::Sips);
        assert_eq!(uri.user_info.as_ref().unwrap().user, "bob");
        assert_eq!(uri.host, Host::Domain("example.com".to_string()));
    }

    #[test]
    fn test_parse_full_uri() {
        let uri = SipUri::parse("sip:alice:password@example.com:5060;transport=tcp?header=value")
            .unwrap();
        assert_eq!(uri.scheme, UriScheme::Sip);
        let ui = uri.user_info.as_ref().unwrap();
        assert_eq!(ui.user, "alice");
        assert_eq!(ui.password.as_ref().unwrap(), "password");
        assert_eq!(uri.host, Host::Domain("example.com".to_string()));
        assert_eq!(uri.port, Some(5060));
        assert_eq!(uri.params.transport(), Some("tcp"));
        assert_eq!(uri.headers.get("header"), Some("value"));
    }

    #[test]
    fn test_parse_ipv6_uri() {
        let uri = SipUri::parse("sip:user@[::1]:5060").unwrap();
        assert_eq!(uri.scheme, UriScheme::Sip);
        assert_eq!(uri.user_info.as_ref().unwrap().user, "user");
        assert!(matches!(uri.host, Host::IPv6(_)));
        assert_eq!(uri.port, Some(5060));
    }

    #[test]
    fn test_parse_ipv6_uri_no_port() {
        let uri = SipUri::parse("sip:user@[2001:db8::1]").unwrap();
        assert_eq!(uri.user_info.as_ref().unwrap().user, "user");
        assert!(matches!(uri.host, Host::IPv6(_)));
        assert!(uri.port.is_none());
    }

    #[test]
    fn test_parse_uri_with_lr_param() {
        let uri = SipUri::parse("sip:proxy.example.com;lr").unwrap();
        assert_eq!(uri.host, Host::Domain("proxy.example.com".to_string()));
        assert!(uri.lr());
        assert!(uri.user_info.is_none());
    }

    #[test]
    fn test_parse_uri_no_user() {
        let uri = SipUri::parse("sip:example.com:5060").unwrap();
        assert!(uri.user_info.is_none());
        assert_eq!(uri.host, Host::Domain("example.com".to_string()));
        assert_eq!(uri.port, Some(5060));
    }

    #[test]
    fn test_parse_uri_multiple_params() {
        let uri = SipUri::parse("sip:alice@example.com;transport=tcp;lr").unwrap();
        assert_eq!(uri.params.transport(), Some("tcp"));
        assert!(uri.lr());
    }

    // ---- 往返一致性测试 ----

    #[test]
    fn test_roundtrip_simple() {
        let input = "sip:bob@example.com";
        let uri = SipUri::parse(input).unwrap();
        let output = uri.to_string();
        let uri2 = SipUri::parse(&output).unwrap();
        assert_eq!(uri, uri2);
    }

    #[test]
    fn test_roundtrip_full() {
        let input = "sip:alice:password@example.com:5060;transport=tcp?header=value";
        let uri = SipUri::parse(input).unwrap();
        let output = uri.to_string();
        let uri2 = SipUri::parse(&output).unwrap();
        assert_eq!(uri, uri2);
    }

    #[test]
    fn test_roundtrip_ipv6() {
        let input = "sip:user@[::1]:5060";
        let uri = SipUri::parse(input).unwrap();
        let output = uri.to_string();
        let uri2 = SipUri::parse(&output).unwrap();
        assert_eq!(uri, uri2);
    }

    #[test]
    fn test_roundtrip_sips() {
        let input = "sips:bob@example.com";
        let uri = SipUri::parse(input).unwrap();
        let output = uri.to_string();
        let uri2 = SipUri::parse(&output).unwrap();
        assert_eq!(uri, uri2);
    }

    // ---- 错误场景测试 ----

    #[test]
    fn test_parse_invalid_scheme() {
        assert!(SipUri::parse("http:example.com").is_err());
    }

    #[test]
    fn test_parse_missing_colon() {
        assert!(SipUri::parse("sipexample.com").is_err());
    }

    #[test]
    fn test_parse_unclosed_ipv6_bracket() {
        assert!(SipUri::parse("sip:user@[::1").is_err());
    }

    #[test]
    fn test_parse_empty_host() {
        assert!(SipUri::parse("sip:user@").is_err());
    }

    // ---- FromStr 测试 ----

    #[test]
    fn test_from_str() {
        let uri: SipUri = "sip:alice@example.com".parse().unwrap();
        assert_eq!(uri.scheme, UriScheme::Sip);
    }
}
