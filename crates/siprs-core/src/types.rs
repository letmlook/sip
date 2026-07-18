//! SIP Core types
//!
//! 定义 SIP 协议栈全局共享的核心 trait 和类型，包括传输协议、主机地址、
//! 协议版本、状态码等基础类型。

use std::borrow::Cow;
use std::fmt;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::str::FromStr;

use uuid::Uuid;

// ============================================================================
// TransportProtocol - SIP 传输协议类型
// ============================================================================

/// SIP 传输协议类型
///
/// 定义 SIP 支持的五种传输协议，涵盖 UDP、TCP、TLS、WebSocket 及其安全变体。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TransportProtocol {
    /// UDP 传输
    Udp,
    /// TCP 传输
    Tcp,
    /// TLS 传输
    Tls,
    /// WebSocket 传输
    Ws,
    /// WebSocket Secure 传输
    Wss,
}

impl TransportProtocol {
    /// 返回该传输协议的默认端口号
    ///
    /// - UDP/TCP: 5060
    /// - TLS/WSS: 5061
    /// - WS: 80
    pub fn default_port(&self) -> u16 {
        match self {
            Self::Udp | Self::Tcp => 5060,
            Self::Tls | Self::Wss => 5061,
            Self::Ws => 80,
        }
    }

    /// 判断该传输协议是否提供可靠传输
    ///
    /// TCP、TLS、WS、WSS 提供可靠传输，UDP 不提供。
    pub fn is_reliable(&self) -> bool {
        !matches!(self, Self::Udp)
    }

    /// 判断该传输协议是否提供安全传输（加密）
    ///
    /// TLS 和 WSS 提供加密传输，其他协议不提供。
    pub fn is_secure(&self) -> bool {
        matches!(self, Self::Tls | Self::Wss)
    }
}

impl fmt::Display for TransportProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Udp => write!(f, "UDP"),
            Self::Tcp => write!(f, "TCP"),
            Self::Tls => write!(f, "TLS"),
            Self::Ws => write!(f, "WS"),
            Self::Wss => write!(f, "WSS"),
        }
    }
}

/// 从字符串解析传输协议（大小写不敏感）
impl FromStr for TransportProtocol {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "UDP" => Ok(Self::Udp),
            "TCP" => Ok(Self::Tcp),
            "TLS" => Ok(Self::Tls),
            "WS" => Ok(Self::Ws),
            "WSS" => Ok(Self::Wss),
            other => Err(format!("unknown transport protocol: {}", other)),
        }
    }
}

// ============================================================================
// Host - 主机地址类型
// ============================================================================

/// 主机地址类型
///
/// 支持 SIP 中使用的三种主机地址形式：域名、IPv4 地址和 IPv6 地址。
/// IPv6 地址在显示时自动用方括号包裹（如 `[::1]`），符合 SIP URI 规范。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Host {
    /// 域名
    Domain(String),
    /// IPv4 地址
    IPv4(Ipv4Addr),
    /// IPv6 地址
    IPv6(Ipv6Addr),
}

impl Host {
    /// 返回地址字符串
    ///
    /// 对于域名返回借用引用，对于 IP 地址返回拥有的字符串。
    /// 注意：IPv6 地址不包含方括号，如需方括号包裹请使用 `Display` trait。
    pub fn as_str(&self) -> Cow<'_, str> {
        match self {
            Self::Domain(s) => Cow::Borrowed(s.as_str()),
            Self::IPv4(addr) => Cow::Owned(addr.to_string()),
            Self::IPv6(addr) => Cow::Owned(addr.to_string()),
        }
    }
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(s) => write!(f, "{}", s),
            Self::IPv4(addr) => write!(f, "{}", addr),
            // IPv6 地址用方括号包裹，符合 SIP URI 规范
            Self::IPv6(addr) => write!(f, "[{}]", addr),
        }
    }
}

/// 从字符串解析主机地址，自动识别 IPv4/IPv6/域名
///
/// 解析优先级：IPv4 → IPv6（需方括号包裹）→ 域名
impl FromStr for Host {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // 尝试解析为 IPv4 地址
        if let Ok(addr) = s.parse::<Ipv4Addr>() {
            return Ok(Self::IPv4(addr));
        }

        // 尝试解析为 IPv6 地址（方括号包裹）
        if s.starts_with('[') && s.ends_with(']') {
            let inner = &s[1..s.len() - 1];
            if let Ok(addr) = inner.parse::<Ipv6Addr>() {
                return Ok(Self::IPv6(addr));
            }
        }

