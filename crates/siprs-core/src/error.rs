//! SIP Core error types
//!
//! 统一的错误类型体系，涵盖 SIP 协议栈各层的错误场景。
//! 顶层 `SipError` 可通过 `?` 操作符从任何子模块错误自动转换。

use thiserror::Error;

// ============================================================================
// 子错误类型
// ============================================================================

/// SIP 消息解析错误
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("invalid start line: {detail}")]
    InvalidStartLine { detail: String },

    #[error("invalid header: name={name}, detail={detail}")]
    InvalidHeader { name: String, detail: String },

    #[error("invalid URI: {detail}")]
    InvalidUri { detail: String },

    #[error("message too large: size={size}, max={max}")]
    MessageTooLarge { size: usize, max: usize },

    #[error("unexpected end of input at position {position}")]
    UnexpectedEof { position: usize },

    #[error("invalid SIP version: {version}")]
    InvalidVersion { version: String },

    #[error("invalid status code: {code}")]
    InvalidStatusCode { code: u16 },

    #[error("invalid method: {method}")]
    InvalidMethod { method: String },

    #[error("UTF-8 decode error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),
}

/// SIP 消息构建错误
#[derive(Debug, Error)]
pub enum BuildError {
    #[error("missing required header: {header}")]
    MissingHeader { header: String },

    #[error("invalid header value: name={name}, detail={detail}")]
    InvalidHeaderValue { name: String, detail: String },

    #[error("serialization failed: {detail}")]
    SerializationFailed { detail: String },
}

/// SIP 消息校验错误
#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("missing Call-ID header")]
    MissingCallId,

    #[error("missing CSeq header")]
    MissingCSeq,

    #[error("missing Via header")]
    MissingVia,

    #[error("Content-Length mismatch: declared={declared}, actual={actual}")]
    ContentLengthMismatch { declared: usize, actual: usize },

    #[error("invalid status code: {code}")]
    InvalidStatusCode { code: u16 },

    #[error("invalid CSeq format: {detail}")]
    InvalidCSeq { detail: String },

    #[error("invalid Via branch: must start with 'z9hG4bK'")]
    InvalidViaBranch,
}

/// 传输层错误
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("connection failed: {addr} - {reason}")]
    ConnectionFailed { addr: String, reason: String },

    #[error("connection closed: {addr}")]
    ConnectionClosed { addr: String },

    #[error("send failed: {reason}")]
    SendFailed { reason: String },

    #[error("receive failed: {reason}")]
    ReceiveFailed { reason: String },

    #[error("bind failed: {addr} - {reason}")]
    BindFailed { addr: String, reason: String },

    #[error("message too large for UDP: size={size}, mtu={mtu}")]
    UdpMessageTooLarge { size: usize, mtu: usize },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// DNS 解析错误
#[derive(Debug, Error)]
pub enum DnsError {
    #[error("NAPTR lookup failed: {domain} - {reason}")]
    NaptrLookupFailed { domain: String, reason: String },

    #[error("SRV lookup failed: {domain} - {reason}")]
    SrvLookupFailed { domain: String, reason: String },

    #[error("A/AAAA lookup failed: {domain} - {reason}")]
    AddrLookupFailed { domain: String, reason: String },

    #[error("no records found: {domain}")]
    NoRecordsFound { domain: String },

    #[error("timeout: {domain}")]
    Timeout { domain: String },
}

/// TLS 错误
#[derive(Debug, Error)]
pub enum TlsError {
    #[error("handshake failed: {reason}")]
    HandshakeFailed { reason: String },

    #[error("certificate verification failed: {reason}")]
    CertificateVerification { reason: String },

    #[error("unsupported TLS version: {version:?}")]
    UnsupportedVersion { version: String },

    #[error("no certificate configured")]
    NoCertificate,
}

/// 事务层错误
#[derive(Debug, Error)]
pub enum TransactionError {
    #[error("transaction not found: {id}")]
    NotFound { id: String },

    #[error("transaction timeout: {id}")]
    Timeout { id: String },

