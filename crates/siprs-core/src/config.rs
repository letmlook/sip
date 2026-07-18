//! SIP Core configuration types
//!
//! 定义 SIP 协议栈各层的配置类型及其默认值，默认值遵循 RFC 3261 推荐。
//! 支持 Builder 模式构建 `SipConfig`，并提供运行时校验。

use crate::error::ConfigError;
use crate::types::{TlsVersion, TransportProtocol};

/// Default SIP port (5060)
pub const DEFAULT_SIP_PORT: u16 = 5060;

/// Default SIP over TLS port (5061)
pub const DEFAULT_SIPS_PORT: u16 = 5061;

/// Default SIP over WebSocket port (8443)
pub const DEFAULT_SIP_WS_PORT: u16 = 8443;

// ============================================================================
// Credentials - 认证凭据
// ============================================================================

/// SIP 认证凭据
#[derive(Debug, Clone)]
pub struct Credentials {
    /// 用户名
    pub username: String,
    /// 密码
    pub password: String,
    /// 认证域
    pub realm: Option<String>,
}

// ============================================================================
// TransportConfig - 传输层配置
// ============================================================================

/// 传输层配置
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// 是否启用 UDP
    pub udp_enabled: bool,
    /// 是否启用 TCP
    pub tcp_enabled: bool,
    /// 是否启用 TLS
    pub tls_enabled: bool,
    /// TCP/TLS 连接空闲超时（秒）
    pub connection_idle_timeout: u64,
    /// 最大消息大小（字节）
    pub max_message_size: usize,
    /// UDP MTU 限制（字节），超过此大小自动切换 TCP
    pub udp_mtu_limit: usize,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            udp_enabled: true,
            tcp_enabled: true,
            tls_enabled: true,
            connection_idle_timeout: 30,
            max_message_size: 65535,
            udp_mtu_limit: 1300,
        }
    }
}

// ============================================================================
// TransactionConfig - 事务层配置
// ============================================================================

/// 事务层配置
#[derive(Debug, Clone)]
pub struct TransactionConfig {
    /// T1 定时器值（毫秒），RTT 估计值，默认 500ms
    pub t1: u64,
    /// T2 定时器值（毫秒），最大重传间隔，默认 4000ms
    pub t2: u64,
    /// T4 定时器值（毫秒），消息存活时间，默认 5000ms
    pub t4: u64,
    /// 100 Trying 自动发送超时（毫秒），默认 200ms
    pub trying_timeout: u64,
}

impl Default for TransactionConfig {
    fn default() -> Self {
        Self {
            t1: 500,
            t2: 4000,
            t4: 5000,
            trying_timeout: 200,
        }
    }
}

// ============================================================================
// TlsConfig - TLS 配置
// ============================================================================

/// TLS 配置
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// 证书路径（PEM 格式）
    pub cert_path: Option<String>,
    /// CA 证书路径
    pub ca_path: Option<String>,
    /// 私钥路径
    pub key_path: Option<String>,
    /// 是否验证服务端证书
    pub verify_certificate: bool,
    /// 最小 TLS 版本
    pub min_tls_version: TlsVersion,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            cert_path: None,
            ca_path: None,
            key_path: None,
            verify_certificate: true,
            min_tls_version: TlsVersion::Tls12,
        }
    }
}

// ============================================================================
// RegistrationConfig - 注册配置
// ============================================================================

/// 注册配置
#[derive(Debug, Clone)]
pub struct RegistrationConfig {
    /// 注册服务器地址
    pub registrar_server: Option<String>,
    /// 默认注册有效期（秒）
    pub default_expires: u64,
    /// 注册刷新阈值（剩余有效期百分比），默认 50%
    pub refresh_threshold: f32,
    /// 注册失败重试间隔（秒）
    pub retry_interval: u64,
    /// 最大重试次数
    pub max_retries: u32,
}