        // 尝试解析为裸 IPv6 地址
        if let Ok(addr) = s.parse::<Ipv6Addr>() {
            return Ok(Self::IPv6(addr));
        }

        // 作为域名处理
        Ok(Self::Domain(s.to_string()))
    }
}

// ============================================================================
// SipVersion - SIP 协议版本（零大小类型）
// ============================================================================

/// SIP 协议版本（编译期保证为 SIP/2.0）
///
/// 零大小类型（ZST），在编译期就确定了 SIP 协议版本为 "SIP/2.0"，
/// 不占用任何运行时内存。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SipVersion;

impl SipVersion {
    /// 版本字符串 "SIP/2.0"
    pub const VERSION: &'static str = "SIP/2.0";

    /// 获取版本字符串
    pub fn as_str(&self) -> &'static str {
        Self::VERSION
    }
}

impl fmt::Display for SipVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", Self::VERSION)
    }
}

// ============================================================================
// TransportInfo - 传输层信息
// ============================================================================

/// 传输层信息
///
/// 描述一次 SIP 传输的协议、本地地址和远端地址。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportInfo {
    /// 传输协议
    pub protocol: TransportProtocol,
    /// 本地地址
    pub local_addr: SocketAddr,
    /// 远端地址
    pub remote_addr: Option<SocketAddr>,
}

// ============================================================================
// TlsVersion - TLS 协议版本
// ============================================================================

/// TLS 协议版本
///
/// SIP over TLS 支持的 TLS 版本，仅允许 TLS 1.2 和 TLS 1.3。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TlsVersion {
    /// TLS 1.2
    Tls12,
    /// TLS 1.3
    Tls13,
}

// ============================================================================
// 辅助类型别名
// ============================================================================

/// SIP 方法名（类型别名）
pub type Method = String;

/// Call-ID（类型别名）
pub type CallId = String;

/// Tag 参数（类型别名）
pub type Tag = String;

/// Branch ID（类型别名）
pub type BranchId = String;

// ============================================================================
// CSeqNumber - CSeq 序列号
// ============================================================================

/// CSeq 序列号
///
/// SIP CSeq 头部中的序列号部分，用于标识和排序事务。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CSeqNumber(pub u32);

// ============================================================================
// StatusCode - SIP 状态码
// ============================================================================

/// SIP 状态码
///
/// SIP 响应消息中的三位数字状态码，用于指示请求的处理结果。
/// 状态码的第一位数字定义了响应类别（1xx-6xx）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatusCode(pub u16);

impl StatusCode {
    // ---- 分类方法 ----

    /// 判断是否为临时响应（1xx）
    pub fn is_provisional(&self) -> bool {
        self.0 >= 100 && self.0 < 200
    }

    /// 判断是否为成功响应（2xx）
    pub fn is_success(&self) -> bool {
        self.0 >= 200 && self.0 < 300
    }

    /// 判断是否为重定向响应（3xx）
    pub fn is_redirect(&self) -> bool {
        self.0 >= 300 && self.0 < 400
    }

    /// 判断是否为客户端错误响应（4xx）
    pub fn is_client_error(&self) -> bool {
        self.0 >= 400 && self.0 < 500
    }

    /// 判断是否为服务端错误响应（5xx）
    pub fn is_server_error(&self) -> bool {
        self.0 >= 500 && self.0 < 600
    }

    /// 判断是否为全局故障响应（6xx）
    pub fn is_global_failure(&self) -> bool {
        self.0 >= 600 && self.0 < 700
    }

    // ---- 常用状态码常量 ----