    #[error("transaction already exists: {id}")]
    AlreadyExists { id: String },

    #[error("invalid state transition: from={from}, to={to}, transaction={id}")]
    InvalidStateTransition {
        from: String,
        to: String,
        id: String,
    },

    #[error("ACK construction failed: {reason}")]
    AckConstructionFailed { reason: String },
}

/// 对话层错误
#[derive(Debug, Error)]
pub enum DialogError {
    #[error("dialog not found: {id}")]
    NotFound { id: String },

    #[error("dialog already exists: {id}")]
    AlreadyExists { id: String },

    #[error("invalid dialog state: {detail}")]
    InvalidState { detail: String },

    #[error("sequence number conflict: local={local}, remote={remote}")]
    SequenceConflict { local: u32, remote: u32 },
}

/// 注册错误
#[derive(Debug, Error)]
pub enum RegistrationError {
    #[error("registration not found: {id}")]
    NotFound { id: String },

    #[error("registration failed: {reason}")]
    RegistrationFailed { reason: String },

    #[error("authentication failed: {reason}")]
    AuthenticationFailed { reason: String },

    #[error("registration expired: {aor}")]
    Expired { aor: String },

    #[error("invalid state: current={current}, expected={expected}")]
    InvalidState { current: String, expected: String },
}

/// 配置错误
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required field: {field}")]
    MissingField { field: String },

    #[error("invalid value: field={field}, value={value}, reason={reason}")]
    InvalidValue {
        field: String,
        value: String,
        reason: String,
    },
}

// ============================================================================
// 顶层错误类型
// ============================================================================

