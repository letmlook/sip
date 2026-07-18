//! Authorization 相关头部类型定义与解析
//!
//! 包括 Authorization、Proxy-Authorization、WWW-Authenticate、Proxy-Authenticate 头部。
//! 支持 Digest 认证方案（RFC 2617）。

use std::fmt;
use std::str::FromStr;

use sip_core::ParseError;

// ============================================================================
// AuthHeader - Authorization 相关头部
// ============================================================================

/// Authorization 相关头部
///
/// 支持 Digest 认证方案的参数，用于 Authorization、Proxy-Authorization、
/// WWW-Authenticate、Proxy-Authenticate 头部。
///
/// 格式：
/// ```text
/// Authorization: Digest username="alice",realm="example.com",nonce="xyz",uri="sip:example.com",response="abc",algorithm=MD5
/// WWW-Authenticate: Digest realm="example.com",nonce="xyz",algorithm=MD5
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthHeader {
    /// 认证方案（如 "Digest"）
    pub scheme: String,
    /// realm 参数
    pub realm: Option<String>,
    /// nonce 参数
    pub nonce: Option<String>,
    /// uri 参数
    pub uri: Option<String>,
    /// response 参数
    pub response: Option<String>,
    /// algorithm 参数
    pub algorithm: Option<String>,
    /// cnonce 参数
    pub cnonce: Option<String>,
    /// nc（nonce count）参数
    pub nc: Option<u32>,
    /// qop（quality of protection）参数
    pub qop: Option<String>,
    /// opaque 参数
    pub opaque: Option<String>,
    /// username 参数
    pub username: Option<String>,
}

impl AuthHeader {
    /// 创建新的 Auth 头部
    pub fn new(scheme: impl Into<String>) -> Self {
        Self {
            scheme: scheme.into(),
            realm: None,
            nonce: None,
            uri: None,
            response: None,
            algorithm: None,
            cnonce: None,
            nc: None,
            qop: None,
            opaque: None,
            username: None,
        }
    }

    /// 创建 Digest 认证头部
    pub fn digest() -> Self {
        Self::new("Digest")
    }

    /// 设置 realm
    pub fn with_realm(mut self, realm: impl Into<String>) -> Self {
        self.realm = Some(realm.into());
        self
    }

    /// 设置 nonce
    pub fn with_nonce(mut self, nonce: impl Into<String>) -> Self {
        self.nonce = Some(nonce.into());
        self
    }

    /// 设置 username
    pub fn with_username(mut self, username: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self
    }

    /// 设置 uri
    pub fn with_uri(mut self, uri: impl Into<String>) -> Self {
        self.uri = Some(uri.into());
        self
    }

    /// 设置 response
    pub fn with_response(mut self, response: impl Into<String>) -> Self {
        self.response = Some(response.into());
        self
    }

    /// 设置 algorithm
    pub fn with_algorithm(mut self, algorithm: impl Into<String>) -> Self {
        self.algorithm = Some(algorithm.into());
        self
    }

    /// 设置 cnonce
    pub fn with_cnonce(mut self, cnonce: impl Into<String>) -> Self {
        self.cnonce = Some(cnonce.into());
        self
    }

    /// 设置 nc
    pub fn with_nc(mut self, nc: u32) -> Self {
        self.nc = Some(nc);
        self
    }

    /// 设置 qop
    pub fn with_qop(mut self, qop: impl Into<String>) -> Self {
        self.qop = Some(qop.into());
        self
    }

    /// 设置 opaque
    pub fn with_opaque(mut self, opaque: impl Into<String>) -> Self {
        self.opaque = Some(opaque.into());
        self
    }
}

impl fmt::Display for AuthHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.scheme)?;
        let mut first = true;

        let write_param = |f: &mut fmt::Formatter<'_>,
                           key: &str,
                           value: &str,
                           is_first: &mut bool|
         -> fmt::Result {
            if *is_first {
                write!(f, " ")?;
                *is_first = false;
            } else {
                write!(f, ", ")?;
            }
            // 带引号的参数
            write!(f, "{}=\"{}\"", key, value)
        };

        let write_param_unquoted = |f: &mut fmt::Formatter<'_>,
                                    key: &str,
                                    value: &str,
                                    is_first: &mut bool|
         -> fmt::Result {
            if *is_first {
                write!(f, " ")?;
                *is_first = false;
            } else {
                write!(f, ", ")?;
            }
            write!(f, "{}={}", key, value)
        };

        if let Some(ref v) = self.username {
            write_param(f, "username", v, &mut first)?;
        }
        if let Some(ref v) = self.realm {
            write_param(f, "realm", v, &mut first)?;
        }
        if let Some(ref v) = self.nonce {
            write_param(f, "nonce", v, &mut first)?;
        }
        if let Some(ref v) = self.uri {
            write_param(f, "uri", v, &mut first)?;
        }
        if let Some(ref v) = self.response {
            write_param(f, "response", v, &mut first)?;
        }
        if let Some(ref v) = self.algorithm {
            write_param_unquoted(f, "algorithm", v, &mut first)?;
        }
        if let Some(ref v) = self.cnonce {
            write_param(f, "cnonce", v, &mut first)?;
        }
        if let Some(v) = self.nc {
            write_param_unquoted(f, "nc", &format!("{:08x}", v), &mut first)?;
        }
        if let Some(ref v) = self.qop {
            write_param_unquoted(f, "qop", v, &mut first)?;
        }
        if let Some(ref v) = self.opaque {
            write_param(f, "opaque", v, &mut first)?;
        }

        Ok(())
    }
}

