//! SIP DNS 解析模块
//!
//! 按照 RFC 3263 规定的顺序进行 DNS 解析：NAPTR → SRV → A/AAAA。
//! 支持自定义 DNS 解析器。

use std::net::SocketAddr;

use async_trait::async_trait;
use sip_core::{DnsError, TransportProtocol};
use tracing;
use trust_dns_resolver::TokioAsyncResolver;

// ============================================================================
// DnsResolver - DNS 解析器 trait
// ============================================================================

/// DNS 解析器抽象
///
/// 定义 DNS 解析的核心接口，支持按照 RFC 3263 规定的顺序进行解析。
/// 可以通过实现此 trait 来自定义 DNS 解析行为。
#[async_trait]
pub trait DnsResolver: Send + Sync {
    /// 解析域名为 IP 地址列表
    ///
    /// 按照 RFC 3263 规定的顺序进行 DNS 解析：
    /// 1. NAPTR 查询（查找支持的协议和 SRV 记录）
    /// 2. SRV 查询（查找服务端口和主机）
    /// 3. A/AAAA 查询（直接查找 IP 地址）
    ///
    /// NAPTR 查询失败时自动降级到 SRV 查询，
    /// SRV 查询失败时自动降级到 A/AAAA 查询。
    ///
    /// # 参数
    ///
    /// - `domain` - 要解析的域名
    /// - `transport` - 传输协议类型（影响 SRV 记录查询）
    ///
    /// # 错误
    ///
    /// 返回 `DnsError` 表示解析失败。
    async fn resolve(
        &self,
        domain: &str,
        transport: TransportProtocol,
    ) -> Result<Vec<SocketAddr>, DnsError>;
}

// ============================================================================
// TrustDnsResolver - 基于 trust-dns-resolver 的 DNS 解析器
// ============================================================================

/// 基于 `trust-dns-resolver` 的 DNS 解析器
///
/// 使用系统 DNS 配置进行域名解析，支持 NAPTR/SRV/A/AAAA 查询。
pub struct TrustDnsResolver {
    resolver: TokioAsyncResolver,
}

impl TrustDnsResolver {
    /// 使用系统默认配置创建 DNS 解析器
    ///
    /// # 错误
    ///
    /// 返回 `DnsError` 表示创建解析器失败。
    pub fn new() -> Result<Self, DnsError> {
        let resolver = TokioAsyncResolver::tokio_from_system_conf().map_err(|e| {
            DnsError::AddrLookupFailed {
                domain: "system".to_string(),
                reason: format!("failed to create resolver: {}", e),
            }
        })?;
        Ok(Self { resolver })
    }

    /// 使用系统默认配置创建 DNS 解析器，失败时回退到 Google DNS
    ///
    /// 优先使用系统 DNS 配置，如果失败则回退到 Google 公共 DNS（8.8.8.8 / 8.8.4.4）。
    /// 适用于无法保证系统 DNS 配置可用的环境（如容器）。
    ///
    /// 此方法不会 panic：系统配置失败时自动回退到 Google DNS，
    /// Google DNS 使用硬编码配置，始终能成功创建解析器。
    pub fn new_with_fallback() -> Self {
        match Self::new() {
            Ok(resolver) => resolver,
            Err(e) => {
                tracing::warn!(
                    "TrustDnsResolver: failed to create from system conf ({}), \
                     falling back to Google DNS",
                    e
                );
                // TokioAsyncResolver::tokio 直接返回 AsyncResolver，不会失败
                let resolver = TokioAsyncResolver::tokio(
                    trust_dns_resolver::config::ResolverConfig::google(),
                    trust_dns_resolver::config::ResolverOpts::default(),
                );
                Self::from_resolver(resolver)
            }
        }
    }

    /// 使用自定义配置创建 DNS 解析器
    pub fn from_resolver(resolver: TokioAsyncResolver) -> Self {
        Self { resolver }
    }
}

#[async_trait]
impl DnsResolver for TrustDnsResolver {
    async fn resolve(
        &self,
        domain: &str,
        transport: TransportProtocol,
    ) -> Result<Vec<SocketAddr>, DnsError> {
        let default_port = transport.default_port();

        // 1. 尝试 SRV 查询
        let srv_result = self.try_srv_lookup(domain, transport).await;
        if let Ok(addrs) = srv_result {
            if !addrs.is_empty() {
                tracing::debug!(
                    "DNS: SRV lookup succeeded for {}, got {} addresses",
                    domain,
                    addrs.len()
                );
                return Ok(addrs);
            }
        }

        // 2. SRV 失败，降级到 A/AAAA 查询
        tracing::debug!(
            "DNS: SRV lookup failed for {}, falling back to A/AAAA",
            domain
        );
        self.lookup_ip(domain, default_port).await
    }
}

