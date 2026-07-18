//! Route/Record-Route 头部类型定义与解析
//!
//! Route 头部指定消息必须经过的强制路由路径；
//! Record-Route 头部由代理服务器添加，确保后续请求仍经过该代理。

use std::fmt;
use std::str::FromStr;

use sip_core::ParseError;

use crate::uri::SipUri;

// ============================================================================
// RouteHeader - Route/Record-Route 头部
// ============================================================================

/// Route/Record-Route 头部
///
/// 格式：
/// ```text
/// Route: Display-Name <sip:uri>
/// Record-Route: <sip:proxy.example.com;lr>
/// ```
///
/// Route 和 Record-Route 共享相同的数据结构。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteHeader {
    /// 显示名称（可选）
    pub display_name: Option<String>,
    /// SIP URI
    pub uri: SipUri,
}

impl RouteHeader {
    /// 创建新的 Route 头部
    pub fn new(uri: SipUri) -> Self {
        Self {
            display_name: None,
            uri,
        }
    }

    /// 设置显示名称
    pub fn with_display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }
}

impl fmt::Display for RouteHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref name) = self.display_name {
            if name.contains(' ') || name.contains(',') || name.contains(';') {
                write!(f, "\"{}\" ", name)?;
            } else {
                write!(f, "{} ", name)?;
            }
        }
        write!(f, "<{}>", self.uri)
    }
}

impl FromStr for RouteHeader {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let mut remaining = s;

        // 1. 解析显示名称
        let display_name;
        if remaining.starts_with('"') {
            if let Some(end_quote) = remaining[1..].find('"') {
                display_name = Some(remaining[1..end_quote + 1].to_string());
                remaining = remaining[end_quote + 2..].trim();
            } else {
                return Err(ParseError::InvalidHeader {
                    name: "Route".to_string(),
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
            } else {
                return Err(ParseError::InvalidHeader {
                    name: "Route".to_string(),
                    detail: "unclosed '<' in URI".to_string(),
                });
            }
        } else {
            // 没有尖括号，直接解析到末尾
            uri = SipUri::parse(remaining)?;
        }

        Ok(Self { display_name, uri })
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
    fn test_route_header_new() {
        let uri = SipUri::parse("sip:proxy.example.com;lr").unwrap();
        let route = RouteHeader::new(uri);
        assert!(route.display_name.is_none());
    }

    #[test]
    fn test_route_header_display() {
        let uri = SipUri::parse("sip:proxy.example.com;lr").unwrap();
        let route = RouteHeader::new(uri);
        let s = route.to_string();
        assert!(s.contains("<sip:proxy.example.com;lr>"));
    }

    #[test]
    fn test_route_header_parse() {
        let s = "<sip:proxy.example.com;lr>";
        let route: RouteHeader = s.parse().unwrap();
        assert_eq!(route.uri.scheme, UriScheme::Sip);
        assert!(route.uri.lr());
    }

    #[test]
    fn test_route_header_parse_with_name() {
        let s = "Proxy <sip:proxy.example.com;lr>";
        let route: RouteHeader = s.parse().unwrap();
        assert_eq!(route.display_name.as_ref().unwrap(), "Proxy");
    }

    #[test]
    fn test_route_header_roundtrip() {
        let uri = SipUri::parse("sip:proxy.example.com;lr").unwrap();
        let route = RouteHeader::new(uri).with_display_name("MyProxy");
        let s = route.to_string();
        let route2: RouteHeader = s.parse().unwrap();
        assert_eq!(route.display_name, route2.display_name);
        assert_eq!(route.uri, route2.uri);
    }
}
