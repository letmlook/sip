//! SIP 消息完整构建器
//!
//! 提供将 SIP 消息序列化为字节流的功能，支持校验模式。
//!
//! # 构建流程
//!
//! 1. 构建 Start Line（请求行或状态行）
//! 2. 构建 Headers
//! 3. 构建 Body
//! 4. 校验模式下验证消息包含必要头部和 Content-Length 一致性

use siprs_core::BuildError;

use crate::headers::{HeaderName, HeaderValue};
use crate::types::{SipMessage, SipRequest, SipResponse};

// ============================================================================
// MessageBuilder - SIP 消息构建器
// ============================================================================

/// SIP 消息构建器
///
/// 将 `SipMessage` 序列化为字节流，支持可选的校验模式。
///
/// # 校验规则
///
/// 在校验模式下（`validate = true`），构建器会验证：
/// - 消息必须包含 Call-ID 头部
/// - 消息必须包含 CSeq 头部
/// - 消息必须包含 Via 头部
/// - Content-Length 与实际消息体长度必须一致
/// - 状态码必须在 100-699 范围内
/// - Via 分支参数必须以 `z9hG4bK` 开头
///
/// # 示例
///
/// ```ignore
/// use siprs_message::builder::MessageBuilder;
/// use siprs_message::types::SipMessage;
///
/// let builder = MessageBuilder::with_validation(true);
/// let bytes = builder.build(&message).unwrap();
/// ```
pub struct MessageBuilder {
    /// 是否启用校验
    validate: bool,
}

impl MessageBuilder {
    /// 创建新的消息构建器（不启用校验）
    pub fn new() -> Self {
        Self { validate: false }
    }

    /// 创建消息构建器，指定是否启用校验
    pub fn with_validation(validate: bool) -> Self {
        Self { validate }
    }

    /// 将消息序列化为字节流
    ///
    /// # 构建流程
    ///
    /// 1. 构建 Start Line
    /// 2. 构建 Headers（自动添加 Content-Length）
    /// 3. 构建 Body
    /// 4. 校验模式下验证消息
    ///
    /// # 错误
    ///
    /// - `BuildError::MissingHeader` - 校验模式下缺少必要头部
    /// - `BuildError::InvalidHeaderValue` - 校验模式下头部值无效
    /// - `BuildError::SerializationFailed` - 序列化失败
    pub fn build(&self, message: &SipMessage) -> Result<Vec<u8>, BuildError> {
        // 校验模式：验证消息
        if self.validate {
            self.validate_message(message)?;
        }

        let mut output = Vec::new();

        // 1. 构建 Start Line
        match message {
            SipMessage::Request(req) => {
                self.build_request_line(req, &mut output)?;
            }
            SipMessage::Response(resp) => {
                self.build_status_line(resp, &mut output)?;
            }
        }

        // 2. 构建 Headers（确保 Content-Length 正确）
        self.build_headers(message, &mut output)?;

        // 3. 空行分隔
        output.extend_from_slice(b"\r\n");

        // 4. 构建 Body
        self.build_body(message, &mut output)?;

        Ok(output)
    }

    /// 构建请求行
    fn build_request_line(&self, req: &SipRequest, output: &mut Vec<u8>) -> Result<(), BuildError> {
        let line = format!(
            "{} {} {}\r\n",
            req.request_line.method, req.request_line.request_uri, req.request_line.version
        );
        output.extend_from_slice(line.as_bytes());
        Ok(())
    }

    /// 构建状态行
    fn build_status_line(
        &self,
        resp: &SipResponse,
        output: &mut Vec<u8>,
    ) -> Result<(), BuildError> {
        let line = format!(
            "{} {} {}\r\n",
            resp.status_line.version, resp.status_line.status_code, resp.status_line.reason_phrase
        );
        output.extend_from_slice(line.as_bytes());
        Ok(())
    }