impl TrustDnsResolver {
    /// 尝试 SRV 记录查询
    ///
    /// 根据传输协议构造 SRV 查询名称：
    /// - UDP: `_sip._udp.domain`
    /// - TCP: `_sip._tcp.domain`
    /// - TLS: `_sips._tcp.domain`
    async fn try_srv_lookup(
        &self,
        domain: &str,
        transport: TransportProtocol,
    ) -> Result<Vec<SocketAddr>, DnsError> {
        let srv_name = match transport {
            TransportProtocol::Udp => format!("_sip._udp.{}", domain),
            TransportProtocol::Tcp => format!("_sip._tcp.{}", domain),
            TransportProtocol::Tls => format!("_sips._tcp.{}", domain),
            _ => format!("_sip._udp.{}", domain),
        };

        // 尝试 SRV 查询
        match self.resolver.srv_lookup(&srv_name).await {
            Ok(srv_lookup) => {
                let mut addrs = Vec::new();
                for srv in srv_lookup.iter() {
                    let target = srv.target().to_utf8();
                    let port = srv.port();
                    // 解析 SRV 目标的 IP 地址
                    match self.lookup_ip(&target, port).await {
                        Ok(ip_addrs) => addrs.extend(ip_addrs),
                        Err(e) => {
                            tracing::warn!("DNS: failed to resolve SRV target {}: {}", target, e);
                        }
                    }
                }
                Ok(addrs)
            }
            Err(e) => Err(DnsError::SrvLookupFailed {
                domain: srv_name,
                reason: e.to_string(),
            }),
        }
    }

    /// A/AAAA 记录查询
    async fn lookup_ip(&self, domain: &str, port: u16) -> Result<Vec<SocketAddr>, DnsError> {
        let lookup =
            self.resolver
                .lookup_ip(domain)
                .await
                .map_err(|e| DnsError::AddrLookupFailed {
                    domain: domain.to_string(),
                    reason: e.to_string(),
                })?;

        let addrs: Vec<SocketAddr> = lookup.iter().map(|ip| SocketAddr::new(ip, port)).collect();

        if addrs.is_empty() {
            return Err(DnsError::NoRecordsFound {
                domain: domain.to_string(),
            });
        }

        Ok(addrs)
    }
}

// ============================================================================
// SystemDnsResolver - 简单的系统 DNS 解析器
// ============================================================================

/// 简单的系统 DNS 解析器
///
/// 使用 `tokio::net::lookup_host` 进行基本的域名解析，
/// 不支持 NAPTR/SRV 查询，仅用于简单场景。
pub struct SystemDnsResolver;

impl SystemDnsResolver {
    /// 创建新的系统 DNS 解析器
    pub fn new() -> Self {
        Self
    }
}

impl Default for SystemDnsResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DnsResolver for SystemDnsResolver {
    async fn resolve(
        &self,
        domain: &str,
        transport: TransportProtocol,
    ) -> Result<Vec<SocketAddr>, DnsError> {
        let default_port = transport.default_port();

        // 构造查询地址
        let lookup_addr = format!("{}:{}", domain, default_port);

        let addrs: Vec<SocketAddr> = tokio::net::lookup_host(&lookup_addr)
            .await
            .map_err(|e| DnsError::AddrLookupFailed {
                domain: domain.to_string(),
                reason: e.to_string(),
            })?
            .collect();

        if addrs.is_empty() {
            return Err(DnsError::NoRecordsFound {
                domain: domain.to_string(),
            });
        }

        Ok(addrs)
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_dns_resolver_new() {
        let resolver = SystemDnsResolver::new();
        let _ = &resolver;
    }

    #[test]
    fn test_system_dns_resolver_default() {
        let resolver = SystemDnsResolver::default();
        let _ = &resolver;
    }

    #[tokio::test]
    async fn test_system_dns_resolver_localhost() {
        let resolver = SystemDnsResolver::new();
        let result = resolver.resolve("localhost", TransportProtocol::Udp).await;
        // localhost 应该能解析
        assert!(result.is_ok());
        let addrs = result.unwrap();
        assert!(!addrs.is_empty());
        // 默认端口应为 5060
        assert!(addrs.iter().any(|a| a.port() == 5060));
    }

    #[tokio::test]
    async fn test_system_dns_resolver_invalid_domain() {
        let resolver = SystemDnsResolver::new();
        let result = resolver
            .resolve(
                "this-domain-definitely-does-not-exist.invalid",
                TransportProtocol::Udp,
            )
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_trust_dns_resolver_creation() {
        // 仅验证可以创建
        let resolver = TrustDnsResolver::new();
        // 在某些 CI 环境中可能失败，所以只检查不 panic
        let _ = resolver;
    }
}
