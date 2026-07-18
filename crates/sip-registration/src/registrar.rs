//! SIP 注册服务器实现（RFC 3261 Section 10）
//!
//! 提供 `RegistrationStore` trait、`MemoryRegistrationStore` 内存存储实现
//! 和 `Registrar` 注册服务器，用于处理 SIP REGISTER 请求，
//! 管理设备绑定关系和摘要认证。
//!
//! # 注册服务器流程
//!
//! 1. 接收 REGISTER 请求
//! 2. 验证请求格式（Contact、Call-ID、CSeq 头部）
//! 3. 如果启用认证且请求不含 Authorization → 返回 401 挑战
//! 4. 验证摘要认证凭据
//! 5. 根据 Expires 值保存或删除绑定
//! 6. 返回 200 OK（含 Contact 头部和 Expires 参数）
//!
//! # 示例
//!
//! ```ignore
//! use sip_registration::registrar::{Registrar, MemoryRegistrationStore};
//! use sip_registration::auth::DigestAuthHandler;
//! use std::sync::Arc;
//!
//! let store = Arc::new(MemoryRegistrationStore::new());
//! let auth_handler = Arc::new(DigestAuthHandler::new());
//!
//! let registrar = Registrar::new(store, auth_handler, "example.com".to_string(), true);
//!
//! // 处理 REGISTER 请求
//! let response = registrar.handle_register(&request, None).await;
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

type CredentialLookup = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

use async_trait::async_trait;
use sip_core::{RegistrationError, SipVersion, StatusCode};
use sip_message::{
    AuthHeader, ContactHeader, FromToHeader, HeaderCollection, HeaderName, HeaderValue, Method,
    SipRequest, SipResponse, StatusLine,
};
use tokio::sync::Mutex;

use crate::auth::AuthHandler;

// ============================================================================
// BindingInfo - 绑定信息
// ============================================================================

/// 绑定信息
///
/// 保存一次 SIP 注册绑定的完整上下文信息，包括 AOR、联系地址、
/// 有效期、注册时间、Call-ID 和 CSeq 序列号。
#[derive(Debug, Clone)]
pub struct BindingInfo {
    /// 地址记录（Address-of-Record），格式 sip:user@domain
    pub aor: String,
    /// 联系地址 URI
    pub contact: String,
    /// 注册有效期（秒）
    pub expires: u64,
    /// 注册成功时间
    pub registered_at: Instant,
    /// Call-ID，同一 AOR 的所有注册使用相同的 Call-ID
    pub call_id: String,
    /// CSeq 序列号
    pub cseq: u32,
}

impl BindingInfo {
    /// 计算绑定剩余有效时间（秒）
    ///
    /// 如果已过期则返回 0。
    pub fn remaining_expires(&self) -> u64 {
        let elapsed = self.registered_at.elapsed().as_secs();
        self.expires.saturating_sub(elapsed)
    }

    /// 判断绑定是否已过期
    pub fn is_expired(&self) -> bool {
        self.remaining_expires() == 0
    }
}

// ============================================================================
// RegistrationStore - 注册信息存储 trait
// ============================================================================

/// 注册信息存储 trait
///
/// 定义注册绑定的持久化接口，支持自定义实现（如 Redis、数据库等）。
/// 默认提供 `MemoryRegistrationStore` 内存实现。
#[async_trait]
pub trait RegistrationStore: Send + Sync {
    /// 保存绑定
    ///
    /// 如果 AOR 已存在，则更新绑定信息。
    async fn save_binding(
        &self,
        aor: &str,
        contact: &str,
        expires: u64,
    ) -> Result<(), RegistrationError>;

    /// 保存绑定（含完整上下文信息）
    ///
    /// 用于内部实现，保存 Call-ID 和 CSeq 等上下文。
    async fn save_binding_full(
        &self,
        aor: &str,
        contact: &str,
        expires: u64,
        call_id: &str,
        cseq: u32,
    ) -> Result<(), RegistrationError>;

    /// 查询绑定
    ///
    /// 根据 AOR 查询绑定信息，如果绑定已过期则返回 None。
    async fn query_binding(&self, aor: &str) -> Result<Option<BindingInfo>, RegistrationError>;

    /// 删除绑定
    async fn remove_binding(&self, aor: &str) -> Result<(), RegistrationError>;

    /// 列出所有绑定（包括已过期的）
    async fn list_bindings(&self) -> Result<Vec<BindingInfo>, RegistrationError>;

    /// 列出所有有效绑定（排除已过期的）
    async fn list_active_bindings(&self) -> Result<Vec<BindingInfo>, RegistrationError>;

    /// 清理过期绑定
    ///
    /// 删除所有已过期的绑定，返回被清理的绑定数量。
    async fn cleanup_expired(&self) -> Result<usize, RegistrationError>;
}

// ============================================================================
// MemoryRegistrationStore - 内存存储实现
// ============================================================================

/// 内存存储实现
///
/// 使用 `HashMap` 存储绑定信息，适用于单进程场景和测试。
/// 生产环境可替换为 Redis 或数据库实现。
pub struct MemoryRegistrationStore {
    bindings: Mutex<HashMap<String, BindingInfo>>,
}