    /// 100 Trying
    pub const TRYING: StatusCode = StatusCode(100);
    /// 180 Ringing
    pub const RINGING: StatusCode = StatusCode(180);
    /// 200 OK
    pub const OK: StatusCode = StatusCode(200);
    /// 300 Multiple Choices
    pub const MULTIPLE_CHOICES: StatusCode = StatusCode(300);
    /// 301 Moved Permanently
    pub const MOVED_PERMANENTLY: StatusCode = StatusCode(301);
    /// 302 Moved Temporarily
    pub const MOVED_TEMPORARILY: StatusCode = StatusCode(302);
    /// 400 Bad Request
    pub const BAD_REQUEST: StatusCode = StatusCode(400);
    /// 401 Unauthorized
    pub const UNAUTHORIZED: StatusCode = StatusCode(401);
    /// 403 Forbidden
    pub const FORBIDDEN: StatusCode = StatusCode(403);
    /// 404 Not Found
    pub const NOT_FOUND: StatusCode = StatusCode(404);
    /// 405 Method Not Allowed
    pub const METHOD_NOT_ALLOWED: StatusCode = StatusCode(405);
    /// 407 Proxy Authentication Required
    pub const PROXY_AUTH_REQUIRED: StatusCode = StatusCode(407);
    /// 408 Request Timeout
    pub const REQUEST_TIMEOUT: StatusCode = StatusCode(408);
    /// 423 Interval Too Brief
    pub const INTERVAL_TOO_BRIEF: StatusCode = StatusCode(423);
    /// 486 Busy Here
    pub const BUSY_HERE: StatusCode = StatusCode(486);
    /// 487 Request Terminated
    pub const REQUEST_TERMINATED: StatusCode = StatusCode(487);
    /// 491 Request Pending
    pub const REQUEST_PENDING: StatusCode = StatusCode(491);
    /// 500 Server Internal Error
    pub const SERVER_INTERNAL_ERROR: StatusCode = StatusCode(500);
    /// 503 Service Unavailable
    pub const SERVICE_UNAVAILABLE: StatusCode = StatusCode(503);
    /// 600 Busy Everywhere
    pub const BUSY_EVERYWHERE: StatusCode = StatusCode(600);
}

impl fmt::Display for StatusCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ============================================================================
// 事务和对话标识符
// ============================================================================

/// SIP 事务的唯一标识符
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransactionId(Uuid);

impl TransactionId {
    /// 创建一个随机的事务 ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TransactionId {
    fn default() -> Self {
        Self::new()
    }
}

/// SIP 对话的唯一标识符
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DialogId(Uuid);

impl DialogId {
    /// 创建一个随机的对话 ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for DialogId {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- TransportProtocol 测试 ----

    #[test]
    fn test_transport_protocol_display() {
        assert_eq!(TransportProtocol::Udp.to_string(), "UDP");
        assert_eq!(TransportProtocol::Tcp.to_string(), "TCP");
        assert_eq!(TransportProtocol::Tls.to_string(), "TLS");
        assert_eq!(TransportProtocol::Ws.to_string(), "WS");
        assert_eq!(TransportProtocol::Wss.to_string(), "WSS");
    }

    #[test]
    fn test_transport_protocol_from_str() {
        assert_eq!(
            "UDP".parse::<TransportProtocol>(),
            Ok(TransportProtocol::Udp)
        );
        assert_eq!(
            "tcp".parse::<TransportProtocol>(),
            Ok(TransportProtocol::Tcp)
        );
        assert_eq!(
            "Tls".parse::<TransportProtocol>(),
            Ok(TransportProtocol::Tls)
        );
        assert_eq!("ws".parse::<TransportProtocol>(), Ok(TransportProtocol::Ws));
        assert_eq!(
            "Wss".parse::<TransportProtocol>(),
            Ok(TransportProtocol::Wss)
        );
        assert!("invalid".parse::<TransportProtocol>().is_err());
    }

    #[test]
    fn test_transport_protocol_default_port() {
        assert_eq!(TransportProtocol::Udp.default_port(), 5060);
        assert_eq!(TransportProtocol::Tcp.default_port(), 5060);
        assert_eq!(TransportProtocol::Tls.default_port(), 5061);
        assert_eq!(TransportProtocol::Ws.default_port(), 80);
        assert_eq!(TransportProtocol::Wss.default_port(), 5061);
    }

    #[test]
    fn test_transport_protocol_is_reliable() {
        assert!(!TransportProtocol::Udp.is_reliable());
        assert!(TransportProtocol::Tcp.is_reliable());
        assert!(TransportProtocol::Tls.is_reliable());
        assert!(TransportProtocol::Ws.is_reliable());
        assert!(TransportProtocol::Wss.is_reliable());
    }

    #[test]
    fn test_transport_protocol_is_secure() {
        assert!(!TransportProtocol::Udp.is_secure());
        assert!(!TransportProtocol::Tcp.is_secure());
        assert!(TransportProtocol::Tls.is_secure());
        assert!(!TransportProtocol::Ws.is_secure());
        assert!(TransportProtocol::Wss.is_secure());
    }

    // ---- Host 测试 ----