impl Default for RegistrationConfig {
    fn default() -> Self {
        Self {
            registrar_server: None,
            default_expires: 3600,
            refresh_threshold: 0.5,
            retry_interval: 30,
            max_retries: 3,
        }
    }
}

// ============================================================================
// SipConfig - SIP 全局配置
// ============================================================================

/// SIP 协议栈全局配置
#[derive(Debug, Clone)]
pub struct SipConfig {
    /// 本地 AOR (Address-of-Record)，格式 sip:user@domain
    pub aor: String,
    /// 本端联系地址，格式 sip:user@host:port;transport=proto
    pub contact: String,
    /// 出站代理地址
    pub outbound_proxy: Option<String>,
    /// 注册服务器地址
    pub registrar_server: Option<String>,
    /// 默认传输协议
    pub transport: TransportProtocol,
    /// 认证凭据
    pub credentials: Option<Credentials>,
    /// SIP 绑定 IP 地址，默认 "0.0.0.0"，支持 IPv6（如 "::"）
    pub bind_ip: String,
    /// SIP 监听端口
    pub sip_port: u16,
    /// 传输层配置
    pub transport_config: TransportConfig,
    /// 事务层配置
    pub transaction_config: TransactionConfig,
    /// TLS 配置
    pub tls_config: TlsConfig,
    /// 注册配置
    pub registration_config: RegistrationConfig,
}

impl SipConfig {
    /// 创建 Builder 实例用于构建 SipConfig
    pub fn builder() -> SipConfigBuilder {
        SipConfigBuilder::default()
    }
}

// ============================================================================
// SipConfigBuilder - Builder 模式
// ============================================================================

/// SipConfig 的 Builder，用于流式构建配置
#[derive(Debug, Clone, Default)]
pub struct SipConfigBuilder {
    aor: Option<String>,
    contact: Option<String>,
    outbound_proxy: Option<String>,
    registrar_server: Option<String>,
    transport: Option<TransportProtocol>,
    credentials: Option<Credentials>,
    bind_ip: Option<String>,
    sip_port: Option<u16>,
    transport_config: Option<TransportConfig>,
    transaction_config: Option<TransactionConfig>,
    tls_config: Option<TlsConfig>,
    registration_config: Option<RegistrationConfig>,
}

impl SipConfigBuilder {
    /// 设置本地 AOR (Address-of-Record)
    pub fn aor(mut self, aor: impl Into<String>) -> Self {
        self.aor = Some(aor.into());
        self
    }

    /// 设置本端联系地址
    pub fn contact(mut self, contact: impl Into<String>) -> Self {
        self.contact = Some(contact.into());
        self
    }

    /// 设置出站代理地址
    pub fn outbound_proxy(mut self, proxy: impl Into<String>) -> Self {
        self.outbound_proxy = Some(proxy.into());
        self
    }

    /// 设置注册服务器地址
    pub fn registrar_server(mut self, server: impl Into<String>) -> Self {
        self.registrar_server = Some(server.into());
        self
    }

    /// 设置默认传输协议
    pub fn transport(mut self, transport: TransportProtocol) -> Self {
        self.transport = Some(transport);
        self
    }

    /// 设置认证凭据
    pub fn credentials(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.credentials = Some(Credentials {
            username: username.into(),
            password: password.into(),
            realm: None,
        });
        self
    }

    /// 设置 SIP 绑定 IP 地址
    ///
    /// 默认为 "0.0.0.0"，可设置为 "::" 以支持 IPv6。
    pub fn bind_ip(mut self, ip: impl Into<String>) -> Self {
        self.bind_ip = Some(ip.into());
        self
    }

    /// 设置 SIP 监听端口
    pub fn sip_port(mut self, port: u16) -> Self {
        self.sip_port = Some(port);
        self
    }

    /// 设置传输层配置
    pub fn transport_config(mut self, config: TransportConfig) -> Self {
        self.transport_config = Some(config);
        self
    }

