//! SIP 摘要认证处理（RFC 2617）
//!
//! 提供 `AuthHandler` trait 和 `DigestAuthHandler` 实现，
//! 用于处理 SIP 401/407 响应中的摘要认证挑战。
//!
//! # 摘要认证流程
//!
//! 1. UAC 发送 REGISTER 请求
//! 2. 服务器返回 401/407 响应，包含 WWW-Authenticate/Proxy-Authenticate 头部
//! 3. UAC 根据 challenge 参数计算摘要响应
//! 4. UAC 重新发送 REGISTER 请求，包含 Authorization/Proxy-Authorization 头部
//!
//! # MD5 摘要计算
//!
//! ```text
//! HA1 = MD5(username:realm:password)
//! HA2 = MD5(method:uri)
//! response = MD5(HA1:nonce:nc:cnonce:qop:HA2)  // 有 qop 时
//! response = MD5(HA1:nonce:HA2)                  // 无 qop 时
//! ```

use sip_core::RegistrationError;
use sip_message::AuthHeader;

// ============================================================================
// AuthHandler - 认证处理器 trait
// ============================================================================

/// 认证处理器 trait
///
/// 定义摘要认证的响应计算接口，支持自定义实现用于测试或扩展。
pub trait AuthHandler: Send + Sync {
    /// 计算摘要认证响应
    ///
    /// # 参数
    ///
    /// - `username` - 用户名
    /// - `password` - 密码
    /// - `realm` - 认证域
    /// - `nonce` - 服务器 nonce
    /// - `uri` - 请求 URI（Digest URI）
    /// - `method` - SIP 方法
    /// - `cnonce` - 客户端 nonce（qop 存在时必需）
    /// - `nc` - nonce 计数
    /// - `qop` - 保护质量（"auth" 或 "auth-int"）
    /// - `algorithm` - 算法（如 "MD5"）
    /// - `opaque` - 服务器透传数据
    ///
    /// # 返回
    ///
    /// 返回计算出的摘要响应字符串，或认证错误。
    #[allow(clippy::too_many_arguments)]
    fn compute_response(
        &self,
        username: &str,
        password: &str,
        realm: &str,
        nonce: &str,
        uri: &str,
        method: &str,
        cnonce: Option<&str>,
        nc: u32,
        qop: Option<&str>,
        algorithm: Option<&str>,
        opaque: Option<&str>,
    ) -> Result<String, RegistrationError>;
}

// ============================================================================
// DigestAuthHandler - MD5 摘要认证处理器
// ============================================================================

/// MD5 摘要认证处理器
///
/// 实现符合 RFC 2617 的 MD5 摘要认证算法。
pub struct DigestAuthHandler;

impl DigestAuthHandler {
    /// 创建新的 MD5 摘要认证处理器
    pub fn new() -> Self {
        Self
    }

    /// 计算 MD5 哈希并返回十六进制字符串
    fn md5_hex(data: &str) -> String {
        use md5::{Digest, Md5};
        let mut hasher = Md5::new();
        hasher.update(data.as_bytes());
        let result = hasher.finalize();
        format!("{:x}", result)
    }
}