impl FromStr for AuthHeader {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();

        // 提取 scheme（第一个空格之前的部分）
        let space_pos = s.find(' ').ok_or_else(|| ParseError::InvalidHeader {
            name: "Auth".to_string(),
            detail: "missing space after scheme".to_string(),
        })?;
        let scheme = s[..space_pos].to_string();
        let rest = s[space_pos + 1..].trim();

        let mut auth = AuthHeader::new(scheme);

        // 解析参数（逗号或空格分隔的 key=value 或 key="value" 对）
        let mut remaining = rest;
        while !remaining.is_empty() {
            remaining = remaining.trim_start();
            if remaining.is_empty() {
                break;
            }

            // 查找 key
            let eq_pos = remaining
                .find('=')
                .ok_or_else(|| ParseError::InvalidHeader {
                    name: "Auth".to_string(),
                    detail: format!("missing '=' in auth parameter: {}", remaining),
                })?;
            let key = remaining[..eq_pos].trim().to_string();
            remaining = remaining[eq_pos + 1..].trim_start();

            let value;
            if remaining.starts_with('"') {
                // 带引号的值
                if let Some(end_quote) = remaining[1..].find('"') {
                    value = remaining[1..end_quote + 1].to_string();
                    remaining = remaining[end_quote + 2..].trim_start();
                } else {
                    return Err(ParseError::InvalidHeader {
                        name: "Auth".to_string(),
                        detail: "unclosed quote in auth parameter value".to_string(),
                    });
                }
            } else {
                // 不带引号的值，到逗号或末尾
                let end_pos = remaining.find(',').unwrap_or(remaining.len());
                value = remaining[..end_pos].trim().to_string();
                remaining = remaining[end_pos..].trim_start();
            }

            // 跳过逗号
            if remaining.starts_with(',') {
                remaining = remaining[1..].trim_start();
            }

            // 映射参数到字段
            match key.to_lowercase().as_str() {
                "username" => auth.username = Some(value),
                "realm" => auth.realm = Some(value),
                "nonce" => auth.nonce = Some(value),
                "uri" => auth.uri = Some(value),
                "response" => auth.response = Some(value),
                "algorithm" => auth.algorithm = Some(value),
                "cnonce" => auth.cnonce = Some(value),
                "nc" => {
                    auth.nc = Some(u32::from_str_radix(&value, 16).map_err(|_| {
                        ParseError::InvalidHeader {
                            name: "Auth".to_string(),
                            detail: format!("invalid nc value: {}", value),
                        }
                    })?)
                }
                "qop" => auth.qop = Some(value),
                "opaque" => auth.opaque = Some(value),
                _ => {} // 忽略未知参数
            }
        }

        Ok(auth)
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_header_digest() {
        let auth = AuthHeader::digest()
            .with_username("alice")
            .with_realm("example.com")
            .with_nonce("xyz123")
            .with_uri("sip:example.com")
            .with_response("abc456")
            .with_algorithm("MD5");

        let s = auth.to_string();
        assert!(s.starts_with("Digest"));
        assert!(s.contains("username=\"alice\""));
        assert!(s.contains("realm=\"example.com\""));
        assert!(s.contains("nonce=\"xyz123\""));
        assert!(s.contains("algorithm=MD5"));
    }

    #[test]
    fn test_auth_header_parse() {
        let s = r#"Digest username="alice",realm="example.com",nonce="xyz",uri="sip:example.com",response="abc",algorithm=MD5"#;
        let auth: AuthHeader = s.parse().unwrap();
        assert_eq!(auth.scheme, "Digest");
        assert_eq!(auth.username.as_ref().unwrap(), "alice");
        assert_eq!(auth.realm.as_ref().unwrap(), "example.com");
        assert_eq!(auth.nonce.as_ref().unwrap(), "xyz");
        assert_eq!(auth.uri.as_ref().unwrap(), "sip:example.com");
        assert_eq!(auth.response.as_ref().unwrap(), "abc");
        assert_eq!(auth.algorithm.as_ref().unwrap(), "MD5");
    }

    #[test]
    fn test_auth_header_parse_with_nc() {
        let s = r#"Digest username="alice",realm="example.com",nonce="xyz",nc=00000001,qop=auth"#;
        let auth: AuthHeader = s.parse().unwrap();
        assert_eq!(auth.nc, Some(1));
        assert_eq!(auth.qop.as_ref().unwrap(), "auth");
    }

    #[test]
    fn test_auth_header_roundtrip() {
        let auth = AuthHeader::digest()
            .with_username("alice")
            .with_realm("example.com")
            .with_nonce("xyz123")
            .with_algorithm("MD5");
        let s = auth.to_string();
        let auth2: AuthHeader = s.parse().unwrap();
        assert_eq!(auth.scheme, auth2.scheme);
        assert_eq!(auth.username, auth2.username);
        assert_eq!(auth.realm, auth2.realm);
        assert_eq!(auth.nonce, auth2.nonce);
        assert_eq!(auth.algorithm, auth2.algorithm);
    }

    #[test]
    fn test_auth_header_www_authenticate() {
        let s = r#"Digest realm="example.com",nonce="xyz",algorithm=MD5"#;
        let auth: AuthHeader = s.parse().unwrap();
        assert_eq!(auth.scheme, "Digest");
        assert_eq!(auth.realm.as_ref().unwrap(), "example.com");
        assert!(auth.username.is_none());
    }
}