    /// 设置事务层配置
    pub fn transaction_config(mut self, config: TransactionConfig) -> Self {
        self.transaction_config = Some(config);
        self
    }

    /// 设置 TLS 配置
    pub fn tls_config(mut self, config: TlsConfig) -> Self {
        self.tls_config = Some(config);
        self
    }

    /// 设置注册配置
    pub fn registration_config(mut self, config: RegistrationConfig) -> Self {
        self.registration_config = Some(config);
        self
    }

    /// 构建 SipConfig，验证必填字段
    ///
    /// # Errors
    ///
    /// 当 `aor` 或 `contact` 未设置时返回 `ConfigError::MissingField`
    pub fn build(self) -> Result<SipConfig, ConfigError> {
        let aor = self.aor.ok_or_else(|| ConfigError::MissingField {
            field: "aor".into(),
        })?;

        let contact = self.contact.ok_or_else(|| ConfigError::MissingField {
            field: "contact".into(),
        })?;

        Ok(SipConfig {
            aor,
            contact,
            outbound_proxy: self.outbound_proxy,
            registrar_server: self.registrar_server,
            transport: self.transport.unwrap_or(TransportProtocol::Udp),
            credentials: self.credentials,
            bind_ip: self.bind_ip.unwrap_or_else(|| "0.0.0.0".to_string()),
            sip_port: self.sip_port.unwrap_or(DEFAULT_SIP_PORT),
            transport_config: self.transport_config.unwrap_or_default(),
            transaction_config: self.transaction_config.unwrap_or_default(),
            tls_config: self.tls_config.unwrap_or_default(),
            registration_config: self.registration_config.unwrap_or_default(),
        })
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- TransportConfig 测试 ----

    #[test]
    fn test_transport_config_default() {
        let config = TransportConfig::default();
        assert!(config.udp_enabled);
        assert!(config.tcp_enabled);
        assert!(config.tls_enabled);
        assert_eq!(config.connection_idle_timeout, 30);
        assert_eq!(config.max_message_size, 65535);
        assert_eq!(config.udp_mtu_limit, 1300);
    }

    // ---- TransactionConfig 测试 ----

    #[test]
    fn test_transaction_config_default() {
        let config = TransactionConfig::default();
        assert_eq!(config.t1, 500);
        assert_eq!(config.t2, 4000);
        assert_eq!(config.t4, 5000);
        assert_eq!(config.trying_timeout, 200);
    }

    // ---- TlsConfig 测试 ----

    #[test]
    fn test_tls_config_default() {
        let config = TlsConfig::default();
        assert!(config.cert_path.is_none());
        assert!(config.ca_path.is_none());
        assert!(config.key_path.is_none());
        assert!(config.verify_certificate);
        assert_eq!(config.min_tls_version, TlsVersion::Tls12);
    }

    // ---- RegistrationConfig 测试 ----

    #[test]
    fn test_registration_config_default() {
        let config = RegistrationConfig::default();
        assert!(config.registrar_server.is_none());
        assert_eq!(config.default_expires, 3600);
        assert!((config.refresh_threshold - 0.5).abs() < f32::EPSILON);
        assert_eq!(config.retry_interval, 30);
        assert_eq!(config.max_retries, 3);
    }

    // ---- SipConfigBuilder 测试 ----

    #[test]
    fn test_builder_missing_aor() {
        let result = SipConfig::builder()
            .contact("sip:alice@192.168.1.1:5060")
            .build();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ConfigError::MissingField { field } if field == "aor"));
    }

    #[test]
    fn test_builder_missing_contact() {
        let result = SipConfig::builder().aor("sip:alice@example.com").build();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ConfigError::MissingField { field } if field == "contact"));
    }

    #[test]
    fn test_builder_success() {
        let config = SipConfig::builder()
            .aor("sip:alice@example.com")
            .contact("sip:alice@192.168.1.1:5060")
            .build()
            .unwrap();

        assert_eq!(config.aor, "sip:alice@example.com");
        assert_eq!(config.contact, "sip:alice@192.168.1.1:5060");
        assert!(config.outbound_proxy.is_none());
        assert!(config.registrar_server.is_none());
        assert_eq!(config.transport, TransportProtocol::Udp);
        assert!(config.credentials.is_none());
        assert_eq!(config.sip_port, DEFAULT_SIP_PORT);
        assert_eq!(config.bind_ip, "0.0.0.0");
    }

    #[test]
    fn test_builder_full_config() {
        let config = SipConfig::builder()
            .aor("sip:bob@example.com")
            .contact("sip:bob@10.0.0.1:5060")
            .outbound_proxy("sip:proxy.example.com:5060")
            .registrar_server("sip:reg.example.com:5060")
            .transport(TransportProtocol::Tcp)
            .credentials("bob", "secret123")
            .bind_ip("::")
            .sip_port(5080)
            .transport_config(TransportConfig {
                udp_enabled: false,
                ..TransportConfig::default()
            })
            .transaction_config(TransactionConfig {
                t1: 1000,
                ..TransactionConfig::default()
            })
            .tls_config(TlsConfig {
                min_tls_version: TlsVersion::Tls13,
                ..TlsConfig::default()
            })
            .registration_config(RegistrationConfig {
                default_expires: 7200,
                ..RegistrationConfig::default()
            })
            .build()
            .unwrap();

        assert_eq!(config.aor, "sip:bob@example.com");
        assert_eq!(config.contact, "sip:bob@10.0.0.1:5060");
        assert_eq!(
            config.outbound_proxy.as_deref(),
            Some("sip:proxy.example.com:5060")
        );
        assert_eq!(
            config.registrar_server.as_deref(),
            Some("sip:reg.example.com:5060")
        );
        assert_eq!(config.transport, TransportProtocol::Tcp);
        assert!(config.credentials.is_some());
        let creds = config.credentials.unwrap();
        assert_eq!(creds.username, "bob");
        assert_eq!(creds.password, "secret123");
        assert!(creds.realm.is_none());
        assert_eq!(config.bind_ip, "::");
        assert_eq!(config.sip_port, 5080);
        assert!(!config.transport_config.udp_enabled);
        assert!(config.transport_config.tcp_enabled);
        assert_eq!(config.transaction_config.t1, 1000);
        assert_eq!(config.tls_config.min_tls_version, TlsVersion::Tls13);
        assert_eq!(config.registration_config.default_expires, 7200);
    }

    #[test]
    fn test_builder_default_sub_configs() {
        let config = SipConfig::builder()
            .aor("sip:alice@example.com")
            .contact("sip:alice@192.168.1.1:5060")
            .build()
            .unwrap();

        // 验证子配置使用默认值
        assert_eq!(config.transport_config.connection_idle_timeout, 30);
        assert_eq!(config.transaction_config.t1, 500);
        assert_eq!(config.tls_config.min_tls_version, TlsVersion::Tls12);
        assert_eq!(config.registration_config.default_expires, 3600);
    }

    // ---- Credentials 测试 ----

    #[test]
    fn test_credentials_fields() {
        let creds = Credentials {
            username: "alice".to_string(),
            password: "pass123".to_string(),
            realm: Some("example.com".to_string()),
        };
        assert_eq!(creds.username, "alice");
        assert_eq!(creds.password, "pass123");
        assert_eq!(creds.realm.as_deref(), Some("example.com"));
    }

    // ---- 常量测试 ----

    #[test]
    fn test_default_port_constants() {
        assert_eq!(DEFAULT_SIP_PORT, 5060);
        assert_eq!(DEFAULT_SIPS_PORT, 5061);
        assert_eq!(DEFAULT_SIP_WS_PORT, 8443);
    }
}
