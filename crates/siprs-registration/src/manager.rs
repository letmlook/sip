//! SIP 注册管理器
//!
//! 提供 `RegistrationManager` 实现，管理 SIP 注册的完整生命周期，
//! 包括注册、注销、刷新、摘要认证处理和状态管理。
//!
//! # 注册流程
//!
//! 1. 调用 `register()` 发起注册，返回 REGISTER 请求
//! 2. 将请求通过事务层发送
//! 3. 收到响应后调用 `handle_response()` 处理
//! 4. 如果需要认证，自动构建带 Authorization 头部的新请求
//! 5. 注册成功后，定期检查是否需要刷新

use std::collections::HashMap;
use std::sync::Arc;

use siprs_core::config::{Credentials, RegistrationConfig};
use siprs_core::metrics::SipMetrics;
use siprs_core::SipVersion;
use siprs_core::{CSeqNumber, Host, RegistrationError, StatusCode, TransportProtocol};
use siprs_message::{
    AuthHeader, CSeqHeader, CallId, ContactHeader, FromToHeader, HeaderCollection, HeaderName,
    HeaderValue, Method, RequestLine, SipRequest, SipResponse, SipUri, Tag, UriParams,
};
use tokio::sync::{mpsc, Mutex};

use crate::auth::{build_auth_header, AuthHandler, DigestAuthHandler};
use crate::types::{
    ContactInfo, RegistrationEvent, RegistrationFailureReason, RegistrationId, RegistrationInfo,
    RegistrationState,
};

// ============================================================================
// RegistrationManager - 注册管理器
// ============================================================================

/// SIP 注册管理器
///
/// 管理所有 SIP 注册会话，提供注册、注销、刷新和状态查询功能。
/// 通过事件通道向 TU 层通知注册状态变化。
///
/// # 示例
///
/// ```ignore
/// use siprs_registration::manager::RegistrationManager;
/// use siprs_core::config::RegistrationConfig;
/// use siprs_core::metrics::SipMetrics;
///
/// let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
/// let config = RegistrationConfig::default();
/// let metrics = Arc::new(SipMetrics::new());
///
/// let manager = RegistrationManager::new(config, None, event_tx, metrics);
///
/// // 发起注册
/// let (reg_id, request) = manager.register(
///     "sip:alice@example.com",
///     "sip:alice@192.168.1.1:5060",
///     None,
/// ).unwrap();
/// ```
pub struct RegistrationManager {
    /// 注册配置
    config: RegistrationConfig,
    /// 认证凭据
    credentials: Option<Credentials>,
    /// 注册信息表
    registrations: Arc<Mutex<HashMap<RegistrationId, RegistrationInfo>>>,
    /// 认证处理器
    auth_handler: Arc<dyn AuthHandler>,
    /// 注册事件发送端
    event_tx: mpsc::UnboundedSender<RegistrationEvent>,
    /// 运行指标
    metrics: Arc<SipMetrics>,
}

impl RegistrationManager {
    /// 创建新的注册管理器
    ///
    /// # 参数
    ///
    /// - `config` - 注册配置
    /// - `credentials` - 认证凭据（可选）
    /// - `event_tx` - 注册事件发送端
    /// - `metrics` - 运行指标收集器
    pub fn new(
        config: RegistrationConfig,
        credentials: Option<Credentials>,
        event_tx: mpsc::UnboundedSender<RegistrationEvent>,
        metrics: Arc<SipMetrics>,
    ) -> Self {
        Self {
            config,
            credentials,
            registrations: Arc::new(Mutex::new(HashMap::new())),
            auth_handler: Arc::new(DigestAuthHandler::new()),
            event_tx,
            metrics,
        }
    }

    /// 创建带自定义认证处理器的注册管理器
    pub fn with_auth_handler(
        config: RegistrationConfig,
        credentials: Option<Credentials>,
        event_tx: mpsc::UnboundedSender<RegistrationEvent>,
        metrics: Arc<SipMetrics>,
        auth_handler: Arc<dyn AuthHandler>,
    ) -> Self {
        Self {
            config,
            credentials,
            registrations: Arc::new(Mutex::new(HashMap::new())),
            auth_handler,
            event_tx,
            metrics,
        }
    }

    // ========================================================================
    // 注册操作
    // ========================================================================