impl Default for DigestAuthHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthHandler for DigestAuthHandler {
    #[allow(clippy::too_many_arguments)]
    fn compute_response(
        &self,
        username: &str,
        password: &str,
        realm: &str,
        nonce: &str,
        uri: &str,
        method: &str,
        cnonce: Option<&str>,
        nc: u32,
        qop: Option<&str>,
        algorithm: Option<&str>,
        _opaque: Option<&str>,
    ) -> Result<String, RegistrationError> {
        // 检查算法支持（仅支持 MD5 和未指定算法）
        if let Some(alg) = algorithm {
            if !alg.eq_ignore_ascii_case("MD5") {
                return Err(RegistrationError::AuthenticationFailed {
                    reason: format!("unsupported digest algorithm: {}", alg),
                });
            }
        }

        // HA1 = MD5(username:realm:password)
        let ha1 = Self::md5_hex(&format!("{}:{}:{}", username, realm, password));

        // HA2 = MD5(method:uri)
        let ha2 = Self::md5_hex(&format!("{}:{}", method, uri));

        // 计算响应
        let response = if let Some(qop_val) = qop {
            // 有 qop 时：MD5(HA1:nonce:nc:cnonce:qop:HA2)
            let cnonce_val = cnonce.ok_or_else(|| RegistrationError::AuthenticationFailed {
                reason: "cnonce is required when qop is specified".to_string(),
            })?;
            let nc_str = format!("{:08x}", nc);
            Self::md5_hex(&format!(
                "{}:{}:{}:{}:{}:{}",
                ha1, nonce, nc_str, cnonce_val, qop_val, ha2
            ))
        } else {
            // 无 qop 时：MD5(HA1:nonce:HA2)
            Self::md5_hex(&format!("{}:{}:{}", ha1, nonce, ha2))
        };

        Ok(response)
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 从 401/407 响应构建 Authorization/Proxy-Authorization 头部
///
/// # 参数
///
/// - `challenge` - WWW-Authenticate 或 Proxy-Authenticate 头部值
/// - `username` - 用户名
/// - `password` - 密码
/// - `uri` - 请求 URI
/// - `method` - SIP 方法
/// - `nc` - nonce 计数
/// - `auth_handler` - 认证处理器
///
/// # 返回
///
/// 返回构建好的 Authorization 头部，或认证错误。
pub fn build_auth_header(
    challenge: &AuthHeader,
    username: &str,
    password: &str,
    uri: &str,
    method: &str,
    nc: u32,
    auth_handler: &dyn AuthHandler,
) -> Result<AuthHeader, RegistrationError> {
    let realm =
        challenge
            .realm
            .as_deref()
            .ok_or_else(|| RegistrationError::AuthenticationFailed {
                reason: "missing realm in challenge".to_string(),
            })?;

    let nonce =
        challenge
            .nonce
            .as_deref()
            .ok_or_else(|| RegistrationError::AuthenticationFailed {
                reason: "missing nonce in challenge".to_string(),
            })?;

    // 生成客户端 nonce（如果 qop 存在）
    let cnonce = if challenge.qop.is_some() {
        Some(uuid::Uuid::new_v4().simple().to_string())
    } else {
        None
    };

    let response = auth_handler.compute_response(
        username,
        password,
        realm,
        nonce,
        uri,
        method,
        cnonce.as_deref(),
        nc,
        challenge.qop.as_deref(),
        challenge.algorithm.as_deref(),
        challenge.opaque.as_deref(),
    )?;

    // 构建 Authorization 头部
    let mut auth = AuthHeader::digest()
        .with_username(username)
        .with_realm(realm)
        .with_nonce(nonce)
        .with_uri(uri)
        .with_response(&response);

    if let Some(ref algorithm) = challenge.algorithm {
        auth = auth.with_algorithm(algorithm);
    }

    if let Some(ref qop) = challenge.qop {
        auth = auth.with_qop(qop);
    }

    if let Some(cnonce) = cnonce {
        auth = auth.with_cnonce(cnonce);
    }

    if nc > 0 {
        auth = auth.with_nc(nc);
    }

    if let Some(ref opaque) = challenge.opaque {
        auth = auth.with_opaque(opaque);
    }

    Ok(auth)
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_digest_auth_handler_basic() {
        let handler = DigestAuthHandler::new();

        // 基于 RFC 2617 Section 3.5 示例参数
        // HA1 = MD5("Mufasa:testrealm@host.com:Circle Of Life") = 939e7578ed9e3c518a452acee763bce9
        // HA2 = MD5("GET:/dir/index.html") = 39aff3a2bab6126f332b942af96d3366
        // response = MD5("939e7578ed9e3c518a452acee763bce9:dcd98b7102dd2f0e8b11d0f600bfb0c093:39aff3a2bab6126f332b942af96d3366")
        let response = handler
            .compute_response(
                "Mufasa",
                "Circle Of Life",
                "testrealm@host.com",
                "dcd98b7102dd2f0e8b11d0f600bfb0c093",
                "/dir/index.html",
                "GET",
                None,
                0,
                None,
                Some("MD5"),
                None,
            )
            .unwrap();

        // 使用实际 MD5 计算验证
        assert_eq!(response, "670fd8c2df070c60b045671b8b24ff02");
    }

    #[test]
    fn test_digest_auth_handler_with_qop() {
        let handler = DigestAuthHandler::new();

        let response = handler
            .compute_response(
                "Mufasa",
                "Circle Of Life",
                "testrealm@host.com",
                "dcd98b7102dd2f0e8b11d0f600bfb0c093",
                "/dir/index.html",
                "GET",
                Some("0a4f113b"),
                1,
                Some("auth"),
                Some("MD5"),
                Some("5ccc069c403ebaf9f0171e9517f40e41"),
            )
            .unwrap();

        // 预期结果基于 RFC 2617 Section 3.5 示例（带 qop=auth）
        assert_eq!(response, "6629fae49393a05397450978507c4ef1");
    }

    #[test]
    fn test_digest_auth_handler_unsupported_algorithm() {
        let handler = DigestAuthHandler::new();

        let result = handler.compute_response(
            "user",
            "pass",
            "realm",
            "nonce",
            "sip:example.com",
            "REGISTER",
            None,
            0,
            None,
            Some("SHA-256"),
            None,
        );

        assert!(result.is_err());
        if let Err(RegistrationError::AuthenticationFailed { reason }) = result {
            assert!(reason.contains("unsupported"));
        } else {
            panic!("Expected AuthenticationFailed error");
        }
    }

    #[test]
    fn test_digest_auth_handler_qop_without_cnonce() {
        let handler = DigestAuthHandler::new();

        let result = handler.compute_response(
            "user",
            "pass",
            "realm",
            "nonce",
            "sip:example.com",
            "REGISTER",
            None, // 缺少 cnonce
            1,
            Some("auth"),
            Some("MD5"),
            None,
        );

        assert!(result.is_err());
        if let Err(RegistrationError::AuthenticationFailed { reason }) = result {
            assert!(reason.contains("cnonce"));
        } else {
            panic!("Expected AuthenticationFailed error");
        }
    }

    #[test]
    fn test_digest_auth_handler_register_method() {
        let handler = DigestAuthHandler::new();

        let response = handler
            .compute_response(
                "alice",
                "secret",
                "example.com",
                "testnonce",
                "sip:example.com",
                "REGISTER",
                None,
                0,
                None,
                None,
                None,
            )
            .unwrap();

        // 验证返回了非空响应
        assert!(!response.is_empty());
        assert_eq!(response.len(), 32); // MD5 哈希长度为 32 个十六进制字符
    }

    #[test]
    fn test_md5_hex() {
        // 验证 MD5("test") 的结果
        let result = DigestAuthHandler::md5_hex("test");
        assert_eq!(result, "098f6bcd4621d373cade4e832627b4f6");
    }

    #[test]
    fn test_build_auth_header() {
        let handler = DigestAuthHandler::new();

        let challenge = AuthHeader::digest()
            .with_realm("example.com")
            .with_nonce("testnonce")
            .with_algorithm("MD5");

        let auth = build_auth_header(
            &challenge,
            "alice",
            "secret",
            "sip:example.com",
            "REGISTER",
            1,
            &handler,
        )
        .unwrap();

        assert_eq!(auth.scheme, "Digest");
        assert_eq!(auth.username.as_deref(), Some("alice"));
        assert_eq!(auth.realm.as_deref(), Some("example.com"));
        assert_eq!(auth.nonce.as_deref(), Some("testnonce"));
        assert_eq!(auth.uri.as_deref(), Some("sip:example.com"));
        assert!(auth.response.is_some());
        assert_eq!(auth.algorithm.as_deref(), Some("MD5"));
    }

    #[test]
    fn test_build_auth_header_with_qop() {
        let handler = DigestAuthHandler::new();

        let challenge = AuthHeader::digest()
            .with_realm("example.com")
            .with_nonce("testnonce")
            .with_algorithm("MD5")
            .with_qop("auth")
            .with_opaque("opaqueval");

        let auth = build_auth_header(
            &challenge,
            "alice",
            "secret",
            "sip:example.com",
            "REGISTER",
            1,
            &handler,
        )
        .unwrap();

        assert!(auth.cnonce.is_some());
        assert_eq!(auth.nc, Some(1));
        assert_eq!(auth.qop.as_deref(), Some("auth"));
        assert_eq!(auth.opaque.as_deref(), Some("opaqueval"));
    }

    #[test]
    fn test_build_auth_header_missing_realm() {
        let handler = DigestAuthHandler::new();

        let challenge = AuthHeader::digest().with_nonce("testnonce");

        let result = build_auth_header(
            &challenge,
            "alice",
            "secret",
            "sip:example.com",
            "REGISTER",
            1,
            &handler,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_build_auth_header_missing_nonce() {
        let handler = DigestAuthHandler::new();

        let challenge = AuthHeader::digest().with_realm("example.com");

        let result = build_auth_header(
            &challenge,
            "alice",
            "secret",
            "sip:example.com",
            "REGISTER",
            1,
            &handler,
        );

        assert!(result.is_err());
    }
}
