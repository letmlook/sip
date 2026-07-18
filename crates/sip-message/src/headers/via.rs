//! Via 头部类型定义与解析
//!
//! Via 头部记录 SIP 消息经过的跳数路径，每经过一个跳点添加一个 Via 值，
//! 用于响应消息的路由回传。

use std::fmt;
use std::str::FromStr;

use sip_core::{Host, ParseError, TransportProtocol};

use crate::types::BranchId;

// ============================================================================
// SentBy - Via 头部的 sent-by 组件
// ============================================================================

/// Via 头部的 sent-by 组件
///
/// 格式为 `host[:port]`，标识发送该消息的跳点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentBy {
    /// 主机地址
    pub host: Host,
    /// 端口号（可选）
    pub port: Option<u16>,
}

impl SentBy {
    /// 创建新的 SentBy
    pub fn new(host: Host, port: Option<u16>) -> Self {
        Self { host, port }
    }
}

impl fmt::Display for SentBy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.host)?;
        if let Some(port) = self.port {
            write!(f, ":{}", port)?;
        }
        Ok(())
    }
}

// ============================================================================
// ViaHeader - Via 头部
// ============================================================================

/// Via 头部
///
/// 格式：`SIP/2.0/TRANSPORT sent-by [;branch=xxx] [;received=xxx] [;rport[=port]]`
///
/// Via 头部记录消息经过的路径，每个代理在转发请求时添加一个 Via 值，
/// 响应消息按照 Via 路径反向路由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViaHeader {
    /// SIP 版本号（编译期保证为 SIP/2.0，零大小类型）
    pub version: sip_core::SipVersion,
    /// 传输协议
    pub transport: TransportProtocol,
    /// sent-by 组件（host:port）
    pub sent_by: SentBy,
    /// 分支参数（必须以 z9hG4bK 开头）
    pub branch: BranchId,
    /// received 参数（实际源 IP 地址）
    pub received: Option<String>,
    /// rport 参数（实际源端口号）
    pub rport: Option<u16>,
}

impl ViaHeader {
    /// 创建新的 Via 头部
    ///
    /// 自动生成以 `z9hG4bK` 开头的分支标识。
    pub fn new(transport: TransportProtocol, host: Host, port: Option<u16>) -> Self {
        Self {
            version: sip_core::SipVersion,
            transport,
            sent_by: SentBy::new(host, port),
            branch: BranchId::new(),
            received: None,
            rport: None,
        }
    }

    /// 设置 received 参数
    pub fn with_received(mut self, received: impl Into<String>) -> Self {
        self.received = Some(received.into());
        self
    }

    /// 设置 rport 参数
    pub fn with_rport(mut self, rport: u16) -> Self {
        self.rport = Some(rport);
        self
    }
}

impl fmt::Display for ViaHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{} {}", self.version, self.transport, self.sent_by)?;
        write!(f, ";branch={}", self.branch)?;
        if let Some(ref received) = self.received {
            write!(f, ";received={}", received)?;
        }
        if let Some(rport) = self.rport {
            let _ = write!(f, ";rport={}", rport);
        }
        Ok(())
    }
}

impl FromStr for ViaHeader {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();

        // 解析格式：SIP/2.0/TRANSPORT host[:port] [;branch=xxx] [;received=xxx] [;rport[=port]]
        // 查找传输协议前的最后一个 /（即 SIP/2.0/UDP 中的第二个 /）
        let space_pos = s.find(' ').ok_or_else(|| ParseError::InvalidHeader {
            name: "Via".to_string(),
            detail: "missing space after transport".to_string(),
        })?;

        let protocol_part = &s[..space_pos];
        let rest = s[space_pos + 1..].trim();

        // 从 protocol_part 中提取 version 和 transport
        // 格式：SIP/2.0/UDP
        let last_slash = protocol_part
            .rfind('/')
            .ok_or_else(|| ParseError::InvalidHeader {
                name: "Via".to_string(),
                detail: "missing '/' before transport".to_string(),
            })?;

        let version_str = &protocol_part[..last_slash];
        // 验证 SIP 版本号
        if version_str != sip_core::SipVersion::VERSION {
            return Err(ParseError::InvalidVersion {
                version: version_str.to_string(),
            });
        }
        let transport_str = &protocol_part[last_slash + 1..];
        let transport: TransportProtocol =
            transport_str
                .parse()
                .map_err(|e: String| ParseError::InvalidHeader {
                    name: "Via".to_string(),
                    detail: e,
                })?;