    /// 发起注册
    ///
    /// 构建 REGISTER 请求并创建注册记录。调用者负责将返回的请求
    /// 通过事务层发送。
    ///
    /// # 参数
    ///
    /// - `aor` - 地址记录（Address-of-Record），格式 sip:user@domain
    /// - `contact_uri` - 本端联系地址，格式 sip:user@host:port
    /// - `registrar` - 注册服务器地址（可选，None 则使用 AOR 域名）
    ///
    /// # 返回
    ///
    /// 返回注册标识和构建的 REGISTER 请求。
    ///
    /// # REGISTER 请求构建规则
    ///
    /// - Request-URI 为注册服务器地址
    /// - To 头部为 AOR
    /// - From 头部与 To 相同（含 tag）
    /// - Contact 头部包含本端联系地址
    /// - Expires 头部为配置的默认有效期
    /// - Call-ID 为新生成（同一 AOR 后续注册复用）
    /// - CSeq 序列号从 1 开始
    pub async fn register(
        &self,
        aor: &str,
        contact_uri: &str,
        registrar: Option<&str>,
    ) -> Result<(RegistrationId, SipRequest), RegistrationError> {
        let reg_id = RegistrationId::new();
        let call_id = CallId::new();
        let from_tag = Tag::new();

        // 确定注册服务器地址
        let registrar_addr = registrar
            .map(|s| s.to_string())
            .or_else(|| self.config.registrar_server.clone())
            .unwrap_or_else(|| extract_domain_from_aor(aor));

        let expires = self.config.default_expires;

        // 构建 REGISTER 请求
        let request = self.build_register_request(
            aor,
            contact_uri,
            &registrar_addr,
            &call_id,
            &from_tag,
            1, // CSeq 从 1 开始
            expires,
            None, // 无 Authorization 头部
            None, // 无第三方注册
        )?;

        // 创建注册信息
        let info = RegistrationInfo {
            id: reg_id.clone(),
            state: RegistrationState::Registering,
            aor: aor.to_string(),
            contacts: vec![ContactInfo {
                uri: contact_uri.to_string(),
                expires: Some(expires as u32),
            }],
            call_id,
            cseq: CSeqNumber(1),
            expires,
            registered_at: None,
            from_tag,
            local_contact: contact_uri.to_string(),
            registrar: registrar_addr,
            auth_attempted: false,
            nonce_count: 0,
            third_party_from: None,
        };

        // 保存注册信息
        self.registrations.lock().await.insert(reg_id.clone(), info);
        self.metrics.inc_active_registrations();

        tracing::info!(
            "RegistrationManager: initiated registration {} for {}",
            reg_id,
            aor
        );

        Ok((reg_id, request))
    }

    /// 发起第三方注册
    ///
    /// 第三方注册时，From 头部为注册者地址，To 头部为被注册者 AOR。
    ///
    /// # 参数
    ///
    /// - `aor` - 被注册者 AOR
    /// - `contact_uri` - 本端联系地址
    /// - `from_aor` - 注册者 AOR（From 头部地址）
    /// - `registrar` - 注册服务器地址（可选）
    pub async fn register_third_party(
        &self,
        aor: &str,
        contact_uri: &str,
        from_aor: &str,
        registrar: Option<&str>,
    ) -> Result<(RegistrationId, SipRequest), RegistrationError> {
        let reg_id = RegistrationId::new();
        let call_id = CallId::new();
        let from_tag = Tag::new();

        let registrar_addr = registrar
            .map(|s| s.to_string())
            .or_else(|| self.config.registrar_server.clone())
            .unwrap_or_else(|| extract_domain_from_aor(aor));

        let expires = self.config.default_expires;

        // 构建第三方注册请求（From 为注册者地址，To 为被注册者 AOR）
        let request = self.build_register_request(
            aor,
            contact_uri,
            &registrar_addr,
            &call_id,
            &from_tag,
            1,
            expires,
            None,
            Some(from_aor),
        )?;

        let info = RegistrationInfo {
            id: reg_id.clone(),
            state: RegistrationState::Registering,
            aor: aor.to_string(),
            contacts: vec![ContactInfo {
                uri: contact_uri.to_string(),
                expires: Some(expires as u32),
            }],
            call_id,
            cseq: CSeqNumber(1),
            expires,
            registered_at: None,
            from_tag,
            local_contact: contact_uri.to_string(),
            registrar: registrar_addr,
            auth_attempted: false,
            nonce_count: 0,
            third_party_from: Some(from_aor.to_string()),
        };

        self.registrations.lock().await.insert(reg_id.clone(), info);
        self.metrics.inc_active_registrations();

        tracing::info!(
            "RegistrationManager: initiated third-party registration {} for {} (from: {})",
            reg_id,
            aor,
            from_aor
        );

        Ok((reg_id, request))
    }

    /// 发起注销
    ///
    /// 构建 Expires 为 0 的 REGISTER 请求。
    ///
    /// # 参数
    ///
    /// - `registration_id` - 注册标识
    pub async fn unregister(
        &self,
        registration_id: &RegistrationId,
    ) -> Result<SipRequest, RegistrationError> {
        let mut registrations = self.registrations.lock().await;
        let info =
            registrations
                .get_mut(registration_id)
                .ok_or_else(|| RegistrationError::NotFound {
                    id: registration_id.to_string(),
                })?;

        if info.state != RegistrationState::Registered && info.state != RegistrationState::Expired {
            return Err(RegistrationError::InvalidState {
                current: info.state.to_string(),
                expected: "Registered or Expired".to_string(),
            });
        }

        // 递增 CSeq
        info.cseq.0 = info.cseq.0.saturating_add(1);
        info.state = RegistrationState::Unregistering;

        let request = self.build_register_request(
            &info.aor,
            &info.local_contact,
            &info.registrar,
            &info.call_id,
            &info.from_tag,
            info.cseq.0,
            0, // Expires 为 0 表示注销
            None,
            info.third_party_from.as_deref(),
        )?;

        tracing::info!(
            "RegistrationManager: initiated unregistration for {}",
            registration_id
        );

        Ok(request)
    }