/// SIP 协议栈顶层错误类型
///
/// 所有子模块错误均可通过 `?` 操作符自动转换为 `SipError`，
/// 便于在跨层调用时统一错误处理。
#[derive(Debug, Error)]
pub enum SipError {
    #[error("parse error: {0}")]
    Parse(#[from] ParseError),

    #[error("build error: {0}")]
    Build(#[from] BuildError),

    #[error("validation error: {0}")]
    Validation(#[from] ValidationError),

    #[error("transport error: {0}")]
    Transport(#[from] TransportError),

    #[error("DNS error: {0}")]
    Dns(#[from] DnsError),

    #[error("TLS error: {0}")]
    Tls(#[from] TlsError),

    #[error("transaction error: {0}")]
    Transaction(#[from] TransactionError),

    #[error("dialog error: {0}")]
    Dialog(#[from] DialogError),

    #[error("registration error: {0}")]
    Registration(#[from] RegistrationError),

    #[error("configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("timeout: {0}")]
    Timeout(String),

    #[error("engine error: {0}")]
    Engine(String),
}

/// 允许从 `String` 创建 `SipError::Engine`
impl From<String> for SipError {
    fn from(s: String) -> Self {
        Self::Engine(s)
    }
}

/// 允许将 `SipError` 转换为 `String`（用于兼容返回 `Result<_, String>` 的旧接口）
impl From<SipError> for String {
    fn from(e: SipError) -> Self {
        e.to_string()
    }
}

/// Result 类型别名，用于 SIP 操作的统一返回类型
pub type Result<T> = std::result::Result<T, SipError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(invalid_from_utf8)]
    fn test_parse_error_from_utf8() {
        let invalid_bytes: &[u8] = &[0xFE, 0xFF]; // invalid UTF-8
        let utf8_err = std::str::from_utf8(invalid_bytes).unwrap_err();

        // Utf8Error -> ParseError -> SipError 自动转换
        let parse_err: ParseError = utf8_err.into();
        let sip_err: SipError = parse_err.into();
        assert!(matches!(sip_err, SipError::Parse(_)));
    }

    #[test]
    fn test_transport_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");

        // io::Error -> TransportError -> SipError 自动转换
        let transport_err: TransportError = io_err.into();
        let sip_err: SipError = transport_err.into();
        assert!(matches!(sip_err, SipError::Transport(_)));
    }

    #[test]
    fn test_question_mark_operator() {
        fn inner_parse() -> Result<()> {
            let err = ParseError::InvalidStartLine {
                detail: "bad line".into(),
            };
            Err(err)?; // ParseError -> SipError via ?
            Ok(())
        }

        let result = inner_parse();
        assert!(matches!(
            result,
            Err(SipError::Parse(ParseError::InvalidStartLine { .. }))
        ));
    }

    #[test]
    fn test_error_display_messages() {
        // ParseError
        let err = ParseError::InvalidHeader {
            name: "Via".into(),
            detail: "missing branch".into(),
        };
        assert_eq!(
            format!("{err}"),
            "invalid header: name=Via, detail=missing branch"
        );

        // ValidationError
        let err = ValidationError::ContentLengthMismatch {
            declared: 100,
            actual: 50,
        };
        assert_eq!(
            format!("{err}"),
            "Content-Length mismatch: declared=100, actual=50"
        );

        // TransportError
        let err = TransportError::ConnectionFailed {
            addr: "192.168.1.1:5060".into(),
            reason: "timeout".into(),
        };
        assert_eq!(
            format!("{err}"),
            "connection failed: 192.168.1.1:5060 - timeout"
        );

        // DnsError
        let err = DnsError::SrvLookupFailed {
            domain: "sip.example.com".into(),
            reason: "NXDOMAIN".into(),
        };
        assert_eq!(
            format!("{err}"),
            "SRV lookup failed: sip.example.com - NXDOMAIN"
        );

        // TlsError
        let err = TlsError::HandshakeFailed {
            reason: "cert expired".into(),
        };
        assert_eq!(format!("{err}"), "handshake failed: cert expired");

        // TransactionError
        let err = TransactionError::InvalidStateTransition {
            from: "Proceeding".into(),
            to: "Confirmed".into(),
            id: "tx-123".into(),
        };
        assert_eq!(
            format!("{err}"),
            "invalid state transition: from=Proceeding, to=Confirmed, transaction=tx-123"
        );

        // DialogError
        let err = DialogError::SequenceConflict {
            local: 1,
            remote: 2,
        };
        assert_eq!(
            format!("{err}"),
            "sequence number conflict: local=1, remote=2"
        );

        // RegistrationError
        let err = RegistrationError::Expired {
            aor: "sip:user@example.com".into(),
        };
        assert_eq!(
            format!("{err}"),
            "registration expired: sip:user@example.com"
        );

        // ConfigError
        let err = ConfigError::InvalidValue {
            field: "port".into(),
            value: "-1".into(),
            reason: "must be positive".into(),
        };
        assert_eq!(
            format!("{err}"),
            "invalid value: field=port, value=-1, reason=must be positive"
        );

        // BuildError
        let err = BuildError::MissingHeader {
            header: "Call-ID".into(),
        };
        assert_eq!(format!("{err}"), "missing required header: Call-ID");
    }

    #[test]
    fn test_sip_error_engine_variant() {
        let err = SipError::Engine("something went wrong".into());
        assert_eq!(format!("{err}"), "engine error: something went wrong");
    }

    #[test]
    fn test_all_from_conversions() {
        // 验证所有子错误类型都能通过 From 转换为 SipError
        let _: SipError = ParseError::InvalidMethod {
            method: "FOO".into(),
        }
        .into();
        let _: SipError = BuildError::SerializationFailed {
            detail: "oops".into(),
        }
        .into();
        let _: SipError = ValidationError::MissingCallId.into();
        let _: SipError = TransportError::ConnectionClosed { addr: "a".into() }.into();
        let _: SipError = DnsError::NoRecordsFound { domain: "d".into() }.into();
        let _: SipError = TlsError::NoCertificate.into();
        let _: SipError = TransactionError::NotFound { id: "t".into() }.into();
        let _: SipError = DialogError::NotFound { id: "d".into() }.into();
        let _: SipError = RegistrationError::NotFound { id: "r".into() }.into();
        let _: SipError = ConfigError::MissingField { field: "f".into() }.into();
    }
}