        // 分离 sent-by 和参数
        let (sent_by_str, params_str) = split_via_params(rest);

        // 解析 sent-by
        let sent_by = parse_sent_by(sent_by_str)?;

        // 解析参数
        let mut branch = None;
        let mut received = None;
        let mut rport = None;

        if let Some(params) = params_str {
            for param in params.split(';') {
                let param = param.trim();
                if param.is_empty() {
                    continue;
                }
                if let Some(eq_pos) = param.find('=') {
                    let key = &param[..eq_pos];
                    let value = &param[eq_pos + 1..];
                    match key.to_lowercase().as_str() {
                        "branch" => branch = Some(BranchId(value.to_string())),
                        "received" => received = Some(value.to_string()),
                        "rport" => {
                            rport = Some(value.parse::<u16>().map_err(|_| {
                                ParseError::InvalidHeader {
                                    name: "Via".to_string(),
                                    detail: format!("invalid rport value: {}", value),
                                }
                            })?)
                        }
                        _ => {} // 忽略未知参数
                    }
                } else {
                    // 无值参数
                    if param.eq_ignore_ascii_case("rport") {
                        rport = Some(0); // rport 无值时标记为 0
                    }
                }
            }
        }

        let branch = branch.ok_or_else(|| ParseError::InvalidHeader {
            name: "Via".to_string(),
            detail: "missing branch parameter".to_string(),
        })?;

        Ok(Self {
            version: sip_core::SipVersion,
            transport,
            sent_by,
            branch,
            received,
            rport: rport.and_then(|p| if p == 0 { None } else { Some(p) }),
        })
    }
}

/// 分离 sent-by 和参数部分
fn split_via_params(s: &str) -> (&str, Option<&str>) {
    if let Some(pos) = s.find(';') {
        (&s[..pos], Some(&s[pos + 1..]))
    } else {
        (s, None)
    }
}