    /// 刷新注册
    ///
    /// 构建新的 REGISTER 请求以刷新现有注册。
    ///
    /// # 参数
    ///
    /// - `registration_id` - 注册标识
    pub async fn refresh(
        &self,
        registration_id: &RegistrationId,
    ) -> Result<SipRequest, RegistrationError> {
        let mut registrations = self.registrations.lock().await;
        let info =
            registrations
                .get_mut(registration_id)
                .ok_or_else(|| RegistrationError::NotFound {
                    id: registration_id.to_string(),
                })?;

        if info.state != RegistrationState::Registered {
            return Err(RegistrationError::InvalidState {
                current: info.state.to_string(),
                expected: "Registered".to_string(),
            });
        }

        // 递增 CSeq
        info.cseq.0 = info.cseq.0.saturating_add(1);
        info.state = RegistrationState::Registering;

        let request = self.build_register_request(
            &info.aor,
            &info.local_contact,
            &info.registrar,
            &info.call_id,
            &info.from_tag,
            info.cseq.0,
            info.expires,
            None,
            info.third_party_from.as_deref(),
        )?;

        tracing::info!(
            "RegistrationManager: refreshing registration for {}",
            registration_id
        );

        Ok(request)
    }

    // ========================================================================
    // 响应处理
    // ========================================================================

    /// 处理注册响应
    ///
    /// 根据响应状态码更新注册状态，处理认证挑战和 423 响应。
    /// 如果需要重试（认证或 423），返回新的 REGISTER 请求。
    ///
    /// # 参数
    ///
    /// - `registration_id` - 注册标识
    /// - `response` - 收到的 SIP 响应
    ///
    /// # 返回
    ///
    /// - `Ok(Some(request))` - 需要重试，返回新的 REGISTER 请求
    /// - `Ok(None)` - 处理完成，无需重试
    /// - `Err(_)` - 处理失败
    pub async fn handle_response(
        &self,
        registration_id: &RegistrationId,
        response: &SipResponse,
    ) -> Result<Option<SipRequest>, RegistrationError> {
        let status_code = response.status_line.status_code;

        tracing::debug!(
            "RegistrationManager: handling response {} for registration {}",
            status_code,
            registration_id
        );

        // Call-ID 一致性检测
        if let Some(call_id_value) = response.headers.get(&HeaderName::CallId) {
            if let Some(call_id) = call_id_value.as_call_id() {
                let registrations = self.registrations.lock().await;
                if let Some(info) = registrations.get(registration_id) {
                    if info.call_id != *call_id {
                        tracing::warn!(
                            "RegistrationManager: Call-ID mismatch for {}, expected={}, got={}",
                            registration_id,
                            info.call_id,
                            call_id
                        );
                    }
                }
            }
        }

        match status_code {
            code if code.is_success() => {
                self.handle_success_response(registration_id, response)
                    .await
            }
            StatusCode::UNAUTHORIZED => {
                self.handle_auth_challenge(
                    registration_id,
                    response,
                    &HeaderName::WwwAuthenticate,
                    &HeaderName::Authorization,
                )
                .await
            }
            StatusCode::PROXY_AUTH_REQUIRED => {
                self.handle_auth_challenge(
                    registration_id,
                    response,
                    &HeaderName::ProxyAuthenticate,
                    &HeaderName::ProxyAuthorization,
                )
                .await
            }
            StatusCode::INTERVAL_TOO_BRIEF => {
                self.handle_interval_too_brief(registration_id, response)
                    .await
            }
            _ => self.handle_error_response(registration_id, response).await,
        }
    }

    /// 处理 2xx 成功响应
    async fn handle_success_response(
        &self,
        registration_id: &RegistrationId,
        response: &SipResponse,
    ) -> Result<Option<SipRequest>, RegistrationError> {
        let mut registrations = self.registrations.lock().await;
        let info =
            registrations
                .get_mut(registration_id)
                .ok_or_else(|| RegistrationError::NotFound {
                    id: registration_id.to_string(),
                })?;

        let was_registering = info.state == RegistrationState::Registering;
        let was_unregistering = info.state == RegistrationState::Unregistering;

        // 提取响应中的 Contact 头部
        let contacts: Vec<ContactInfo> = response
            .headers
            .get_all(&HeaderName::Contact)
            .iter()
            .filter_map(|v| {
                if let HeaderValue::Contact(contact) = v {
                    Some(ContactInfo {
                        uri: contact.uri.to_string(),
                        expires: contact.expires,
                    })
                } else {
                    None
                }
            })
            .collect();

        // 提取响应中的 Expires 头部
        if let Some(HeaderValue::Expires(expires)) = response.headers.get(&HeaderName::Expires) {
            info.expires = *expires as u64;
        }

        if was_unregistering {
            // 注销成功
            info.state = RegistrationState::Unregistered;
            info.registered_at = None;
            self.metrics.dec_active_registrations();

            let _ = self.event_tx.send(RegistrationEvent::Unregistered {
                registration_id: registration_id.clone(),
            });

            tracing::info!("RegistrationManager: unregistered {}", registration_id);
        } else if was_registering {
            // 注册成功
            info.state = RegistrationState::Registered;
            info.registered_at = Some(std::time::Instant::now());
            if !contacts.is_empty() {
                info.contacts = contacts;
            }

            let _ = self.event_tx.send(RegistrationEvent::Registered {
                registration_id: registration_id.clone(),
            });

            tracing::info!("RegistrationManager: registered {}", registration_id);
        }

        Ok(None)
    }

