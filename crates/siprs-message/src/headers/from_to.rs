//! From/To 头部类型定义与解析
//!
//! From 和 To 头部共享相同的数据结构，包含显示名称、URI 和 tag 参数。
//! From 头部由 UAC 设置（含本地 Tag），To 头部由 UAS 设置（含远端 Tag）。

use std::fmt;
use std::str::FromStr;

use siprs_core::ParseError;

use crate::types::Tag;
use crate::uri::SipUri;

// ============================================================================
// FromToHeader - From/To 头部
// ============================================================================

/// From/To 头部
///
/// From 和 To 头部共享相同的数据结构：
/// ```text
/// Display-Name <sip:uri> ;tag=xxx
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FromToHeader {
    /// 显示名称（可选）
    pub display_name: Option<String>,
    /// SIP URI
    pub uri: SipUri,
    /// tag 参数（可选）
    pub tag: Option<Tag>,
}

impl FromToHeader {
    /// 创建新的 From/To 头部
    pub fn new(uri: SipUri) -> Self {
        Self {
            display_name: None,
            uri,
            tag: None,
        }
    }

    /// 创建带自动生成 Tag 的 From/To 头部
    pub fn with_generated_tag(uri: SipUri) -> Self {
        Self {
            display_name: None,
            uri,
            tag: Some(Tag::new()),
        }
    }

    /// 设置显示名称
    pub fn with_display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    /// 设置 tag
    pub fn with_tag(mut self, tag: Tag) -> Self {
        self.tag = Some(tag);
        self
    }
}

impl fmt::Display for FromToHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref name) = self.display_name {
            // 如果显示名称包含特殊字符，用引号包裹
            if name.contains(' ') || name.contains(',') || name.contains(';') {
                write!(f, "\"{}\" ", name)?;
            } else {
                write!(f, "{} ", name)?;
            }
        }
        write!(f, "<{}>", self.uri)?;
        if let Some(ref tag) = self.tag {
            write!(f, ";tag={}", tag)?;
        }
        Ok(())
    }
}

impl FromStr for FromToHeader {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let mut remaining = s;

        // 1. 解析显示名称（如果存在）
        let display_name;
        if remaining.starts_with('"') {
            // 带引号的显示名称
            if let Some(end_quote) = remaining[1..].find('"') {
                display_name = Some(remaining[1..end_quote + 1].to_string());
                remaining = remaining[end_quote + 2..].trim();
            } else {
                return Err(ParseError::InvalidHeader {
                    name: "From/To".to_string(),
                    detail: "unclosed quote in display name".to_string(),
                });
            }
        } else if !remaining.starts_with('<') {
            // 不带引号的显示名称（到 < 之前的文本）
            if let Some(lt_pos) = remaining.find('<') {
                let name = remaining[..lt_pos].trim();
                display_name = if name.is_empty() {
                    None
                } else {
                    Some(name.to_string())
                };
                remaining = remaining[lt_pos..].trim();
            } else {
                // 没有 < 和显示名称，尝试直接解析 URI
                display_name = None;
            }
        } else {
            display_name = None;
        }

        // 2. 解析 URI（尖括号内）
        let uri;
        if remaining.starts_with('<') {
            if let Some(gt_pos) = remaining.find('>') {
                let uri_str = &remaining[1..gt_pos];
                uri = SipUri::parse(uri_str)?;
                remaining = remaining[gt_pos + 1..].trim();
            } else {
                return Err(ParseError::InvalidHeader {
                    name: "From/To".to_string(),
                    detail: "unclosed '<' in URI".to_string(),
                });
            }
        } else {
            // 没有尖括号，尝试解析到分号或末尾
            let uri_end = remaining.find(';').unwrap_or(remaining.len());
            uri = SipUri::parse(&remaining[..uri_end])?;
            remaining = remaining[uri_end..].trim();
        }

        // 3. 解析参数（如 tag）
        let mut tag = None;
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
                if key.eq_ignore_ascii_case("tag") {
                    tag = Some(Tag(value.to_string()));
                }
                // 忽略其他参数
            }
        }

        Ok(Self {
            display_name,
            uri,
            tag,
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
    fn test_from_to_header_new() {
        let uri = SipUri::parse("sip:alice@example.com").unwrap();
        let header = FromToHeader::new(uri.clone());
        assert!(header.display_name.is_none());
        assert_eq!(header.uri, uri);
        assert!(header.tag.is_none());
    }

    #[test]
    fn test_from_to_header_with_generated_tag() {
        let uri = SipUri::parse("sip:alice@example.com").unwrap();
        let header = FromToHeader::with_generated_tag(uri);
        assert!(header.tag.is_some());
    }

    #[test]
    fn test_from_to_header_display() {
        let uri = SipUri::parse("sip:alice@example.com").unwrap();
        let header = FromToHeader::new(uri).with_tag(Tag("abc123".to_string()));
        let s = header.to_string();
        assert!(s.contains("<sip:alice@example.com>"));
        assert!(s.contains(";tag=abc123"));
    }

    #[test]
    fn test_from_to_header_display_with_name() {
        let uri = SipUri::parse("sip:alice@example.com").unwrap();
        let header = FromToHeader::new(uri)
            .with_display_name("Alice Smith")
            .with_tag(Tag("abc".to_string()));
        let s = header.to_string();
        assert!(s.contains("\"Alice Smith\""));
        assert!(s.contains("<sip:alice@example.com>"));
    }

    #[test]
    fn test_from_to_header_parse_simple() {
        let s = "<sip:alice@example.com>;tag=abc123";
        let header: FromToHeader = s.parse().unwrap();
        assert!(header.display_name.is_none());
        assert_eq!(header.uri.scheme, UriScheme::Sip);
        assert_eq!(header.tag.as_ref().unwrap().0, "abc123");
    }

    #[test]
    fn test_from_to_header_parse_with_name() {
        let s = "Alice <sip:alice@example.com>;tag=xyz";
        let header: FromToHeader = s.parse().unwrap();
        assert_eq!(header.display_name.as_ref().unwrap(), "Alice");
        assert_eq!(header.tag.as_ref().unwrap().0, "xyz");
    }

    #[test]
    fn test_from_to_header_parse_quoted_name() {
        let s = "\"Alice Smith\" <sip:alice@example.com>;tag=abc";
        let header: FromToHeader = s.parse().unwrap();
        assert_eq!(header.display_name.as_ref().unwrap(), "Alice Smith");
    }

    #[test]
    fn test_from_to_header_roundtrip() {
        let uri = SipUri::parse("sip:alice@example.com").unwrap();
        let header = FromToHeader::new(uri)
            .with_display_name("Alice")
            .with_tag(Tag("abc123".to_string()));
        let s = header.to_string();
        let header2: FromToHeader = s.parse().unwrap();
        assert_eq!(header.display_name, header2.display_name);
        assert_eq!(header.uri, header2.uri);
        assert_eq!(header.tag, header2.tag);
    }
}