    /// 构建消息头
    ///
    /// 自动更新 Content-Length 以匹配实际消息体长度。
    fn build_headers(&self, message: &SipMessage, output: &mut Vec<u8>) -> Result<(), BuildError> {
        let headers = message.headers();
        let body_len = message
            .body()
            .as_ref()
            .map(|b| b.content.len())
            .unwrap_or(0);

        // 序列化头部，确保 Content-Length 正确

        let mut has_content_type = false;

        for (name, value) in headers.iter() {
            match name {
                HeaderName::ContentLength => {
                    // 跳过原有的 Content-Length，后面会重新添加

                    continue;
                }
                HeaderName::ContentType => {
                    has_content_type = true;
                }
                _ => {}
            }
            let header_line = format!("{}: {}\r\n", name, value);
            output.extend_from_slice(header_line.as_bytes());
        }

        // 添加 Content-Type（如果有消息体但未设置）
        if !has_content_type {
            if let Some(body) = message.body() {
                let ct_line = format!("Content-Type: {}\r\n", body.content_type);
                output.extend_from_slice(ct_line.as_bytes());
            }
        }

        // 添加 Content-Length（始终添加）
        let cl_line = format!("Content-Length: {}\r\n", body_len);
        output.extend_from_slice(cl_line.as_bytes());

        Ok(())
    }

    /// 构建消息体
    fn build_body(&self, message: &SipMessage, output: &mut Vec<u8>) -> Result<(), BuildError> {
        if let Some(body) = message.body() {
            output.extend_from_slice(&body.content);
        }
        Ok(())
    }

    /// 校验消息
    ///
    /// 校验规则：
    /// - 消息必须包含 Call-ID 头部
    /// - 消息必须包含 CSeq 头部
    /// - 消息必须包含 Via 头部
    /// - Content-Length 与实际消息体长度必须一致
    /// - 状态码必须在 100-699 范围内
    /// - Via 分支参数必须以 `z9hG4bK` 开头
    fn validate_message(&self, message: &SipMessage) -> Result<(), BuildError> {
        let headers = message.headers();

        // 检查 Call-ID
        if !headers.contains(&HeaderName::CallId) {
            return Err(BuildError::MissingHeader {
                header: "Call-ID".to_string(),
            });
        }

        // 检查 CSeq
        if !headers.contains(&HeaderName::CSeq) {
            return Err(BuildError::MissingHeader {
                header: "CSeq".to_string(),
            });
        }

        // 检查 Via
        if !headers.contains(&HeaderName::Via) {
            return Err(BuildError::MissingHeader {
                header: "Via".to_string(),
            });
        }

        // 检查 Content-Length 与实际消息体长度一致性
        let declared_length = headers
            .get(&HeaderName::ContentLength)
            .and_then(|v| v.as_content_length());

        if let Some(declared) = declared_length {
            let actual = message
                .body()
                .as_ref()
                .map(|b| b.content.len())
                .unwrap_or(0);
            if declared != actual {
                return Err(BuildError::InvalidHeaderValue {
                    name: "Content-Length".to_string(),
                    detail: format!(
                        "Content-Length mismatch: declared={}, actual={}",
                        declared, actual
                    ),
                });
            }
        }

        // 检查状态码范围（仅响应消息）
        if let SipMessage::Response(resp) = message {
            let code = resp.status_line.status_code.0;
            if !(100..=699).contains(&code) {
                return Err(BuildError::InvalidHeaderValue {
                    name: "Status-Code".to_string(),
                    detail: format!("status code {} is not in range 100-699", code),
                });
            }
        }

        // 检查 Via 分支参数
        let via_values = headers.get_all(&HeaderName::Via);
        for via_value in via_values {
            if let HeaderValue::Via(via) = via_value {
                if !via.branch.is_valid() {
                    return Err(BuildError::InvalidHeaderValue {
                        name: "Via".to_string(),
                        detail: "Via branch must start with 'z9hG4bK'".to_string(),
                    });
                }
            }
        }

        Ok(())
    }
}