    /// 处理 401/407 认证挑战
    async fn handle_auth_challenge(
        &self,
        registration_id: &RegistrationId,
        response: &SipResponse,
        challenge_header: &HeaderName,
        auth_header: &HeaderName,
    ) -> Result<Option<SipRequest>, RegistrationError> {
        let mut registrations = self.registrations.lock().await;
        let info =
            registrations
                .get_mut(registration_id)
                .ok_or_else(|| RegistrationError::NotFound {
                    id: registration_id.to_string(),
                })?;

        // 如果已经尝试过认证，停止重试
        if info.auth_attempted {
            info.state = RegistrationState::Unregistered;
            self.metrics.dec_active_registrations();

            let _ = self.event_tx.send(RegistrationEvent::AuthenticationFailed {
                registration_id: registration_id.clone(),
            });

            tracing::warn!(
                "RegistrationManager: authentication failed for {} (already attempted)",
                registration_id
            );

            return Ok(None);
        }

        // 获取凭据
        let credentials = match &self.credentials {
            Some(creds) => creds.clone(),
            None => {
                info.state = RegistrationState::Unregistered;
                self.metrics.dec_active_registrations();

                let _ = self.event_tx.send(RegistrationEvent::AuthenticationFailed {
                    registration_id: registration_id.clone(),
                });

                return Err(RegistrationError::AuthenticationFailed {
                    reason: "no credentials available".to_string(),
                });
            }
        };

        // 提取 challenge
        let challenge = response
            .headers
            .get(challenge_header)
            .and_then(|v| v.as_auth())
            .ok_or_else(|| RegistrationError::AuthenticationFailed {
                reason: format!("missing {:?} header in challenge", challenge_header),
            })?
            .clone();

        // 递增 CSeq 和 nonce 计数
        info.cseq.0 = info.cseq.0.saturating_add(1);
        info.nonce_count = info.nonce_count.saturating_add(1);
        info.auth_attempted = true;
        info.state = RegistrationState::Registering;

        // 计算 Digest URI（Request-URI）
        let digest_uri = info.registrar.clone();

        // 构建认证头部
        let auth = build_auth_header(
            &challenge,
            &credentials.username,
            &credentials.password,
            &digest_uri,
            "REGISTER",
            info.nonce_count,
            self.auth_handler.as_ref(),
        )?;

        // 构建带认证头部的 REGISTER 请求
        let request = self.build_register_request(
            &info.aor,
            &info.local_contact,
            &info.registrar,
            &info.call_id,
            &info.from_tag,
            info.cseq.0,
            info.expires,
            Some((auth_header, &auth)),
            info.third_party_from.as_deref(),
        )?;

        tracing::info!(
            "RegistrationManager: retrying registration {} with authentication",
            registration_id
        );

        Ok(Some(request))
    }

    /// 处理 423 Interval Too Brief
    async fn handle_interval_too_brief(
        &self,
        registration_id: &RegistrationId,
        response: &SipResponse,
    ) -> Result<Option<SipRequest>, RegistrationError> {
        let mut registrations = self.registrations.lock().await;
        let info =
            registrations
                .get_mut(registration_id)
                .ok_or_else(|| RegistrationError::NotFound {
                    id: registration_id.to_string(),
                })?;

        // 提取 Min-Expires 头部
        let min_expires = response
            .headers
            .get(&HeaderName::MinExpires)
            .and_then(|v| {
                if let HeaderValue::Raw(s) = v {
                    s.parse::<u64>().ok()
                } else {
                    None
                }
            })
            .ok_or_else(|| RegistrationError::RegistrationFailed {
                reason: "423 response missing Min-Expires header".to_string(),
            })?;

        // 使用服务器建议的最小有效期
        info.expires = min_expires;
        info.cseq.0 = info.cseq.0.saturating_add(1);
        info.state = RegistrationState::Registering;

        let request = self.build_register_request(
            &info.aor,
            &info.local_contact,
            &info.registrar,
            &info.call_id,
            &info.from_tag,
            info.cseq.0,
            info.expires,
            None,
            info.third_party_from.as_deref(),
        )?;

        tracing::info!(
            "RegistrationManager: retrying registration {} with Min-Expires={}",
            registration_id,
            min_expires
        );

        Ok(Some(request))
    }

    /// 处理错误响应
    async fn handle_error_response(
        &self,
        registration_id: &RegistrationId,
        response: &SipResponse,
    ) -> Result<Option<SipRequest>, RegistrationError> {
        let mut registrations = self.registrations.lock().await;
        let info =
            registrations
                .get_mut(registration_id)
                .ok_or_else(|| RegistrationError::NotFound {
                    id: registration_id.to_string(),
                })?;

        let status_code = response.status_line.status_code;
        let reason = match status_code {
            StatusCode::FORBIDDEN => RegistrationFailureReason::ServerRejected,
            StatusCode::NOT_FOUND => RegistrationFailureReason::ServerRejected,
            StatusCode::REQUEST_TIMEOUT => RegistrationFailureReason::Timeout,
            StatusCode::SERVICE_UNAVAILABLE => RegistrationFailureReason::NetworkUnreachable,
            _ => RegistrationFailureReason::ServerRejected,
        };

        info.state = RegistrationState::Unregistered;
        self.metrics.dec_active_registrations();

        let _ = self.event_tx.send(RegistrationEvent::RegistrationFailed {
            registration_id: registration_id.clone(),
            reason,
        });

        tracing::warn!(
            "RegistrationManager: registration {} failed with status {}",
            registration_id,
            status_code
        );

        Ok(None)
    }

    // ========================================================================
    // 状态查询
    // ========================================================================

    /// 获取注册状态
    pub async fn registration_state(
        &self,
        registration_id: &RegistrationId,
    ) -> Option<RegistrationState> {
        let registrations = self.registrations.lock().await;
        registrations.get(registration_id).map(|info| info.state)
    }