    #[test]
    fn test_host_display() {
        assert_eq!(
            Host::Domain("example.com".to_string()).to_string(),
            "example.com"
        );
        assert_eq!(
            Host::IPv4("127.0.0.1".parse().unwrap()).to_string(),
            "127.0.0.1"
        );
        assert_eq!(Host::IPv6("::1".parse().unwrap()).to_string(), "[::1]");
        assert_eq!(
            Host::IPv6("2001:db8::1".parse().unwrap()).to_string(),
            "[2001:db8::1]"
        );
    }

    #[test]
    fn test_host_from_str_ipv4() {
        let host = "192.168.1.1".parse::<Host>().unwrap();
        assert_eq!(host, Host::IPv4("192.168.1.1".parse().unwrap()));
    }

    #[test]
    fn test_host_from_str_ipv6_with_brackets() {
        let host = "[::1]".parse::<Host>().unwrap();
        assert_eq!(host, Host::IPv6("::1".parse().unwrap()));
    }

    #[test]
    fn test_host_from_str_ipv6_without_brackets() {
        let host = "::1".parse::<Host>().unwrap();
        assert_eq!(host, Host::IPv6("::1".parse().unwrap()));
    }

    #[test]
    fn test_host_from_str_domain() {
        let host = "sip.example.com".parse::<Host>().unwrap();
        assert_eq!(host, Host::Domain("sip.example.com".to_string()));
    }

    // ---- SipVersion 测试 ----

    #[test]
    fn test_sip_version_is_zst() {
        assert_eq!(std::mem::size_of::<SipVersion>(), 0);
    }

    #[test]
    fn test_sip_version_display() {
        let v = SipVersion;
        assert_eq!(v.to_string(), "SIP/2.0");
    }

    #[test]
    fn test_sip_version_as_str() {
        assert_eq!(SipVersion.as_str(), "SIP/2.0");
        assert_eq!(SipVersion::VERSION, "SIP/2.0");
    }

    // ---- StatusCode 测试 ----

    #[test]
    fn test_status_code_classification() {
        assert!(StatusCode::TRYING.is_provisional());
        assert!(StatusCode::OK.is_success());
        assert!(StatusCode::MULTIPLE_CHOICES.is_redirect());
        assert!(StatusCode::BAD_REQUEST.is_client_error());
        assert!(StatusCode::SERVER_INTERNAL_ERROR.is_server_error());
        assert!(StatusCode::BUSY_EVERYWHERE.is_global_failure());
    }

    #[test]
    fn test_status_code_constants() {
        assert_eq!(StatusCode::TRYING.0, 100);
        assert_eq!(StatusCode::RINGING.0, 180);
        assert_eq!(StatusCode::OK.0, 200);
        assert_eq!(StatusCode::BAD_REQUEST.0, 400);
        assert_eq!(StatusCode::UNAUTHORIZED.0, 401);
        assert_eq!(StatusCode::FORBIDDEN.0, 403);
        assert_eq!(StatusCode::NOT_FOUND.0, 404);
        assert_eq!(StatusCode::SERVER_INTERNAL_ERROR.0, 500);
        assert_eq!(StatusCode::SERVICE_UNAVAILABLE.0, 503);
        assert_eq!(StatusCode::BUSY_EVERYWHERE.0, 600);
    }

    #[test]
    fn test_status_code_display() {
        assert_eq!(StatusCode::OK.to_string(), "200");
        assert_eq!(StatusCode::NOT_FOUND.to_string(), "404");
    }

    // ---- CSeqNumber 测试 ----

    #[test]
    fn test_cseq_number_ordering() {
        assert!(CSeqNumber(1) < CSeqNumber(2));
        assert_eq!(CSeqNumber(42), CSeqNumber(42));
    }

    // ---- TlsVersion 测试 ----

    #[test]
    fn test_tls_version_ordering() {
        assert!(TlsVersion::Tls12 < TlsVersion::Tls13);
    }

    // ---- TransportInfo 测试 ----

    #[test]
    fn test_transport_info_construction() {
        let info = TransportInfo {
            protocol: TransportProtocol::Tcp,
            local_addr: "127.0.0.1:5060".parse().unwrap(),
            remote_addr: Some("192.168.1.1:5060".parse().unwrap()),
        };
        assert_eq!(info.protocol, TransportProtocol::Tcp);
        assert!(info.remote_addr.is_some());
    }

    // ---- TransactionId / DialogId 测试 ----

    #[test]
    fn test_transaction_id_uniqueness() {
        let id1 = TransactionId::new();
        let id2 = TransactionId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_dialog_id_uniqueness() {
        let id1 = DialogId::new();
        let id2 = DialogId::new();
        assert_ne!(id1, id2);
    }
}