impl Default for MessageBuilder {
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
    use crate::headers::{HeaderCollection, ViaHeader};
    use crate::parser::MessageParser;
    use crate::types::{Body, Method, RequestLine, StatusLine};
    use crate::uri::SipUri;
    use siprs_core::{Host, SipVersion, StatusCode, TransportProtocol};

    /// 创建一个带基本头部的请求消息
    fn create_test_request() -> SipMessage {
        let uri = SipUri::parse("sip:bob@example.com").unwrap();
        let mut headers = HeaderCollection::new();
        headers.insert(
            HeaderName::Via,
            HeaderValue::Via(ViaHeader::new(
                TransportProtocol::Udp,
                Host::Domain("192.168.1.1".to_string()),
                Some(5060),
            )),
        );
        headers.insert(
            HeaderName::CallId,
            HeaderValue::CallId(crate::types::CallId("test@example.com".to_string())),
        );
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(crate::headers::CSeqHeader::new(1, Method::Invite)),
        );

        SipMessage::Request(SipRequest {
            request_line: RequestLine {
                method: Method::Invite,
                request_uri: uri,
                version: SipVersion,
            },
            headers,
            body: None,
        })
    }

    /// 创建一个带基本头部的响应消息
    fn create_test_response() -> SipMessage {
        let mut headers = HeaderCollection::new();
        headers.insert(
            HeaderName::Via,
            HeaderValue::Via(ViaHeader::new(
                TransportProtocol::Udp,
                Host::Domain("192.168.1.1".to_string()),
                Some(5060),
            )),
        );
        headers.insert(
            HeaderName::CallId,
            HeaderValue::CallId(crate::types::CallId("test@example.com".to_string())),
        );
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(crate::headers::CSeqHeader::new(1, Method::Invite)),
        );

        SipMessage::Response(SipResponse {
            status_line: StatusLine {
                version: SipVersion,
                status_code: StatusCode::OK,
                reason_phrase: "OK".to_string(),
            },
            headers,
            body: None,
        })
    }

    // ---- 基本构建测试 ----

    #[test]
    fn test_build_request() {
        let message = create_test_request();
        let builder = MessageBuilder::new();
        let bytes = builder.build(&message).unwrap();

        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.starts_with("INVITE sip:bob@example.com SIP/2.0\r\n"));
        assert!(text.contains("Via:"));
        assert!(text.contains("Call-ID:"));
        assert!(text.contains("CSeq:"));
        assert!(text.contains("Content-Length: 0"));
    }

    #[test]
    fn test_build_response() {
        let message = create_test_response();
        let builder = MessageBuilder::new();
        let bytes = builder.build(&message).unwrap();

        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.starts_with("SIP/2.0 200 OK\r\n"));
        assert!(text.contains("Via:"));
        assert!(text.contains("Call-ID:"));
        assert!(text.contains("CSeq:"));
        assert!(text.contains("Content-Length: 0"));
    }

    #[test]
    fn test_build_request_with_body() {
        let mut message = create_test_request();
        let body_content = b"v=0\r\no=- 12345 1 IN IP4 192.168.1.1\r\n".to_vec();

        if let SipMessage::Request(ref mut req) = message {
            req.body = Some(Body::new("application/sdp", body_content.clone()));
        }

        let builder = MessageBuilder::new();
        let bytes = builder.build(&message).unwrap();

        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains("Content-Type: application/sdp"));
        assert!(text.contains(&format!("Content-Length: {}", body_content.len())));

        // 验证 body 在末尾
        assert!(bytes.ends_with(&body_content));
    }

    // ---- 往返一致性测试 ----

    #[test]
    fn test_roundtrip_request() {
        let message = create_test_request();
        let builder = MessageBuilder::new();
        let bytes = builder.build(&message).unwrap();

        let parser = MessageParser::default_parser();
        let parsed = parser.parse(&bytes).unwrap();

        assert!(parsed.is_request());
        if let SipMessage::Request(req) = parsed {
            assert_eq!(req.request_line.method, Method::Invite);
            assert_eq!(req.request_line.version, SipVersion);
        } else {
            panic!("Expected request");
        }
    }

    #[test]
    fn test_roundtrip_response() {
        let message = create_test_response();
        let builder = MessageBuilder::new();
        let bytes = builder.build(&message).unwrap();

        let parser = MessageParser::default_parser();
        let parsed = parser.parse(&bytes).unwrap();

        assert!(parsed.is_response());
        if let SipMessage::Response(resp) = parsed {
            assert_eq!(resp.status_line.status_code, StatusCode::OK);
            assert_eq!(resp.status_line.reason_phrase, "OK");
        } else {
            panic!("Expected response");
        }
    }

    #[test]
    fn test_roundtrip_with_body() {
        let mut message = create_test_request();
        let body_content = b"v=0\r\no=- 12345 1 IN IP4 192.168.1.1\r\n".to_vec();

        if let SipMessage::Request(ref mut req) = message {
            req.body = Some(Body::new("application/sdp", body_content.clone()));
        }

        let builder = MessageBuilder::new();
        let bytes = builder.build(&message).unwrap();

        let parser = MessageParser::default_parser();
        let parsed = parser.parse(&bytes).unwrap();

        if let SipMessage::Request(req) = parsed {
            assert!(req.body.is_some());
            let body = req.body.unwrap();
            assert_eq!(body.content_type, "application/sdp");
            assert_eq!(body.content, body_content);
        } else {
            panic!("Expected request");
        }
    }

    // ---- 校验模式测试 ----

    #[test]
    fn test_validation_missing_call_id() {
        let uri = SipUri::parse("sip:bob@example.com").unwrap();
        let mut headers = HeaderCollection::new();
        headers.insert(
            HeaderName::Via,
            HeaderValue::Via(ViaHeader::new(
                TransportProtocol::Udp,
                Host::Domain("192.168.1.1".to_string()),
                Some(5060),
            )),
        );
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(crate::headers::CSeqHeader::new(1, Method::Invite)),
        );

        let message = SipMessage::Request(SipRequest {
            request_line: RequestLine {
                method: Method::Invite,
                request_uri: uri,
                version: SipVersion,
            },
            headers,
            body: None,
        });

        let builder = MessageBuilder::with_validation(true);
        let result = builder.build(&message);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, BuildError::MissingHeader { ref header } if header == "Call-ID"));
    }

    #[test]
    fn test_validation_missing_cseq() {
        let uri = SipUri::parse("sip:bob@example.com").unwrap();
        let mut headers = HeaderCollection::new();
        headers.insert(
            HeaderName::Via,
            HeaderValue::Via(ViaHeader::new(
                TransportProtocol::Udp,
                Host::Domain("192.168.1.1".to_string()),
                Some(5060),
            )),
        );
        headers.insert(
            HeaderName::CallId,
            HeaderValue::CallId(crate::types::CallId("test@example.com".to_string())),
        );

        let message = SipMessage::Request(SipRequest {
            request_line: RequestLine {
                method: Method::Invite,
                request_uri: uri,
                version: SipVersion,
            },
            headers,
            body: None,
        });

        let builder = MessageBuilder::with_validation(true);
        let result = builder.build(&message);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, BuildError::MissingHeader { ref header } if header == "CSeq"));
    }

    #[test]
    fn test_validation_missing_via() {
        let uri = SipUri::parse("sip:bob@example.com").unwrap();
        let mut headers = HeaderCollection::new();
        headers.insert(
            HeaderName::CallId,
            HeaderValue::CallId(crate::types::CallId("test@example.com".to_string())),
        );
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(crate::headers::CSeqHeader::new(1, Method::Invite)),
        );

        let message = SipMessage::Request(SipRequest {
            request_line: RequestLine {
                method: Method::Invite,
                request_uri: uri,
                version: SipVersion,
            },
            headers,
            body: None,
        });

        let builder = MessageBuilder::with_validation(true);
        let result = builder.build(&message);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, BuildError::MissingHeader { ref header } if header == "Via"));
    }

    #[test]
    fn test_validation_content_length_mismatch() {
        let uri = SipUri::parse("sip:bob@example.com").unwrap();
        let mut headers = HeaderCollection::new();
        headers.insert(
            HeaderName::Via,
            HeaderValue::Via(ViaHeader::new(
                TransportProtocol::Udp,
                Host::Domain("192.168.1.1".to_string()),
                Some(5060),
            )),
        );
        headers.insert(
            HeaderName::CallId,
            HeaderValue::CallId(crate::types::CallId("test@example.com".to_string())),
        );
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(crate::headers::CSeqHeader::new(1, Method::Invite)),
        );
        // 声明 Content-Length 为 100，但实际 body 为空
        headers.insert(HeaderName::ContentLength, HeaderValue::ContentLength(100));

        let message = SipMessage::Request(SipRequest {
            request_line: RequestLine {
                method: Method::Invite,
                request_uri: uri,
                version: SipVersion,
            },
            headers,
            body: None,
        });

        let builder = MessageBuilder::with_validation(true);
        let result = builder.build(&message);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            BuildError::InvalidHeaderValue { ref name, .. } if name == "Content-Length"
        ));
    }

    #[test]
    fn test_validation_invalid_via_branch() {
        let uri = SipUri::parse("sip:bob@example.com").unwrap();
        let mut headers = HeaderCollection::new();
        let mut invalid_via = ViaHeader::new(
            TransportProtocol::Udp,
            Host::Domain("192.168.1.1".to_string()),
            Some(5060),
        );
        // 设置无效的分支参数
        invalid_via.branch = crate::types::BranchId("invalid-branch".to_string());

        headers.insert(HeaderName::Via, HeaderValue::Via(invalid_via));
        headers.insert(
            HeaderName::CallId,
            HeaderValue::CallId(crate::types::CallId("test@example.com".to_string())),
        );
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(crate::headers::CSeqHeader::new(1, Method::Invite)),
        );

        let message = SipMessage::Request(SipRequest {
            request_line: RequestLine {
                method: Method::Invite,
                request_uri: uri,
                version: SipVersion,
            },
            headers,
            body: None,
        });

        let builder = MessageBuilder::with_validation(true);
        let result = builder.build(&message);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            BuildError::InvalidHeaderValue { ref name, .. } if name == "Via"
        ));
    }

    #[test]
    fn test_validation_valid_message() {
        let message = create_test_request();
        let builder = MessageBuilder::with_validation(true);
        let result = builder.build(&message);
        assert!(result.is_ok());
    }

    #[test]
    fn test_no_validation_missing_headers() {
        let uri = SipUri::parse("sip:bob@example.com").unwrap();
        let headers = HeaderCollection::new();

        let message = SipMessage::Request(SipRequest {
            request_line: RequestLine {
                method: Method::Invite,
                request_uri: uri,
                version: SipVersion,
            },
            headers,
            body: None,
        });

        let builder = MessageBuilder::new(); // 不启用校验
        let result = builder.build(&message);
        assert!(result.is_ok());
    }

    // ---- Content-Length 自动更新测试 ----

    #[test]
    fn test_content_length_auto_update() {
        let mut message = create_test_request();
        let body_content = b"hello world".to_vec();

        if let SipMessage::Request(ref mut req) = message {
            // 添加一个错误的 Content-Length
            req.headers
                .insert(HeaderName::ContentLength, HeaderValue::ContentLength(999));
            req.body = Some(Body::new("text/plain", body_content.clone()));
        }

        let builder = MessageBuilder::new(); // 不启用校验，允许 Content-Length 不一致
        let bytes = builder.build(&message).unwrap();

        let text = std::str::from_utf8(&bytes).unwrap();
        // 构建器应该自动更新 Content-Length 为正确的值
        assert!(text.contains(&format!("Content-Length: {}", body_content.len())));
        assert!(!text.contains("Content-Length: 999"));
    }
}