    /// 获取注册信息
    pub async fn registration_info(
        &self,
        registration_id: &RegistrationId,
    ) -> Option<RegistrationInfo> {
        let registrations = self.registrations.lock().await;
        registrations.get(registration_id).cloned()
    }

    /// 检查所有注册是否需要刷新
    ///
    /// 返回需要刷新的注册标识列表。
    /// 当注册有效期剩余不足配置的刷新阈值（默认 50%）时，需要刷新。
    pub async fn check_refresh_needed(&self) -> Vec<RegistrationId> {
        let registrations = self.registrations.lock().await;
        let threshold = self.config.refresh_threshold;

        registrations
            .values()
            .filter(|info| {
                info.state == RegistrationState::Registered && info.needs_refresh(threshold)
            })
            .map(|info| info.id.clone())
            .collect()
    }

    /// 检查所有已过期注册
    ///
    /// 返回已过期的注册标识列表，并将状态标记为 Expired。
    pub async fn check_expired(&self) -> Vec<RegistrationId> {
        let mut registrations = self.registrations.lock().await;
        let mut expired = Vec::new();

        for info in registrations.values_mut() {
            if info.state == RegistrationState::Registered {
                if let Some(remaining) = info.remaining_expires() {
                    if remaining == 0 {
                        info.state = RegistrationState::Expired;
                        self.metrics.dec_active_registrations();

                        let _ = self.event_tx.send(RegistrationEvent::Expired {
                            registration_id: info.id.clone(),
                        });

                        expired.push(info.id.clone());
                    }
                }
            }
        }

        expired
    }

    /// 获取所有注册标识
    pub async fn registration_ids(&self) -> Vec<RegistrationId> {
        let registrations = self.registrations.lock().await;
        registrations.keys().cloned().collect()
    }

    // ========================================================================
    // REGISTER 请求构建
    // ========================================================================