impl MemoryRegistrationStore {
    /// 创建新的内存存储
    pub fn new() -> Self {
        Self {
            bindings: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for MemoryRegistrationStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RegistrationStore for MemoryRegistrationStore {
    async fn save_binding(
        &self,
        aor: &str,
        contact: &str,
        expires: u64,
    ) -> Result<(), RegistrationError> {
        let mut bindings = self.bindings.lock().await;

        // 如果已存在绑定，保留原有的 call_id 和 cseq
        let (call_id, cseq) = bindings
            .get(aor)
            .map(|b| (b.call_id.clone(), b.cseq))
            .unwrap_or_else(|| (String::new(), 0));

        let info = BindingInfo {
            aor: aor.to_string(),
            contact: contact.to_string(),
            expires,
            registered_at: Instant::now(),
            call_id,
            cseq,
        };

        bindings.insert(aor.to_string(), info);
        Ok(())
    }

    async fn save_binding_full(
        &self,
        aor: &str,
        contact: &str,
        expires: u64,
        call_id: &str,
        cseq: u32,
    ) -> Result<(), RegistrationError> {
        let mut bindings = self.bindings.lock().await;

        let info = BindingInfo {
            aor: aor.to_string(),
            contact: contact.to_string(),
            expires,
            registered_at: Instant::now(),
            call_id: call_id.to_string(),
            cseq,
        };

        bindings.insert(aor.to_string(), info);
        Ok(())
    }

    async fn query_binding(&self, aor: &str) -> Result<Option<BindingInfo>, RegistrationError> {
        let bindings = self.bindings.lock().await;
        match bindings.get(aor) {
            Some(info) if !info.is_expired() => Ok(Some(info.clone())),
            _ => Ok(None),
        }
    }

    async fn remove_binding(&self, aor: &str) -> Result<(), RegistrationError> {
        let mut bindings = self.bindings.lock().await;
        bindings.remove(aor);
        Ok(())
    }

    async fn list_bindings(&self) -> Result<Vec<BindingInfo>, RegistrationError> {
        let bindings = self.bindings.lock().await;
        Ok(bindings.values().cloned().collect())
    }

    async fn list_active_bindings(&self) -> Result<Vec<BindingInfo>, RegistrationError> {
        let bindings = self.bindings.lock().await;
        Ok(bindings
            .values()
            .filter(|b| !b.is_expired())
            .cloned()
            .collect())
    }

    async fn cleanup_expired(&self) -> Result<usize, RegistrationError> {
        let mut bindings = self.bindings.lock().await;
        let before = bindings.len();
        bindings.retain(|_, info| !info.is_expired());
        let removed = before - bindings.len();
        Ok(removed)
    }
}

// ============================================================================
// Registrar - SIP 注册服务器
// ============================================================================

/// SIP 注册服务器
///
/// 处理 SIP REGISTER 请求，管理设备绑定关系和摘要认证。
/// 支持可插拔的存储后端和认证处理器。
///
/// # 功能
///
/// - 处理 REGISTER 请求（注册、注销、刷新）
/// - 摘要认证验证（RFC 2617）
/// - 绑定查询和列表
/// - 过期绑定清理
pub struct Registrar {
    /// 绑定存储后端
    store: Arc<dyn RegistrationStore>,
    /// 认证处理器
    auth_handler: Arc<dyn AuthHandler>,
    /// 认证域
    realm: String,
    /// 是否要求认证
    require_auth: bool,
    /// 用户凭据查找函数（username → password）
    credential_lookup: Option<CredentialLookup>,
}

impl Registrar {
    /// 创建新的注册服务器
    ///
    /// # 参数
    ///
    /// - `store` - 绑定存储后端
    /// - `auth_handler` - 认证处理器
    /// - `realm` - 认证域
    /// - `require_auth` - 是否要求认证
    pub fn new(
        store: Arc<dyn RegistrationStore>,
        auth_handler: Arc<dyn AuthHandler>,
        realm: String,
        require_auth: bool,
    ) -> Self {
        Self {
            store,
            auth_handler,
            realm,
            require_auth,
            credential_lookup: None,
        }
    }

    /// 设置用户凭据查找函数
    ///
    /// # 参数
    ///
    /// - `lookup` - 凭据查找函数，接受用户名，返回密码
    pub fn with_credential_lookup(mut self, lookup: CredentialLookup) -> Self {
        self.credential_lookup = Some(lookup);
        self
    }

    /// 处理 REGISTER 请求
    ///
    /// 按照 RFC 3261 Section 10 处理 REGISTER 请求：
    ///
    /// 1. 验证请求格式（必须包含 Contact、Call-ID、CSeq 头部）
    /// 2. 如果需要认证且请求不含 Authorization → 返回 401 挑战
    /// 3. 验证摘要认证
    /// 4. 根据 Expires 值保存或删除绑定
    /// 5. 返回 200 OK（含 Contact 头部和 Expires 参数）
    ///
    /// # 参数
    ///
    /// - `request` - REGISTER 请求
    /// - `nonce` - 当前 nonce（用于认证挑战），如果为 None 则自动生成
    ///
    /// # 返回
    ///
    /// 返回应发送给客户端的 SIP 响应。
    pub async fn handle_register(&self, request: &SipRequest, nonce: Option<&str>) -> SipResponse {
        // 1. 验证请求方法
        if request.request_line.method != Method::Register {
            return self.build_error_response(StatusCode::METHOD_NOT_ALLOWED, "Method Not Allowed");
        }

        // 2. 验证必需头部
        let validation_result = self.validate_register_request(request);
        if let Err(response) = validation_result {
            return response;
        }

        let (contact_uri, expires, call_id, cseq) = validation_result.unwrap();

        // 3. 提取 AOR（从 To 头部）
        let aor = match self.extract_aor(request) {
            Some(aor) => aor,
            None => {
                return self.build_error_response(StatusCode::BAD_REQUEST, "Invalid To header");
            }
        };

        // 4. 认证检查
        if self.require_auth {
            match self.check_authentication(request, &aor, nonce).await {
                AuthResult::Authenticated => {
                    // 认证通过，继续处理
                }
                AuthResult::Challenge(challenge) => {
                    // 返回 401 挑战
                    return self.build_401_response(&challenge);
                }
                AuthResult::Failed => {
                    // 认证失败
                    return self.build_error_response(StatusCode::UNAUTHORIZED, "Unauthorized");
                }
            }
        }

        // 5. 处理绑定（注册或注销）
        if expires == 0 {
            // 注销
            match self.store.remove_binding(&aor).await {
                Ok(()) => {
                    tracing::info!("Registrar: unregistered {}", aor);
                    self.build_200_ok_response(&aor, &contact_uri, 0)
                }
                Err(e) => {
                    tracing::error!("Registrar: failed to remove binding for {}: {}", aor, e);
                    self.build_error_response(
                        StatusCode::SERVER_INTERNAL_ERROR,
                        "Internal Server Error",
                    )
                }
            }
        } else {
            // 注册
            match self
                .store
                .save_binding_full(&aor, &contact_uri, expires, &call_id, cseq)
                .await
            {
                Ok(()) => {
                    tracing::info!(
                        "Registrar: registered {} -> {} (expires={})",
                        aor,
                        contact_uri,
                        expires
                    );
                    self.build_200_ok_response(&aor, &contact_uri, expires)
                }
                Err(e) => {
                    tracing::error!("Registrar: failed to save binding for {}: {}", aor, e);
                    self.build_error_response(
                        StatusCode::SERVER_INTERNAL_ERROR,
                        "Internal Server Error",
                    )
                }
            }
        }
    }

    /// 查询已注册设备
    ///
    /// # 参数
    ///
    /// - `aor` - 地址记录
    ///
    /// # 返回
    ///
    /// 返回绑定信息，如果未注册或已过期则返回 None。
    pub async fn query_binding(&self, aor: &str) -> Result<Option<BindingInfo>, RegistrationError> {
        self.store.query_binding(aor).await
    }

    /// 列出所有已注册设备
    ///
    /// 返回所有有效（未过期）的绑定信息。
    pub async fn list_bindings(&self) -> Result<Vec<BindingInfo>, RegistrationError> {
        self.store.list_active_bindings().await
    }

    /// 清理过期绑定
    ///
    /// 删除所有已过期的绑定，返回被清理的绑定数量。
    pub async fn cleanup_expired(&self) -> Result<usize, RegistrationError> {
        self.store.cleanup_expired().await
    }

    // ========================================================================
    // 内部辅助方法
    // ========================================================================

    /// 验证 REGISTER 请求格式
    ///
    /// 检查必需的 Contact、Call-ID、CSeq 头部。
    fn validate_register_request(
        &self,
        request: &SipRequest,
    ) -> Result<(String, u64, String, u32), SipResponse> {
        // 检查 Contact 头部
        let contact_uri = request
            .headers
            .get(&HeaderName::Contact)
            .and_then(|v| v.as_contact())
            .map(|c| c.uri.to_string())
            .ok_or_else(|| {
                self.build_error_response(StatusCode::BAD_REQUEST, "Missing Contact header")
            })?;

        // 检查 Call-ID 头部
        let call_id = request
            .headers
            .get(&HeaderName::CallId)
            .and_then(|v| v.as_call_id())
            .map(|c| c.0.clone())
            .ok_or_else(|| {
                self.build_error_response(StatusCode::BAD_REQUEST, "Missing Call-ID header")
            })?;

        // 检查 CSeq 头部
        let cseq = request
            .headers
            .get(&HeaderName::CSeq)
            .and_then(|v| v.as_cseq())
            .map(|c| c.sequence.0)
            .ok_or_else(|| {
                self.build_error_response(StatusCode::BAD_REQUEST, "Missing CSeq header")
            })?;

        // 提取 Expires 值
        let expires = self.extract_expires(request);

        Ok((contact_uri, expires, call_id, cseq))
    }

    /// 从请求中提取 Expires 值
    ///
    /// 优先使用 Contact 头部的 expires 参数，其次使用 Expires 头部。
    /// 默认值为 3600 秒。
    fn extract_expires(&self, request: &SipRequest) -> u64 {
        // 优先从 Contact 头部的 expires 参数获取
        if let Some(contact) = request
            .headers
            .get(&HeaderName::Contact)
            .and_then(|v| v.as_contact())
        {
            if let Some(expires) = contact.expires {
                return expires as u64;
            }
        }

        // 其次从 Expires 头部获取
        if let Some(HeaderValue::Expires(expires)) = request.headers.get(&HeaderName::Expires) {
            return *expires as u64;
        }

        // 默认值 3600 秒
        3600
    }

    /// 从请求的 To 头部提取 AOR
    fn extract_aor(&self, request: &SipRequest) -> Option<String> {
        request
            .headers
            .get(&HeaderName::To)
            .and_then(|v| v.as_from_to())
            .map(|ft| ft.uri.to_string())
    }

    /// 检查认证
    ///
    /// 返回认证结果：通过、需要挑战或失败。
    async fn check_authentication(
        &self,
        request: &SipRequest,
        _aor: &str,
        nonce: Option<&str>,
    ) -> AuthResult {
        // 检查是否包含 Authorization 头部
        let auth_header = request
            .headers
            .get(&HeaderName::Authorization)
            .and_then(|v| v.as_auth());

        if auth_header.is_none() {
            // 无 Authorization 头部，返回挑战
            let challenge_nonce = nonce
                .map(|s| s.to_string())
                .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());

            return AuthResult::Challenge(AuthChallenge {
                realm: self.realm.clone(),
                nonce: challenge_nonce,
                algorithm: Some("MD5".to_string()),
                opaque: None,
            });
        }

        let auth = auth_header.unwrap();

        // 验证认证方案
        if !auth.scheme.eq_ignore_ascii_case("Digest") {
            return AuthResult::Failed;
        }

        // 提取认证参数
        let username = match &auth.username {
            Some(u) => u.clone(),
            None => return AuthResult::Failed,
        };

        let realm = match &auth.realm {
            Some(r) => r.clone(),
            None => return AuthResult::Failed,
        };

        let nonce_val = match &auth.nonce {
            Some(n) => n.clone(),
            None => return AuthResult::Failed,
        };

        let uri = match &auth.uri {
            Some(u) => u.clone(),
            None => return AuthResult::Failed,
        };

        let response = match &auth.response {
            Some(r) => r.clone(),
            None => return AuthResult::Failed,
        };

        // 查找用户密码
        let password = match &self.credential_lookup {
            Some(lookup) => match lookup(&username) {
                Some(pw) => pw,
                None => return AuthResult::Failed,
            },
            None => return AuthResult::Failed,
        };

        // 计算期望的摘要响应
        let expected = match self.auth_handler.compute_response(
            &username,
            &password,
            &realm,
            &nonce_val,
            &uri,
            "REGISTER",
            auth.cnonce.as_deref(),
            auth.nc.unwrap_or(0),
            auth.qop.as_deref(),
            auth.algorithm.as_deref(),
            auth.opaque.as_deref(),
        ) {
            Ok(resp) => resp,
            Err(_) => return AuthResult::Failed,
        };

        // 比较摘要响应
        if expected.eq_ignore_ascii_case(&response) {
            AuthResult::Authenticated
        } else {
            tracing::warn!(
                "Registrar: authentication failed for {} (expected={}, got={})",
                username,
                expected,
                response
            );
            AuthResult::Failed
        }
    }

    /// 构建 401 Unauthorized 响应
    fn build_401_response(&self, challenge: &AuthChallenge) -> SipResponse {
        let mut www_auth = AuthHeader::digest()
            .with_realm(&challenge.realm)
            .with_nonce(&challenge.nonce);

        if let Some(ref algorithm) = challenge.algorithm {
            www_auth = www_auth.with_algorithm(algorithm);
        }

        if let Some(ref opaque) = challenge.opaque {
            www_auth = www_auth.with_opaque(opaque);
        }

        let mut headers = HeaderCollection::new();
        headers.insert(HeaderName::WwwAuthenticate, HeaderValue::Auth(www_auth));

        SipResponse {
            status_line: StatusLine {
                version: SipVersion,
                status_code: StatusCode::UNAUTHORIZED,
                reason_phrase: "Unauthorized".to_string(),
            },
            headers,
            body: None,
        }
    }

    /// 构建 200 OK 响应
    fn build_200_ok_response(&self, aor: &str, contact_uri: &str, expires: u64) -> SipResponse {
        // 构建 Contact 头部
        let contact_sip_uri = match sip_message::SipUri::parse(contact_uri) {
            Ok(uri) => uri,
            Err(_) => {
                return self.build_error_response(
                    StatusCode::SERVER_INTERNAL_ERROR,
                    "Invalid contact URI",
                );
            }
        };

        let contact_header = if expires > 0 {
            ContactHeader::new(contact_sip_uri).with_expires(expires as u32)
        } else {
            ContactHeader::new(contact_sip_uri)
        };

        // 构建 To 头部（添加 tag）
        let to_uri = match sip_message::SipUri::parse(aor) {
            Ok(uri) => uri,
            Err(_) => {
                return self
                    .build_error_response(StatusCode::SERVER_INTERNAL_ERROR, "Invalid AOR URI");
            }
        };
        let to_header = FromToHeader::with_generated_tag(to_uri);

        let mut headers = HeaderCollection::new();
        headers.insert(HeaderName::Contact, HeaderValue::Contact(contact_header));
        headers.insert(HeaderName::To, HeaderValue::FromTo(to_header));

        if expires > 0 {
            headers.insert(HeaderName::Expires, HeaderValue::Expires(expires as u32));
        }

        SipResponse {
            status_line: StatusLine {
                version: SipVersion,
                status_code: StatusCode::OK,
                reason_phrase: "OK".to_string(),
            },
            headers,
            body: None,
        }
    }

    /// 构建错误响应
    fn build_error_response(&self, status_code: StatusCode, reason: &str) -> SipResponse {
        SipResponse {
            status_line: StatusLine {
                version: SipVersion,
                status_code,
                reason_phrase: reason.to_string(),
            },
            headers: HeaderCollection::new(),
            body: None,
        }
    }
}

// ============================================================================
// 内部类型
// ============================================================================

/// 认证结果
enum AuthResult {
    /// 认证通过
    Authenticated,
    /// 需要认证挑战
    Challenge(AuthChallenge),
    /// 认证失败
    Failed,
}

/// 认证挑战参数
struct AuthChallenge {
    realm: String,
    nonce: String,
    algorithm: Option<String>,
    opaque: Option<String>,
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::DigestAuthHandler;
    use sip_message::{CSeqHeader, CallId, RequestLine, SipUri, Tag};

    /// 创建测试用 Registrar（不要求认证）
    fn create_registrar_no_auth() -> Registrar {
        let store = Arc::new(MemoryRegistrationStore::new());
        let auth_handler = Arc::new(DigestAuthHandler::new());
        Registrar::new(store, auth_handler, "example.com".to_string(), false)
    }

    /// 创建测试用 Registrar（要求认证）
    fn create_registrar_with_auth() -> Registrar {
        let store = Arc::new(MemoryRegistrationStore::new());
        let auth_handler = Arc::new(DigestAuthHandler::new());

        let credentials: Arc<dyn Fn(&str) -> Option<String> + Send + Sync> =
            Arc::new(|username| match username {
                "alice" => Some("secret".to_string()),
                "bob" => Some("password".to_string()),
                _ => None,
            });

        Registrar::new(store, auth_handler, "example.com".to_string(), true)
            .with_credential_lookup(credentials)
    }

    /// 构建测试 REGISTER 请求（无认证）
    fn build_register_request(
        aor: &str,
        contact_uri: &str,
        expires: u64,
        call_id: &str,
        cseq: u32,
    ) -> SipRequest {
        let aor_uri = SipUri::parse(aor).unwrap();
        let contact_sip_uri = SipUri::parse(contact_uri).unwrap();
        let request_uri = SipUri::parse("sip:example.com").unwrap();

        let from_header = FromToHeader::new(aor_uri.clone()).with_tag(Tag("fromtag".to_string()));
        let to_header = FromToHeader::new(aor_uri);
        let contact_header = if expires > 0 {
            ContactHeader::new(contact_sip_uri).with_expires(expires as u32)
        } else {
            ContactHeader::new(contact_sip_uri)
        };

        let mut headers = HeaderCollection::new();
        headers.insert(HeaderName::From, HeaderValue::FromTo(from_header));
        headers.insert(HeaderName::To, HeaderValue::FromTo(to_header));
        headers.insert(
            HeaderName::CallId,
            HeaderValue::CallId(CallId(call_id.to_string())),
        );
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(CSeqHeader::new(cseq, Method::Register)),
        );
        headers.insert(HeaderName::Contact, HeaderValue::Contact(contact_header));
        headers.insert(HeaderName::Expires, HeaderValue::Expires(expires as u32));
        headers.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

        SipRequest {
            request_line: RequestLine {
                method: Method::Register,
                request_uri,
                version: SipVersion,
            },
            headers,
            body: None,
        }
    }

    /// 构建带认证的 REGISTER 请求
    fn build_register_request_with_auth(
        aor: &str,
        contact_uri: &str,
        expires: u64,
        call_id: &str,
        cseq: u32,
        username: &str,
        password: &str,
        realm: &str,
        nonce: &str,
    ) -> SipRequest {
        let mut request = build_register_request(aor, contact_uri, expires, call_id, cseq);

        // 计算 Digest 响应
        let auth_handler = DigestAuthHandler::new();
        let digest_uri = "sip:example.com";
        let response = auth_handler
            .compute_response(
                username,
                password,
                realm,
                nonce,
                digest_uri,
                "REGISTER",
                None,
                0,
                None,
                Some("MD5"),
                None,
            )
            .unwrap();

        let auth = AuthHeader::digest()
            .with_username(username)
            .with_realm(realm)
            .with_nonce(nonce)
            .with_uri(digest_uri)
            .with_response(&response)
            .with_algorithm("MD5");

        request
            .headers
            .insert(HeaderName::Authorization, HeaderValue::Auth(auth));

        request
    }

    // ---- BindingInfo 测试 ----

    #[test]
    fn test_binding_info_remaining_expires() {
        let info = BindingInfo {
            aor: "sip:alice@example.com".to_string(),
            contact: "sip:alice@192.168.1.1:5060".to_string(),
            expires: 3600,
            registered_at: Instant::now(),
            call_id: "test-call-id".to_string(),
            cseq: 1,
        };

        let remaining = info.remaining_expires();
        assert!(remaining <= 3600);
        assert!(remaining > 3500);
        assert!(!info.is_expired());
    }

    #[test]
    fn test_binding_info_expired() {
        let info = BindingInfo {
            aor: "sip:alice@example.com".to_string(),
            contact: "sip:alice@192.168.1.1:5060".to_string(),
            expires: 0,
            registered_at: Instant::now() - std::time::Duration::from_secs(1),
            call_id: "test-call-id".to_string(),
            cseq: 1,
        };

        assert!(info.is_expired());
        assert_eq!(info.remaining_expires(), 0);
    }

    // ---- MemoryRegistrationStore 测试 ----

    #[tokio::test]
    async fn test_memory_store_save_and_query() {
        let store = MemoryRegistrationStore::new();

        store
            .save_binding_full(
                "sip:alice@example.com",
                "sip:alice@192.168.1.1:5060",
                3600,
                "call-id-1",
                1,
            )
            .await
            .unwrap();

        let binding = store.query_binding("sip:alice@example.com").await.unwrap();

        assert!(binding.is_some());
        let binding = binding.unwrap();
        assert_eq!(binding.aor, "sip:alice@example.com");
        assert_eq!(binding.contact, "sip:alice@192.168.1.1:5060");
        assert_eq!(binding.expires, 3600);
        assert_eq!(binding.call_id, "call-id-1");
        assert_eq!(binding.cseq, 1);
    }

    #[tokio::test]
    async fn test_memory_store_query_not_found() {
        let store = MemoryRegistrationStore::new();

        let binding = store
            .query_binding("sip:unknown@example.com")
            .await
            .unwrap();

        assert!(binding.is_none());
    }

    #[tokio::test]
    async fn test_memory_store_remove() {
        let store = MemoryRegistrationStore::new();

        store
            .save_binding_full(
                "sip:alice@example.com",
                "sip:alice@192.168.1.1:5060",
                3600,
                "call-id-1",
                1,
            )
            .await
            .unwrap();

        store.remove_binding("sip:alice@example.com").await.unwrap();

        let binding = store.query_binding("sip:alice@example.com").await.unwrap();

        assert!(binding.is_none());
    }

    #[tokio::test]
    async fn test_memory_store_list_bindings() {
        let store = MemoryRegistrationStore::new();

        store
            .save_binding_full(
                "sip:alice@example.com",
                "sip:alice@192.168.1.1:5060",
                3600,
                "call-id-1",
                1,
            )
            .await
            .unwrap();

        store
            .save_binding_full(
                "sip:bob@example.com",
                "sip:bob@192.168.1.2:5060",
                3600,
                "call-id-2",
                1,
            )
            .await
            .unwrap();

        let bindings = store.list_bindings().await.unwrap();
        assert_eq!(bindings.len(), 2);

        let active = store.list_active_bindings().await.unwrap();
        assert_eq!(active.len(), 2);
    }

    #[tokio::test]
    async fn test_memory_store_cleanup_expired() {
        let store = MemoryRegistrationStore::new();

        // 保存一个已过期的绑定
        {
            let mut bindings = store.bindings.lock().await;
            bindings.insert(
                "sip:expired@example.com".to_string(),
                BindingInfo {
                    aor: "sip:expired@example.com".to_string(),
                    contact: "sip:expired@192.168.1.1:5060".to_string(),
                    expires: 1,
                    registered_at: Instant::now() - std::time::Duration::from_secs(2),
                    call_id: "call-id-expired".to_string(),
                    cseq: 1,
                },
            );
        }

        // 保存一个有效的绑定
        store
            .save_binding_full(
                "sip:active@example.com",
                "sip:active@192.168.1.2:5060",
                3600,
                "call-id-active",
                1,
            )
            .await
            .unwrap();

        let removed = store.cleanup_expired().await.unwrap();
        assert_eq!(removed, 1);

        let active = store.list_active_bindings().await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].aor, "sip:active@example.com");
    }

    #[tokio::test]
    async fn test_memory_store_save_updates_existing() {
        let store = MemoryRegistrationStore::new();

        store
            .save_binding_full(
                "sip:alice@example.com",
                "sip:alice@192.168.1.1:5060",
                3600,
                "call-id-1",
                1,
            )
            .await
            .unwrap();

        store
            .save_binding_full(
                "sip:alice@example.com",
                "sip:alice@192.168.1.1:5061",
                7200,
                "call-id-1",
                2,
            )
            .await
            .unwrap();

        let binding = store
            .query_binding("sip:alice@example.com")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(binding.contact, "sip:alice@192.168.1.1:5061");
        assert_eq!(binding.expires, 7200);
        assert_eq!(binding.cseq, 2);
    }

    // ---- Registrar 测试（无认证）----

    #[tokio::test]
    async fn test_registrar_register_no_auth() {
        let registrar = create_registrar_no_auth();

        let request = build_register_request(
            "sip:alice@example.com",
            "sip:alice@192.168.1.1:5060",
            3600,
            "call-id-1",
            1,
        );

        let response = registrar.handle_register(&request, None).await;

        assert_eq!(response.status_line.status_code, StatusCode::OK);

        // 验证绑定已保存
        let binding = registrar
            .query_binding("sip:alice@example.com")
            .await
            .unwrap();
        assert!(binding.is_some());
        let binding = binding.unwrap();
        assert_eq!(binding.contact, "sip:alice@192.168.1.1:5060");
        assert_eq!(binding.expires, 3600);
    }

    #[tokio::test]
    async fn test_registrar_unregister_no_auth() {
        let registrar = create_registrar_no_auth();

        // 先注册
        let register_request = build_register_request(
            "sip:alice@example.com",
            "sip:alice@192.168.1.1:5060",
            3600,
            "call-id-1",
            1,
        );
        let response = registrar.handle_register(&register_request, None).await;
        assert_eq!(response.status_line.status_code, StatusCode::OK);

        // 再注销
        let unregister_request = build_register_request(
            "sip:alice@example.com",
            "sip:alice@192.168.1.1:5060",
            0,
            "call-id-1",
            2,
        );
        let response = registrar.handle_register(&unregister_request, None).await;
        assert_eq!(response.status_line.status_code, StatusCode::OK);

        // 验证绑定已删除
        let binding = registrar
            .query_binding("sip:alice@example.com")
            .await
            .unwrap();
        assert!(binding.is_none());
    }

    #[tokio::test]
    async fn test_registrar_missing_contact_header() {
        let registrar = create_registrar_no_auth();

        let mut request = build_register_request(
            "sip:alice@example.com",
            "sip:alice@192.168.1.1:5060",
            3600,
            "call-id-1",
            1,
        );
        request.headers.remove(&HeaderName::Contact);

        let response = registrar.handle_register(&request, None).await;
        assert_eq!(response.status_line.status_code, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_registrar_missing_call_id_header() {
        let registrar = create_registrar_no_auth();

        let mut request = build_register_request(
            "sip:alice@example.com",
            "sip:alice@192.168.1.1:5060",
            3600,
            "call-id-1",
            1,
        );
        request.headers.remove(&HeaderName::CallId);

        let response = registrar.handle_register(&request, None).await;
        assert_eq!(response.status_line.status_code, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_registrar_missing_cseq_header() {
        let registrar = create_registrar_no_auth();

        let mut request = build_register_request(
            "sip:alice@example.com",
            "sip:alice@192.168.1.1:5060",
            3600,
            "call-id-1",
            1,
        );
        request.headers.remove(&HeaderName::CSeq);

        let response = registrar.handle_register(&request, None).await;
        assert_eq!(response.status_line.status_code, StatusCode::BAD_REQUEST);
    }

    // ---- Registrar 测试（要求认证）----

    #[tokio::test]
    async fn test_registrar_register_no_auth_returns_401() {
        let registrar = create_registrar_with_auth();

        let request = build_register_request(
            "sip:alice@example.com",
            "sip:alice@192.168.1.1:5060",
            3600,
            "call-id-1",
            1,
        );

        let response = registrar.handle_register(&request, None).await;

        assert_eq!(response.status_line.status_code, StatusCode::UNAUTHORIZED);

        // 验证 WWW-Authenticate 头部
        let www_auth = response
            .headers
            .get(&HeaderName::WwwAuthenticate)
            .and_then(|v| v.as_auth());
        assert!(www_auth.is_some());

        let auth = www_auth.unwrap();
        assert_eq!(auth.scheme, "Digest");
        assert_eq!(auth.realm.as_deref(), Some("example.com"));
        assert!(auth.nonce.is_some());
    }

    #[tokio::test]
    async fn test_registrar_register_with_correct_auth() {
        let registrar = create_registrar_with_auth();

        // 先发送无认证请求获取 nonce
        let request_no_auth = build_register_request(
            "sip:alice@example.com",
            "sip:alice@192.168.1.1:5060",
            3600,
            "call-id-1",
            1,
        );

        let response_401 = registrar.handle_register(&request_no_auth, None).await;
        assert_eq!(
            response_401.status_line.status_code,
            StatusCode::UNAUTHORIZED
        );

        // 提取 nonce
        let nonce = response_401
            .headers
            .get(&HeaderName::WwwAuthenticate)
            .and_then(|v| v.as_auth())
            .and_then(|a| a.nonce.clone())
            .unwrap();

        // 构建带认证的请求
        let request_with_auth = build_register_request_with_auth(
            "sip:alice@example.com",
            "sip:alice@192.168.1.1:5060",
            3600,
            "call-id-1",
            2,
            "alice",
            "secret",
            "example.com",
            &nonce,
        );

        let response = registrar.handle_register(&request_with_auth, None).await;
        assert_eq!(response.status_line.status_code, StatusCode::OK);

        // 验证绑定已保存
        let binding = registrar
            .query_binding("sip:alice@example.com")
            .await
            .unwrap();
        assert!(binding.is_some());
    }

    #[tokio::test]
    async fn test_registrar_register_with_wrong_auth() {
        let registrar = create_registrar_with_auth();

        // 使用错误密码构建认证请求
        let request_with_wrong_auth = build_register_request_with_auth(
            "sip:alice@example.com",
            "sip:alice@192.168.1.1:5060",
            3600,
            "call-id-1",
            1,
            "alice",
            "wrongpassword",
            "example.com",
            "testnonce",
        );

        let response = registrar
            .handle_register(&request_with_wrong_auth, None)
            .await;
        assert_eq!(response.status_line.status_code, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_registrar_unregister_with_auth() {
        let registrar = create_registrar_with_auth();

        // 先用认证注册
        let nonce = "testnonce";
        let register_request = build_register_request_with_auth(
            "sip:alice@example.com",
            "sip:alice@192.168.1.1:5060",
            3600,
            "call-id-1",
            1,
            "alice",
            "secret",
            "example.com",
            nonce,
        );

        let response = registrar.handle_register(&register_request, None).await;
        assert_eq!(response.status_line.status_code, StatusCode::OK);

        // 再用认证注销（Expires=0）
        let unregister_request = build_register_request_with_auth(
            "sip:alice@example.com",
            "sip:alice@192.168.1.1:5060",
            0,
            "call-id-1",
            2,
            "alice",
            "secret",
            "example.com",
            nonce,
        );

        let response = registrar.handle_register(&unregister_request, None).await;
        assert_eq!(response.status_line.status_code, StatusCode::OK);

        // 验证绑定已删除
        let binding = registrar
            .query_binding("sip:alice@example.com")
            .await
            .unwrap();
        assert!(binding.is_none());
    }

    // ---- Registrar 列表和清理测试 ----

    #[tokio::test]
    async fn test_registrar_list_bindings() {
        let registrar = create_registrar_no_auth();

        // 注册两个设备
        let request1 = build_register_request(
            "sip:alice@example.com",
            "sip:alice@192.168.1.1:5060",
            3600,
            "call-id-1",
            1,
        );
        let request2 = build_register_request(
            "sip:bob@example.com",
            "sip:bob@192.168.1.2:5060",
            3600,
            "call-id-2",
            1,
        );

        registrar.handle_register(&request1, None).await;
        registrar.handle_register(&request2, None).await;

        let bindings = registrar.list_bindings().await.unwrap();
        assert_eq!(bindings.len(), 2);
    }

    #[tokio::test]
    async fn test_registrar_cleanup_expired() {
        let store = Arc::new(MemoryRegistrationStore::new());
        let auth_handler = Arc::new(DigestAuthHandler::new());
        let registrar = Registrar::new(
            store.clone(),
            auth_handler,
            "example.com".to_string(),
            false,
        );

        // 注册设备
        let request = build_register_request(
            "sip:alice@example.com",
            "sip:alice@192.168.1.1:5060",
            3600,
            "call-id-1",
            1,
        );
        registrar.handle_register(&request, None).await;

        // 手动插入一个过期绑定
        {
            let memory_store = store.as_ref();
            let mut bindings = memory_store.bindings.lock().await;
            bindings.insert(
                "sip:expired@example.com".to_string(),
                BindingInfo {
                    aor: "sip:expired@example.com".to_string(),
                    contact: "sip:expired@192.168.1.3:5060".to_string(),
                    expires: 1,
                    registered_at: Instant::now() - std::time::Duration::from_secs(2),
                    call_id: "call-id-expired".to_string(),
                    cseq: 1,
                },
            );
        }

        let removed = registrar.cleanup_expired().await.unwrap();
        assert_eq!(removed, 1);

        let bindings = registrar.list_bindings().await.unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].aor, "sip:alice@example.com");
    }

    // ---- 200 OK 响应验证测试 ----

    #[tokio::test]
    async fn test_registrar_200_ok_contains_contact_and_expires() {
        let registrar = create_registrar_no_auth();

        let request = build_register_request(
            "sip:alice@example.com",
            "sip:alice@192.168.1.1:5060",
            3600,
            "call-id-1",
            1,
        );

        let response = registrar.handle_register(&request, None).await;

        assert_eq!(response.status_line.status_code, StatusCode::OK);

        // 验证 Contact 头部
        let contact = response
            .headers
            .get(&HeaderName::Contact)
            .and_then(|v| v.as_contact());
        assert!(contact.is_some());
        let contact = contact.unwrap();
        assert_eq!(contact.expires, Some(3600));

        // 验证 Expires 头部
        let expires = response.headers.get(&HeaderName::Expires).and_then(|v| {
            if let HeaderValue::Expires(e) = v {
                Some(*e)
            } else {
                None
            }
        });
        assert_eq!(expires, Some(3600));
    }

    #[tokio::test]
    async fn test_registrar_200_ok_unregister_no_expires() {
        let registrar = create_registrar_no_auth();

        // 先注册
        let register_request = build_register_request(
            "sip:alice@example.com",
            "sip:alice@192.168.1.1:5060",
            3600,
            "call-id-1",
            1,
        );
        registrar.handle_register(&register_request, None).await;

        // 再注销
        let unregister_request = build_register_request(
            "sip:alice@example.com",
            "sip:alice@192.168.1.1:5060",
            0,
            "call-id-1",
            2,
        );
        let response = registrar.handle_register(&unregister_request, None).await;

        assert_eq!(response.status_line.status_code, StatusCode::OK);

        // 注销响应不应包含 Expires 头部
        let expires = response.headers.get(&HeaderName::Expires);
        assert!(expires.is_none());
    }
}