/// 解析 sent-by（host[:port]）
fn parse_sent_by(s: &str) -> Result<SentBy, ParseError> {
    if s.starts_with('[') {
        // IPv6 地址
        let bracket_end = s.find(']').ok_or_else(|| ParseError::InvalidHeader {
            name: "Via".to_string(),
            detail: "unclosed IPv6 bracket in sent-by".to_string(),
        })?;
        let ipv6_str = &s[1..bracket_end];
        let host: Host =
            format!("[{}]", ipv6_str)
                .parse()
                .map_err(|_| ParseError::InvalidHeader {
                    name: "Via".to_string(),
                    detail: format!("invalid IPv6 address: {}", ipv6_str),
                })?;

        let rest = &s[bracket_end + 1..];
        let port = if let Some(port_str) = rest.strip_prefix(':') {
            Some(
                port_str
                    .parse::<u16>()
                    .map_err(|_| ParseError::InvalidHeader {
                        name: "Via".to_string(),
                        detail: format!("invalid port in sent-by: {}", port_str),
                    })?,
            )
        } else {
            None
        };

        Ok(SentBy::new(host, port))
    } else {
        // 域名或 IPv4
        if let Some(colon_pos) = s.rfind(':') {
            let host_str = &s[..colon_pos];
            let port_str = &s[colon_pos + 1..];
            let host: Host = host_str.parse().map_err(|_| ParseError::InvalidHeader {
                name: "Via".to_string(),
                detail: format!("invalid host in sent-by: {}", host_str),
            })?;
            let port = port_str
                .parse::<u16>()
                .map_err(|_| ParseError::InvalidHeader {
                    name: "Via".to_string(),
                    detail: format!("invalid port in sent-by: {}", port_str),
                })?;
            Ok(SentBy::new(host, Some(port)))
        } else {
            let host: Host = s.parse().map_err(|_| ParseError::InvalidHeader {
                name: "Via".to_string(),
                detail: format!("invalid host in sent-by: {}", s),
            })?;
            Ok(SentBy::new(host, None))
        }
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_via_header_new() {
        let via = ViaHeader::new(
            TransportProtocol::Udp,
            Host::Domain("example.com".to_string()),
            Some(5060),
        );
        assert_eq!(via.version, sip_core::SipVersion);
        assert_eq!(via.transport, TransportProtocol::Udp);
        assert!(via.branch.is_valid());
        assert!(via.received.is_none());
        assert!(via.rport.is_none());
    }

    #[test]
    fn test_via_header_display() {
        let via = ViaHeader::new(
            TransportProtocol::Udp,
            Host::Domain("example.com".to_string()),
            Some(5060),
        );
        let s = via.to_string();
        assert!(s.starts_with("SIP/2.0/UDP example.com:5060"));
        assert!(s.contains(";branch=z9hG4bK"));
    }

    #[test]
    fn test_via_header_with_received() {
        let via = ViaHeader::new(
            TransportProtocol::Tcp,
            Host::Domain("proxy.example.com".to_string()),
            None,
        )
        .with_received("192.168.1.1");
        assert_eq!(via.received.as_ref().unwrap(), "192.168.1.1");
        let s = via.to_string();
        assert!(s.contains(";received=192.168.1.1"));
    }

    #[test]
    fn test_via_header_with_rport() {
        let via = ViaHeader::new(
            TransportProtocol::Udp,
            Host::Domain("example.com".to_string()),
            Some(5060),
        )
        .with_rport(12345);
        assert_eq!(via.rport, Some(12345));
        let s = via.to_string();
        assert!(s.contains(";rport=12345"));
    }

    #[test]
    fn test_via_header_parse() {
        let s = "SIP/2.0/UDP 192.168.1.1:5060;branch=z9hG4bK-abc123";
        let via: ViaHeader = s.parse().unwrap();
        assert_eq!(via.version, sip_core::SipVersion);
        assert_eq!(via.transport, TransportProtocol::Udp);
        assert_eq!(via.sent_by.host, Host::IPv4("192.168.1.1".parse().unwrap()));
        assert_eq!(via.sent_by.port, Some(5060));
        assert_eq!(via.branch.0, "z9hG4bK-abc123");
    }

    #[test]
    fn test_via_header_parse_with_received() {
        let s = "SIP/2.0/UDP proxy.example.com:5060;branch=z9hG4bK-xyz;received=10.0.0.1";
        let via: ViaHeader = s.parse().unwrap();
        assert_eq!(via.received.as_ref().unwrap(), "10.0.0.1");
    }

    #[test]
    fn test_via_header_parse_with_rport() {
        let s = "SIP/2.0/UDP proxy.example.com:5060;branch=z9hG4bK-xyz;rport=12345";
        let via: ViaHeader = s.parse().unwrap();
        assert_eq!(via.rport, Some(12345));
    }

    #[test]
    fn test_via_header_roundtrip() {
        let via = ViaHeader::new(
            TransportProtocol::Tcp,
            Host::Domain("proxy.example.com".to_string()),
            Some(5060),
        );
        let s = via.to_string();
        let via2: ViaHeader = s.parse().unwrap();
        assert_eq!(via.version, via2.version);
        assert_eq!(via.transport, via2.transport);
        assert_eq!(via.sent_by, via2.sent_by);
        assert_eq!(via.branch, via2.branch);
    }

    #[test]
    fn test_sent_by_display() {
        let sb = SentBy::new(Host::Domain("example.com".to_string()), Some(5060));
        assert_eq!(sb.to_string(), "example.com:5060");

        let sb_no_port = SentBy::new(Host::Domain("example.com".to_string()), None);
        assert_eq!(sb_no_port.to_string(), "example.com");
    }

    #[test]
    fn test_via_header_parse_invalid_version() {
        // Via 头部版本号必须为 SIP/2.0，其他版本应返回 InvalidVersion 错误
        let s = "SIP/1.0/UDP 192.168.1.1:5060;branch=z9hG4bK-abc123";
        let result: Result<ViaHeader, _> = s.parse();
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), ParseError::InvalidVersion { .. }),
            "ViaHeader::from_str should reject non-SIP/2.0 versions"
        );
    }

    #[test]
    fn test_via_header_version_is_zst() {
        // 验证 version 字段为零大小类型
        assert_eq!(std::mem::size_of::<sip_core::SipVersion>(), 0);
        let via = ViaHeader::new(
            TransportProtocol::Udp,
            Host::Domain("example.com".to_string()),
            Some(5060),
        );
        // version 字段不占用 ViaHeader 的内存
        assert_eq!(via.version, sip_core::SipVersion);
        assert_eq!(via.version.as_str(), "SIP/2.0");
    }
}