    /// 构建 REGISTER 请求
    ///
    /// 按照 RFC 3261 Section 10 构建符合规范的 REGISTER 请求：
    /// - Request-URI 为注册服务器地址
    /// - To 头部为 AOR
    /// - From 头部与 To 相同（第三方注册时为注册者地址）
    /// - Contact 头部包含本端联系地址
    /// - Expires 头部设置有效期
    /// - Call-ID、CSeq、Via、Max-Forwards 头部
    #[allow(clippy::too_many_arguments)]
    fn build_register_request(
        &self,
        aor: &str,
        contact_uri: &str,
        registrar: &str,
        call_id: &CallId,
        from_tag: &Tag,
        cseq_number: u32,
        expires: u64,
        auth: Option<(&HeaderName, &AuthHeader)>,
        third_party_from: Option<&str>,
    ) -> Result<SipRequest, RegistrationError> {
        // 解析 AOR URI
        let aor_uri = SipUri::parse(aor).map_err(|e| RegistrationError::RegistrationFailed {
            reason: format!("invalid AOR URI: {}", e),
        })?;

        // 解析注册服务器 URI 作为 Request-URI
        let request_uri =
            SipUri::parse(registrar).map_err(|e| RegistrationError::RegistrationFailed {
                reason: format!("invalid registrar URI: {}", e),
            })?;

        // 解析联系地址 URI
        let contact_sip_uri =
            SipUri::parse(contact_uri).map_err(|e| RegistrationError::RegistrationFailed {
                reason: format!("invalid contact URI: {}", e),
            })?;

        // 构建 From 头部
        let from_aor = third_party_from.unwrap_or(aor);
        let from_uri =
            SipUri::parse(from_aor).map_err(|e| RegistrationError::RegistrationFailed {
                reason: format!("invalid From URI: {}", e),
            })?;
        let from_header = FromToHeader::new(from_uri).with_tag(from_tag.clone());

        // 构建 To 头部（始终为 AOR）
        let to_header = FromToHeader::new(aor_uri);

        // 构建 Contact 头部
        let contact_header = ContactHeader::new(contact_sip_uri);

        // 构建 Via 头部
        let via_host = extract_host_from_uri(&request_uri);
        let via = siprs_message::ViaHeader::new(TransportProtocol::Udp, via_host, Some(5060));

        // 构建 CSeq 头部
        let cseq = CSeqHeader::new(cseq_number, Method::Register);

        // 组装头部
        let mut headers = HeaderCollection::new();
        headers.insert(HeaderName::Via, HeaderValue::Via(via));
        headers.insert(HeaderName::From, HeaderValue::FromTo(from_header));
        headers.insert(HeaderName::To, HeaderValue::FromTo(to_header));
        headers.insert(HeaderName::CallId, HeaderValue::CallId(call_id.clone()));
        headers.insert(HeaderName::CSeq, HeaderValue::CSeq(cseq));
        headers.insert(HeaderName::Contact, HeaderValue::Contact(contact_header));
        headers.insert(HeaderName::Expires, HeaderValue::Expires(expires as u32));
        headers.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

        // 添加认证头部（如果存在）
        if let Some((auth_name, auth_value)) = auth {
            headers.insert(auth_name.clone(), HeaderValue::Auth(auth_value.clone()));
        }

        // 构建请求
        let request = SipRequest {
            request_line: RequestLine {
                method: Method::Register,
                request_uri,
                version: SipVersion,
            },
            headers,
            body: None,
        };

        Ok(request)
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 从 AOR 提取域名部分作为注册服务器地址
///
/// 例如：`sip:alice@example.com` → `sip:example.com`
fn extract_domain_from_aor(aor: &str) -> String {
    SipUri::parse(aor)
        .map(|uri| {
            let domain_uri = SipUri {
                scheme: uri.scheme,
                user_info: None,
                host: uri.host,
                port: uri.port,
                params: UriParams::new(),
                headers: siprs_message::UriHeaders::new(),
            };
            domain_uri.to_string()
        })
        .unwrap_or_else(|_| aor.to_string())
}

/// 从 URI 提取 Host 用于 Via 头部
fn extract_host_from_uri(uri: &SipUri) -> Host {
    uri.host.clone()
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 创建测试用的 RegistrationManager
    fn create_test_manager() -> RegistrationManager {
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let config = RegistrationConfig::default();
        let metrics = Arc::new(SipMetrics::new());

        RegistrationManager::new(config, None, event_tx, metrics)
    }

    /// 创建带凭据的 RegistrationManager
    fn create_test_manager_with_credentials() -> RegistrationManager {
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let config = RegistrationConfig::default();
        let metrics = Arc::new(SipMetrics::new());
        let credentials = Credentials {
            username: "alice".to_string(),
            password: "secret".to_string(),
            realm: None,
        };

        RegistrationManager::new(config, Some(credentials), event_tx, metrics)
    }

    #[tokio::test]
    async fn test_register() {
        let manager = create_test_manager();

        let (reg_id, request) = manager
            .register("sip:alice@example.com", "sip:alice@192.168.1.1:5060", None)
            .await
            .unwrap();

        // 验证 REGISTER 请求
        assert_eq!(request.request_line.method, Method::Register);

        // Request-URI 应为注册服务器地址（AOR 的域名部分）
        assert_eq!(
            request.request_line.request_uri.to_string(),
            "sip:example.com"
        );

        // 验证 To 头部为 AOR
        let to = request
            .headers
            .get(&HeaderName::To)
            .unwrap()
            .as_from_to()
            .unwrap();
        assert_eq!(to.uri.to_string(), "sip:alice@example.com");

        // 验证 From 头部与 To 相同（含 tag）
        let from = request
            .headers
            .get(&HeaderName::From)
            .unwrap()
            .as_from_to()
            .unwrap();
        assert_eq!(from.uri.to_string(), "sip:alice@example.com");
        assert!(from.tag.is_some());

        // 验证 Contact 头部
        let contact = request
            .headers
            .get(&HeaderName::Contact)
            .unwrap()
            .as_contact()
            .unwrap();
        assert_eq!(contact.uri.to_string(), "sip:alice@192.168.1.1:5060");

        // 验证 Expires 头部
        let expires = request.headers.get(&HeaderName::Expires).unwrap();
        if let HeaderValue::Expires(val) = expires {
            assert_eq!(*val, 3600);
        } else {
            panic!("Expected Expires header");
        }

        // 验证 CSeq
        let cseq = request
            .headers
            .get(&HeaderName::CSeq)
            .unwrap()
            .as_cseq()
            .unwrap();
        assert_eq!(cseq.sequence.0, 1);
        assert_eq!(cseq.method, Method::Register);

        // 验证注册状态
        let state = manager.registration_state(&reg_id).await;
        assert_eq!(state, Some(RegistrationState::Registering));
    }

    #[tokio::test]
    async fn test_register_with_registrar() {
        let manager = create_test_manager();

        let (_, request) = manager
            .register(
                "sip:alice@example.com",
                "sip:alice@192.168.1.1:5060",
                Some("sip:reg.example.com"),
            )
            .await
            .unwrap();

        // Request-URI 应为指定的注册服务器地址
        assert_eq!(
            request.request_line.request_uri.to_string(),
            "sip:reg.example.com"
        );
    }

    #[tokio::test]
    async fn test_register_expires_3600() {
        let manager = create_test_manager();

        let (_, request) = manager
            .register("sip:alice@example.com", "sip:alice@192.168.1.1:5060", None)
            .await
            .unwrap();

        // 验证 Expires: 3600
        let expires = request.headers.get(&HeaderName::Expires).unwrap();
        if let HeaderValue::Expires(val) = expires {
            assert_eq!(*val, 3600);
        } else {
            panic!("Expected Expires header");
        }
    }

    #[tokio::test]
    async fn test_unregister_expires_0() {
        let manager = create_test_manager();

        let (reg_id, _) = manager
            .register("sip:alice@example.com", "sip:alice@192.168.1.1:5060", None)
            .await
            .unwrap();

        // 模拟注册成功
        {
            let mut registrations = manager.registrations.lock().await;
            if let Some(info) = registrations.get_mut(&reg_id) {
                info.state = RegistrationState::Registered;
                info.registered_at = Some(std::time::Instant::now());
            }
        }

        // 发起注销
        let request = manager.unregister(&reg_id).await.unwrap();

        // 验证 Expires 为 0
        let expires = request.headers.get(&HeaderName::Expires).unwrap();
        if let HeaderValue::Expires(val) = expires {
            assert_eq!(*val, 0);
        } else {
            panic!("Expected Expires header");
        }

        // 验证 CSeq 递增
        let cseq = request
            .headers
            .get(&HeaderName::CSeq)
            .unwrap()
            .as_cseq()
            .unwrap();
        assert_eq!(cseq.sequence.0, 2); // 从 1 递增到 2
    }

    #[tokio::test]
    async fn test_cseq_increment_on_reregister() {
        let manager = create_test_manager();

        let (reg_id, first_request) = manager
            .register("sip:alice@example.com", "sip:alice@192.168.1.1:5060", None)
            .await
            .unwrap();

        // 第一次注册 CSeq = 1
        let cseq1 = first_request
            .headers
            .get(&HeaderName::CSeq)
            .unwrap()
            .as_cseq()
            .unwrap();
        assert_eq!(cseq1.sequence.0, 1);

        // 模拟注册成功
        {
            let mut registrations = manager.registrations.lock().await;
            if let Some(info) = registrations.get_mut(&reg_id) {
                info.state = RegistrationState::Registered;
                info.registered_at = Some(std::time::Instant::now());
            }
        }

        // 刷新注册
        let refresh_request = manager.refresh(&reg_id).await.unwrap();

        // 刷新注册 CSeq = 2
        let cseq2 = refresh_request
            .headers
            .get(&HeaderName::CSeq)
            .unwrap()
            .as_cseq()
            .unwrap();
        assert_eq!(cseq2.sequence.0, 2);
    }

    #[tokio::test]
    async fn test_third_party_register() {
        let manager = create_test_manager();

        let (_, request) = manager
            .register_third_party(
                "sip:carol@example.com",
                "sip:carol@192.168.1.1:5060",
                "sip:alice@example.com",
                None,
            )
            .await
            .unwrap();

        // To 头部为被注册者 AOR
        let to = request
            .headers
            .get(&HeaderName::To)
            .unwrap()
            .as_from_to()
            .unwrap();
        assert_eq!(to.uri.to_string(), "sip:carol@example.com");

        // From 头部为注册者地址
        let from = request
            .headers
            .get(&HeaderName::From)
            .unwrap()
            .as_from_to()
            .unwrap();
        assert_eq!(from.uri.to_string(), "sip:alice@example.com");
    }

    #[tokio::test]
    async fn test_handle_200_ok() {
        let manager = create_test_manager();

        let (reg_id, _) = manager
            .register("sip:alice@example.com", "sip:alice@192.168.1.1:5060", None)
            .await
            .unwrap();

        // 构建 200 OK 响应
        let response = SipResponse {
            status_line: siprs_message::StatusLine {
                version: SipVersion,
                status_code: StatusCode::OK,
                reason_phrase: "OK".to_string(),
            },
            headers: HeaderCollection::new(),
            body: None,
        };

        let result = manager.handle_response(&reg_id, &response).await.unwrap();
        assert!(result.is_none());

        // 验证注册状态为 Registered
        let state = manager.registration_state(&reg_id).await;
        assert_eq!(state, Some(RegistrationState::Registered));
    }

    #[tokio::test]
    async fn test_handle_401_auth_challenge() {
        let manager = create_test_manager_with_credentials();

        let (reg_id, _) = manager
            .register("sip:alice@example.com", "sip:alice@192.168.1.1:5060", None)
            .await
            .unwrap();

        // 构建 401 响应
        let challenge = AuthHeader::digest()
            .with_realm("example.com")
            .with_nonce("testnonce")
            .with_algorithm("MD5");

        let mut headers = HeaderCollection::new();
        headers.insert(HeaderName::WwwAuthenticate, HeaderValue::Auth(challenge));

        let response = SipResponse {
            status_line: siprs_message::StatusLine {
                version: SipVersion,
                status_code: StatusCode::UNAUTHORIZED,
                reason_phrase: "Unauthorized".to_string(),
            },
            headers,
            body: None,
        };

        // 处理 401 响应
        let result = manager.handle_response(&reg_id, &response).await.unwrap();

        // 应返回带 Authorization 头部的新 REGISTER 请求
        assert!(result.is_some());
        let new_request = result.unwrap();

        // 验证 Authorization 头部存在
        let auth = new_request
            .headers
            .get(&HeaderName::Authorization)
            .and_then(|v| v.as_auth());
        assert!(auth.is_some());

        // 验证 CSeq 递增
        let cseq = new_request
            .headers
            .get(&HeaderName::CSeq)
            .unwrap()
            .as_cseq()
            .unwrap();
        assert_eq!(cseq.sequence.0, 2);

        // 验证 Call-ID 不变
        let call_id = new_request
            .headers
            .get(&HeaderName::CallId)
            .unwrap()
            .as_call_id()
            .unwrap();
        let info = manager.registration_info(&reg_id).await.unwrap();
        assert_eq!(*call_id, info.call_id);
    }

    #[tokio::test]
    async fn test_auth_retry_stops_on_second_401() {
        let manager = create_test_manager_with_credentials();

        let (reg_id, _) = manager
            .register("sip:alice@example.com", "sip:alice@192.168.1.1:5060", None)
            .await
            .unwrap();

        // 第一次 401
        let challenge = AuthHeader::digest()
            .with_realm("example.com")
            .with_nonce("testnonce")
            .with_algorithm("MD5");

        let mut headers = HeaderCollection::new();
        headers.insert(HeaderName::WwwAuthenticate, HeaderValue::Auth(challenge));

        let response = SipResponse {
            status_line: siprs_message::StatusLine {
                version: SipVersion,
                status_code: StatusCode::UNAUTHORIZED,
                reason_phrase: "Unauthorized".to_string(),
            },
            headers,
            body: None,
        };

        let result = manager.handle_response(&reg_id, &response).await.unwrap();
        assert!(result.is_some()); // 第一次 401，返回重试请求

        // 第二次 401（同样的挑战）
        let challenge2 = AuthHeader::digest()
            .with_realm("example.com")
            .with_nonce("testnonce2")
            .with_algorithm("MD5");

        let mut headers2 = HeaderCollection::new();
        headers2.insert(HeaderName::WwwAuthenticate, HeaderValue::Auth(challenge2));

        let response2 = SipResponse {
            status_line: siprs_message::StatusLine {
                version: SipVersion,
                status_code: StatusCode::UNAUTHORIZED,
                reason_phrase: "Unauthorized".to_string(),
            },
            headers: headers2,
            body: None,
        };

        let result2 = manager.handle_response(&reg_id, &response2).await.unwrap();
        assert!(result2.is_none()); // 第二次 401，停止重试

        // 验证状态为 Unregistered
        let state = manager.registration_state(&reg_id).await;
        assert_eq!(state, Some(RegistrationState::Unregistered));
    }

    #[tokio::test]
    async fn test_handle_423_interval_too_brief() {
        let manager = create_test_manager();

        let (reg_id, _) = manager
            .register("sip:alice@example.com", "sip:alice@192.168.1.1:5060", None)
            .await
            .unwrap();

        // 构建 423 响应
        let mut headers = HeaderCollection::new();
        headers.insert(HeaderName::MinExpires, HeaderValue::Raw("7200".to_string()));

        let response = SipResponse {
            status_line: siprs_message::StatusLine {
                version: SipVersion,
                status_code: StatusCode::INTERVAL_TOO_BRIEF,
                reason_phrase: "Interval Too Brief".to_string(),
            },
            headers,
            body: None,
        };

        let result = manager.handle_response(&reg_id, &response).await.unwrap();

        // 应返回带新 expires 的 REGISTER 请求
        assert!(result.is_some());
        let new_request = result.unwrap();

        // 验证 Expires 使用了 Min-Expires 的值
        let expires = new_request.headers.get(&HeaderName::Expires).unwrap();
        if let HeaderValue::Expires(val) = expires {
            assert_eq!(*val, 7200);
        } else {
            panic!("Expected Expires header");
        }
    }

    #[tokio::test]
    async fn test_same_call_id_for_same_aor() {
        let manager = create_test_manager();

        let (reg_id, first_request) = manager
            .register("sip:alice@example.com", "sip:alice@192.168.1.1:5060", None)
            .await
            .unwrap();

        let first_call_id = first_request
            .headers
            .get(&HeaderName::CallId)
            .unwrap()
            .as_call_id()
            .unwrap()
            .clone();

        // 模拟注册成功
        {
            let mut registrations = manager.registrations.lock().await;
            if let Some(info) = registrations.get_mut(&reg_id) {
                info.state = RegistrationState::Registered;
                info.registered_at = Some(std::time::Instant::now());
            }
        }

        // 刷新注册
        let refresh_request = manager.refresh(&reg_id).await.unwrap();

        let second_call_id = refresh_request
            .headers
            .get(&HeaderName::CallId)
            .unwrap()
            .as_call_id()
            .unwrap()
            .clone();

        // 同一 AOR 的所有注册使用相同的 Call-ID
        assert_eq!(first_call_id, second_call_id);
    }

    #[tokio::test]
    async fn test_contact_header_contains_transport() {
        let manager = create_test_manager();

        let (_, request) = manager
            .register(
                "sip:alice@example.com",
                "sip:alice@192.168.1.1:5060;transport=udp",
                None,
            )
            .await
            .unwrap();

        // 验证 Contact 头部包含联系地址
        let contact = request
            .headers
            .get(&HeaderName::Contact)
            .unwrap()
            .as_contact()
            .unwrap();
        assert!(contact.uri.to_string().contains("192.168.1.1:5060"));
    }

    #[tokio::test]
    async fn test_unregister_invalid_state() {
        let manager = create_test_manager();

        let (reg_id, _) = manager
            .register("sip:alice@example.com", "sip:alice@192.168.1.1:5060", None)
            .await
            .unwrap();

        // 尝试在 Registering 状态下注销（应该失败）
        let result = manager.unregister(&reg_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_refresh_invalid_state() {
        let manager = create_test_manager();

        let (reg_id, _) = manager
            .register("sip:alice@example.com", "sip:alice@192.168.1.1:5060", None)
            .await
            .unwrap();

        // 尝试在 Registering 状态下刷新（应该失败）
        let result = manager.refresh(&reg_id).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_domain_from_aor() {
        // 域名 AOR：移除用户部分
        assert_eq!(
            extract_domain_from_aor("sip:alice@example.com"),
            "sip:example.com"
        );
        // IP 地址 AOR：也移除用户部分，保留端口
        assert_eq!(
            extract_domain_from_aor("sip:bob@192.168.1.1:5060"),
            "sip:192.168.1.1:5060"
        );
    }

    #[tokio::test]
    async fn test_registration_not_found() {
        let manager = create_test_manager();

        let result = manager
            .unregister(&RegistrationId("nonexistent".to_string()))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_check_refresh_needed() {
        let manager = create_test_manager();

        let (reg_id, _) = manager
            .register("sip:alice@example.com", "sip:alice@192.168.1.1:5060", None)
            .await
            .unwrap();

        // 模拟注册成功
        {
            let mut registrations = manager.registrations.lock().await;
            if let Some(info) = registrations.get_mut(&reg_id) {
                info.state = RegistrationState::Registered;
                info.registered_at = Some(std::time::Instant::now());
            }
        }

        // 刚注册，不需要刷新
        let needs_refresh = manager.check_refresh_needed().await;
        assert!(needs_refresh.is_empty());
    }
}
