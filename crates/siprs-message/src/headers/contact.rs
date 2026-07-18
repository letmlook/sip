//! Contact 头部类型定义与解析
//!
//! Contact 头部提供发送者的可达联系地址，用于后续请求的直接路由。

use std::fmt;
use std::str::FromStr;

use siprs_core::ParseError;

use crate::uri::SipUri;

// ============================================================================
// ContactHeader - Contact 头部
// ============================================================================

/// Contact 头部
///
/// 格式：
/// ```text
/// Contact: Display-Name <sip:uri> [;expires=xxx]
/// Contact: *
/// ```
///
/// Contact 头部提供发送者的可达联系地址，用于后续请求的直接路由。
/// 特殊值 `*` 表示注销所有联系地址。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactHeader {
    /// 显示名称（可选）
    pub display_name: Option<String>,
    /// SIP URI
    pub uri: SipUri,
    /// expires 参数（可选，单位秒）
    pub expires: Option<u32>,
}

impl ContactHeader {
    /// 创建新的 Contact 头部
    pub fn new(uri: SipUri) -> Self {
        Self {
            display_name: None,
            uri,
            expires: None,
        }
    }

    /// 设置显示名称
    pub fn with_display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    /// 设置 expires 参数
    pub fn with_expires(mut self, expires: u32) -> Self {
        self.expires = Some(expires);
        self
    }
}

impl fmt::Display for ContactHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref name) = self.display_name {
            if name.contains(' ') || name.contains(',') || name.contains(';') {
                write!(f, "\"{}\" ", name)?;
            } else {
                write!(f, "{} ", name)?;
            }
        }
        write!(f, "<{}>", self.uri)?;
        if let Some(expires) = self.expires {
            write!(f, ";expires={}", expires)?;
        }
        Ok(())
    }
}

impl FromStr for ContactHeader {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, ParseError> {
        let s = s.trim();

        // 处理特殊值 *
        if s == "*" {
            return Err(ParseError::InvalidHeader {
                name: "Contact".to_string(),
                detail: "wildcard Contact (*) is not representable as ContactHeader".to_string(),
            });
        }

        let mut remaining = s;

        // 1. 解析显示名称
        let display_name;
        if remaining.starts_with('"') {
            if let Some(end_quote) = remaining[1..].find('"') {
                display_name = Some(remaining[1..end_quote + 1].to_string());
                remaining = remaining[end_quote + 2..].trim();
            } else {
                return Err(ParseError::InvalidHeader {
                    name: "Contact".to_string(),
                    detail: "unclosed quote in display name".to_string(),
                });
            }
        } else if !remaining.starts_with('<') {
            if let Some(lt_pos) = remaining.find('<') {
                let name = remaining[..lt_pos].trim();
                display_name = if name.is_empty() {
                    None
                } else {
                    Some(name.to_string())
                };
                remaining = remaining[lt_pos..].trim();
            } else {
                display_name = None;
            }
        } else {
            display_name = None;
        }

        // 2. 解析 URI
        let uri;
        if remaining.starts_with('<') {
            if let Some(gt_pos) = remaining.find('>') {
                let uri_str = &remaining[1..gt_pos];
                uri = SipUri::parse(uri_str)?;
                remaining = remaining[gt_pos + 1..].trim();
            } else {
                return Err(ParseError::InvalidHeader {
                    name: "Contact".to_string(),
                    detail: "unclosed '<' in URI".to_string(),
                });
            }
        } else {
            let uri_end = remaining.find(';').unwrap_or(remaining.len());
            uri = SipUri::parse(&remaining[..uri_end])?;
            remaining = remaining[uri_end..].trim();
        }

        // 3. 解析参数
        let mut expires = None;
        if remaining.starts_with(';') {
            remaining = remaining[1..].trim();
        }
        for param in remaining.split(';') {
            let param = param.trim();
            if param.is_empty() {
                continue;
            }
            if let Some(eq_pos) = param.find('=') {
                let key = &param[..eq_pos];
                let value = &param[eq_pos + 1..];
                if key.eq_ignore_ascii_case("expires") {
                    expires =
                        Some(
                            value
                                .parse::<u32>()
                                .map_err(|_| ParseError::InvalidHeader {
                                    name: "Contact".to_string(),
                                    detail: format!("invalid expires value: {}", value),
                                })?,
                        );
                }
            }
        }

        Ok(Self {
            display_name,
            uri,
            expires,
        })
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uri::UriScheme;

    #[test]
    fn test_contact_header_new() {
        let uri = SipUri::parse("sip:alice@example.com").unwrap();
        let contact = ContactHeader::new(uri);
        assert!(contact.display_name.is_none());
        assert!(contact.expires.is_none());
    }

    #[test]
    fn test_contact_header_display() {
        let uri = SipUri::parse("sip:alice@192.168.1.1:5060").unwrap();
        let contact = ContactHeader::new(uri).with_expires(3600);
        let s = contact.to_string();
        assert!(s.contains("<sip:alice@192.168.1.1:5060>"));
        assert!(s.contains(";expires=3600"));
    }

    #[test]
    fn test_contact_header_parse() {
        let s = "<sip:alice@example.com>;expires=3600";
        let contact: ContactHeader = s.parse().unwrap();
        assert_eq!(contact.uri.scheme, UriScheme::Sip);
        assert_eq!(contact.expires, Some(3600));
    }

    #[test]
    fn test_contact_header_parse_with_name() {
        let s = "Alice <sip:alice@example.com>;expires=600";
        let contact: ContactHeader = s.parse().unwrap();
        assert_eq!(contact.display_name.as_ref().unwrap(), "Alice");
        assert_eq!(contact.expires, Some(600));
    }

    #[test]
    fn test_contact_header_roundtrip() {
        let uri = SipUri::parse("sip:alice@example.com").unwrap();
        let contact = ContactHeader::new(uri)
            .with_display_name("Alice")
            .with_expires(3600);
        let s = contact.to_string();
        let contact2: ContactHeader = s.parse().unwrap();
        assert_eq!(contact.display_name, contact2.display_name);
        assert_eq!(contact.uri, contact2.uri);
        assert_eq!(contact.expires, contact2.expires);
    }
}
