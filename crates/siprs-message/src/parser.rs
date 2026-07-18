//! SIP 消息完整解析器
//!
//! 提供从字节流解析完整 SIP 消息的功能，支持一次性解析和流式解析。
//!
//! # 解析流程
//!
//! 1. 解析 Start Line（请求行或状态行）
//! 2. 解析 Headers（使用已有的 `parse_headers` 函数）
//! 3. 解析 Body（根据 Content-Length 读取指定长度的字节）
//! 4. 消息大小超过 max_message_size 限制时返回 `MessageTooLarge` 错误

use siprs_core::{ParseError, SipVersion, StatusCode};

use crate::headers::{parse_headers, HeaderName, HeaderValue};
use crate::types::{Body, Method, RequestLine, SipMessage, SipRequest, SipResponse, StatusLine};
use crate::uri::SipUri;

/// 默认最大消息大小（64 KB）
const DEFAULT_MAX_MESSAGE_SIZE: usize = 65536;

// ============================================================================
// MessageParser - SIP 消息解析器
// ============================================================================

/// SIP 消息解析器
///
/// 从字节流解析完整的 SIP 消息，支持一次性解析和流式解析。
///
/// # 示例
///
/// ```ignore
/// use siprs_message::parser::MessageParser;
///
/// let parser = MessageParser::new(65536);
/// let data = b"INVITE sip:bob@example.com SIP/2.0\r\n\r\n";
/// let message = parser.parse(data).unwrap();
/// ```
pub struct MessageParser {
    /// 最大允许的消息大小（字节）
    max_message_size: usize,
}

impl MessageParser {
    /// 创建新的消息解析器
    ///
    /// # 参数
    ///
    /// - `max_message_size` - 最大允许的消息大小（字节），超过此大小的消息将返回错误
    pub fn new(max_message_size: usize) -> Self {
        Self { max_message_size }
    }

    /// 使用默认最大消息大小创建解析器
    pub fn default_parser() -> Self {
        Self::new(DEFAULT_MAX_MESSAGE_SIZE)
    }

    /// 从字节流解析完整 SIP 消息
    ///
    /// 解析流程：
    /// 1. 解析 Start Line（请求行或状态行）
    /// 2. 解析 Headers
    /// 3. 解析 Body（根据 Content-Length）
    ///
    /// # 错误
    ///
    /// - `ParseError::MessageTooLarge` - 消息大小超过限制
    /// - `ParseError::InvalidStartLine` - 起始行格式错误
    /// - `ParseError::InvalidVersion` - SIP 版本号不是 "SIP/2.0"
    /// - `ParseError::InvalidStatusCode` - 状态码不在 100-699 范围内
    /// - `ParseError::UnexpectedEof` - 输入数据不完整
    pub fn parse(&self, data: &[u8]) -> Result<SipMessage, ParseError> {
        // 检查消息大小
        if data.len() > self.max_message_size {
            return Err(ParseError::MessageTooLarge {
                size: data.len(),
                max: self.max_message_size,
            });
        }

        let text = std::str::from_utf8(data)?;

        // 1. 解析 Start Line
        let (start_line_str, rest) = split_first_line(text);

        // 2. 分离 Headers 和 Body
        let (headers_str, body_str) = split_headers_body(rest);

        // 3. 解析 Start Line（请求行或状态行）
        let message = if is_status_line(start_line_str) {
            // 状态行
            let status_line = parse_status_line(start_line_str)?;
            let parse_result = parse_headers(headers_str.as_bytes())?;
            let headers = parse_result.headers;
            let body = parse_body(&headers, body_str.as_bytes())?;
            SipMessage::Response(SipResponse {
                status_line,
                headers,
                body,
            })
        } else {
            // 请求行
            let request_line = parse_request_line(start_line_str)?;
            let parse_result = parse_headers(headers_str.as_bytes())?;
            let headers = parse_result.headers;
            let body = parse_body(&headers, body_str.as_bytes())?;
            SipMessage::Request(SipRequest {
                request_line,
                headers,
                body,
            })
        };

        Ok(message)
    }

    /// 流式解析（用于 TCP/TLS 分帧）
    ///
    /// 尝试从字节流中解析一条完整的 SIP 消息。如果数据不完整，返回 `Ok(None)`。
    /// 如果解析成功，返回解析结果和消耗的字节数。
    ///
    /// # 返回值
    ///
    /// - `Ok(Some((message, consumed)))` - 解析成功，返回消息和消耗的字节数
    /// - `Ok(None)` - 数据不完整，需要更多数据
    /// - `Err(error)` - 解析错误
    pub fn parse_streaming(&self, data: &[u8]) -> Result<Option<(SipMessage, usize)>, ParseError> {
        // 检查消息大小
        if data.len() > self.max_message_size {
            return Err(ParseError::MessageTooLarge {
                size: data.len(),
                max: self.max_message_size,
            });
        }

        let text = match std::str::from_utf8(data) {
            Ok(t) => t,
            Err(e) => {
                // UTF-8 解码失败，可能是数据不完整
                return Err(ParseError::Utf8Error(e));
            }
        };

        // 1. 查找 Start Line 结束位置
        let first_crlf = match find_crlf(text) {
            Some(pos) => pos,
            None => return Ok(None), // 数据不完整
        };

        // 2. 查找 Headers 结束位置（空行 CRLFCRLF）
        let header_end = match find_header_end(text) {
            Some(pos) => pos,
            None => return Ok(None), // 数据不完整
        };

        // 3. 解析 Headers 以获取 Content-Length
        let headers_start = first_crlf + 2; // 跳过 Start Line 的 CRLF
        let headers_str = &text[headers_start..header_end];

        let parse_result = parse_headers(headers_str.as_bytes())?;
        let content_length = parse_result
            .headers
            .get(&HeaderName::ContentLength)
            .and_then(|v| v.as_content_length())
            .unwrap_or(0);

        // 4. 检查 Body 是否完整
        let body_start = header_end + 4; // 跳过 CRLFCRLF
        let total_needed = body_start + content_length;

        if text.len() < total_needed {
            return Ok(None); // Body 数据不完整
        }

        // 5. 解析完整消息
        let consumed = total_needed;
        let message_data = &data[..consumed];
        let message = self.parse(message_data)?;

        Ok(Some((message, consumed)))
    }
}

impl Default for MessageParser {
    fn default() -> Self {
        Self::default_parser()
    }
}

// ============================================================================
// 内部解析辅助函数
// ============================================================================

/// 分离第一行（Start Line）和剩余内容
///
/// 返回 (start_line, rest)
fn split_first_line(text: &str) -> (&str, &str) {
    if let Some(pos) = find_crlf(text) {
        (&text[..pos], &text[pos + 2..])
    } else if let Some(pos) = text.find('\n') {
        (&text[..pos], &text[pos + 1..])
    } else {
        (text, "")
    }
}

/// 分离 Headers 和 Body
///
/// Headers 以空行（CRLFCRLF）结束，之后为 Body。
fn split_headers_body(text: &str) -> (String, String) {
    // 查找 CRLFCRLF
    if let Some(pos) = text.find("\r\n\r\n") {
        let headers = text[..pos].to_string();
        let body = text[pos + 4..].to_string();
        (headers, body)
    } else if let Some(pos) = text.find("\n\n") {
        // 容错：处理 LF 分隔
        let headers = text[..pos].to_string();
        let body = text[pos + 2..].to_string();
        (headers, body)
    } else {
        // 没有空行分隔，全部作为 headers
        (text.to_string(), String::new())
    }
}

/// 查找第一个 CRLF 的位置
fn find_crlf(text: &str) -> Option<usize> {
    text.find("\r\n").or_else(|| text.find('\n'))
}

/// 查找 Headers 结束位置（空行 CRLFCRLF 的起始位置）
///
/// 返回空行之前的位置（即最后一个头部行的末尾）。
fn find_header_end(text: &str) -> Option<usize> {
    // 查找 CRLFCRLF
    text.find("\r\n\r\n").or_else(|| text.find("\n\n"))
}

/// 判断起始行是否为状态行
///
/// 状态行以 "SIP/" 开头，请求行以方法名开头。
fn is_status_line(line: &str) -> bool {
    line.trim().starts_with("SIP/")
}

/// 解析请求行
///
/// 格式：`METHOD Request-URI SIP-Version`
fn parse_request_line(line: &str) -> Result<RequestLine, ParseError> {
    let line = line.trim();

    // 格式：METHOD SP Request-URI SP SIP-Version
    let parts: Vec<&str> = line.splitn(3, ' ').collect();
    if parts.len() != 3 {
        return Err(ParseError::InvalidStartLine {
            detail: format!("invalid request line format: {}", line),
        });
    }

    let method: Method = parts[0].parse()?;
    let request_uri: SipUri = SipUri::parse(parts[1])?;

    // 验证 SIP 版本号
    let version_str = parts[2].trim();
    if version_str != SipVersion::VERSION {
        return Err(ParseError::InvalidVersion {
            version: version_str.to_string(),
        });
    }

    Ok(RequestLine {
        method,
        request_uri,
        version: SipVersion,
    })
}

/// 解析状态行
///
/// 格式：`SIP-Version Status-Code Reason-Phrase`
fn parse_status_line(line: &str) -> Result<StatusLine, ParseError> {
    let line = line.trim();

    // 格式：SIP-Version SP Status-Code SP Reason-Phrase
    let parts: Vec<&str> = line.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return Err(ParseError::InvalidStartLine {
            detail: format!("invalid status line format: {}", line),
        });
    }

    // 验证 SIP 版本号
    let version_str = parts[0].trim();
    if version_str != SipVersion::VERSION {
        return Err(ParseError::InvalidVersion {
            version: version_str.to_string(),
        });
    }

    // 解析状态码
    let status_code_val: u16 =
        parts[1]
            .trim()
            .parse()
            .map_err(|_| ParseError::InvalidStatusCode {
                code: 0, // 无法解析的数字
            })?;

    // 验证状态码范围（100-699）
    if !(100..=699).contains(&status_code_val) {
        return Err(ParseError::InvalidStatusCode {
            code: status_code_val,
        });
    }

    let status_code = StatusCode(status_code_val);
    let reason_phrase = if parts.len() > 2 {
        parts[2].trim().to_string()
    } else {
        String::new()
    };

    Ok(StatusLine {
        version: SipVersion,
        status_code,
        reason_phrase,
    })
}

/// 解析消息体
///
/// 根据 Content-Type 和 Content-Length 头部解析消息体。
/// - 如果有 Content-Type 且有消息体内容，创建 Body
/// - 如果无 Content-Type 且无 Content-Length，消息体为空
/// - 如果有 Content-Length，按指定长度读取消息体
fn parse_body(
    headers: &crate::headers::HeaderCollection,
    raw_body: &[u8],
) -> Result<Option<Body>, ParseError> {
    let content_length = headers
        .get(&HeaderName::ContentLength)
        .and_then(|v| v.as_content_length())
        .unwrap_or(0);

    let content_type = headers.get(&HeaderName::ContentType).and_then(|v| {
        if let HeaderValue::ContentType(ct) = v {
            Some(ct.clone())
        } else {
            None
        }
    });

    // 无 Content-Type 且无 Content-Length 时消息体为空
    if content_type.is_none() && content_length == 0 {
        return Ok(None);
    }

    // 有 Content-Length 时，按指定长度读取
    let body_bytes = if content_length > 0 {
        if raw_body.len() < content_length {
            // 数据不完整，Content-Length 声明的长度超过实际可用数据
            return Err(ParseError::UnexpectedEof {
                position: raw_body.len(),
            });
        } else {
            raw_body[..content_length].to_vec()
        }
    } else if content_type.is_some() && !raw_body.is_empty() {
        // 有 Content-Type 但无 Content-Length，使用全部可用数据
        raw_body.to_vec()
    } else {
        return Ok(None);
    };

    if body_bytes.is_empty() && content_type.is_none() {
        return Ok(None);
    }

    let ct = content_type.unwrap_or_else(|| "application/octet-stream".to_string());
    Ok(Some(Body::new(ct, body_bytes)))
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- 请求行解析测试 ----

    #[test]
    fn test_parse_request_line_invite() {
        let rl = parse_request_line("INVITE sip:bob@example.com SIP/2.0").unwrap();
        assert_eq!(rl.method, Method::Invite);
        assert_eq!(rl.request_uri.to_string(), "sip:bob@example.com");
        assert_eq!(rl.version, SipVersion);
    }

    #[test]
    fn test_parse_request_line_register() {
        let rl = parse_request_line("REGISTER sip:registrar.example.com SIP/2.0").unwrap();
        assert_eq!(rl.method, Method::Register);
    }

    #[test]
    fn test_parse_request_line_invalid_version() {
        let result = parse_request_line("INVITE sip:bob@example.com SIP/1.0");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ParseError::InvalidVersion { .. }
        ));
    }

    #[test]
    fn test_parse_request_line_missing_parts() {
        let result = parse_request_line("INVITE sip:bob@example.com");
        assert!(result.is_err());
    }

    // ---- 状态行解析测试 ----

    #[test]
    fn test_parse_status_line_ok() {
        let sl = parse_status_line("SIP/2.0 200 OK").unwrap();
        assert_eq!(sl.version, SipVersion);
        assert_eq!(sl.status_code, StatusCode::OK);
        assert_eq!(sl.reason_phrase, "OK");
    }

    #[test]
    fn test_parse_status_line_trying() {
        let sl = parse_status_line("SIP/2.0 100 Trying").unwrap();
        assert_eq!(sl.status_code, StatusCode(100));
        assert_eq!(sl.reason_phrase, "Trying");
    }

    #[test]
    fn test_parse_status_line_invalid_version() {
        let result = parse_status_line("SIP/1.0 200 OK");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ParseError::InvalidVersion { .. }
        ));
    }

    #[test]
    fn test_parse_status_line_invalid_status_code_low() {
        let result = parse_status_line("SIP/2.0 99 Bad");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ParseError::InvalidStatusCode { code: 99 }
        ));
    }

    #[test]
    fn test_parse_status_line_invalid_status_code_high() {
        let result = parse_status_line("SIP/2.0 800 Bad");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ParseError::InvalidStatusCode { code: 800 }
        ));
    }

    // ---- 完整消息解析测试 ----

    #[test]
    fn test_parse_invite_request() {
        let raw = b"INVITE sip:bob@example.com SIP/2.0\r\n\
                    Via: SIP/2.0/UDP 192.168.1.1:5060;branch=z9hG4bK-abc123\r\n\
                    Call-ID: test@example.com\r\n\
                    CSeq: 1 INVITE\r\n\
                    Content-Length: 0\r\n\
                    \r\n";

        let parser = MessageParser::new(DEFAULT_MAX_MESSAGE_SIZE);
        let message = parser.parse(raw).unwrap();

        assert!(message.is_request());
        if let SipMessage::Request(req) = message {
            assert_eq!(req.request_line.method, Method::Invite);
            assert_eq!(
                req.request_line.request_uri.to_string(),
                "sip:bob@example.com"
            );
            assert_eq!(req.request_line.version, SipVersion);
            assert!(req.body.is_none());
        } else {
            panic!("Expected request message");
        }
    }

    #[test]
    fn test_parse_200_ok_response() {
        let raw = b"SIP/2.0 200 OK\r\n\
                    Via: SIP/2.0/UDP 192.168.1.1:5060;branch=z9hG4bK-abc123\r\n\
                    Call-ID: test@example.com\r\n\
                    CSeq: 1 INVITE\r\n\
                    Content-Length: 0\r\n\
                    \r\n";

        let parser = MessageParser::new(DEFAULT_MAX_MESSAGE_SIZE);
        let message = parser.parse(raw).unwrap();

        assert!(message.is_response());
        if let SipMessage::Response(resp) = message {
            assert_eq!(resp.status_line.version, SipVersion);
            assert_eq!(resp.status_line.status_code, StatusCode::OK);
            assert_eq!(resp.status_line.reason_phrase, "OK");
        } else {
            panic!("Expected response message");
        }
    }

    #[test]
    fn test_parse_message_with_body() {
        let sdp_body = b"v=0\r\no=- 12345 1 IN IP4 192.168.1.1\r\n";
        let raw = format!(
            "INVITE sip:bob@example.com SIP/2.0\r\n\
             Via: SIP/2.0/UDP 192.168.1.1:5060;branch=z9hG4bK-abc123\r\n\
             Call-ID: test@example.com\r\n\
             CSeq: 1 INVITE\r\n\
             Content-Type: application/sdp\r\n\
             Content-Length: {}\r\n\
             \r\n",
            sdp_body.len()
        );

        let mut data = raw.into_bytes();
        data.extend_from_slice(sdp_body);

        let parser = MessageParser::new(DEFAULT_MAX_MESSAGE_SIZE);
        let message = parser.parse(&data).unwrap();

        if let SipMessage::Request(req) = message {
            assert!(req.body.is_some());
            let body = req.body.unwrap();
            assert_eq!(body.content_type, "application/sdp");
            assert_eq!(body.content, sdp_body);
        } else {
            panic!("Expected request message");
        }
    }

    #[test]
    fn test_parse_message_no_content_type_no_content_length() {
        let raw = b"INVITE sip:bob@example.com SIP/2.0\r\n\
                    Via: SIP/2.0/UDP 192.168.1.1:5060;branch=z9hG4bK-abc123\r\n\
                    Call-ID: test@example.com\r\n\
                    CSeq: 1 INVITE\r\n\
                    \r\n";

        let parser = MessageParser::new(DEFAULT_MAX_MESSAGE_SIZE);
        let message = parser.parse(raw).unwrap();

        if let SipMessage::Request(req) = message {
            assert!(req.body.is_none());
        } else {
            panic!("Expected request message");
        }
    }

    #[test]
    fn test_parse_message_too_large() {
        let raw = b"INVITE sip:bob@example.com SIP/2.0\r\n\r\n";
        let parser = MessageParser::new(10); // 非常小的限制
        let result = parser.parse(raw);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ParseError::MessageTooLarge { .. }
        ));
    }

    #[test]
    fn test_parse_invalid_version_request() {
        let raw = b"INVITE sip:bob@example.com SIP/1.0\r\n\r\n";
        let parser = MessageParser::new(DEFAULT_MAX_MESSAGE_SIZE);
        let result = parser.parse(raw);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ParseError::InvalidVersion { .. }
        ));
    }

    #[test]
    fn test_parse_invalid_status_code() {
        let raw = b"SIP/2.0 99 Bad\r\n\r\n";
        let parser = MessageParser::new(DEFAULT_MAX_MESSAGE_SIZE);
        let result = parser.parse(raw);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ParseError::InvalidStatusCode { .. }
        ));
    }

    #[test]
    fn test_parse_invalid_status_code_800() {
        let raw = b"SIP/2.0 800 Bad\r\n\r\n";
        let parser = MessageParser::new(DEFAULT_MAX_MESSAGE_SIZE);
        let result = parser.parse(raw);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ParseError::InvalidStatusCode { .. }
        ));
    }

    #[test]
    fn test_parse_content_length_mismatch_short_body() {
        // Content-Length 声明 100 字节，但实际 body 只有 5 字节
        // parse() 应返回 UnexpectedEof 错误，而非静默截断
        let raw = b"INVITE sip:bob@example.com SIP/2.0\r\n\
                     Content-Type: application/sdp\r\n\
                     Content-Length: 100\r\n\
                     \r\nhello";
        let parser = MessageParser::new(DEFAULT_MAX_MESSAGE_SIZE);
        let result = parser.parse(raw);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), ParseError::UnexpectedEof { .. }),
            "parse() should return UnexpectedEof when Content-Length exceeds actual body"
        );
    }

    #[test]
    fn test_parse_content_length_zero_with_body_data() {
        // Content-Length: 0 但有 Content-Type，body 数据应被忽略
        let raw = b"INVITE sip:bob@example.com SIP/2.0\r\n\
                     Content-Type: application/sdp\r\n\
                     Content-Length: 0\r\n\
                     \r\nextra data";
        let parser = MessageParser::new(DEFAULT_MAX_MESSAGE_SIZE);
        let result = parser.parse(raw);
        // Content-Length: 0 表示没有 body，即使有额外数据也应成功解析
        assert!(result.is_ok());
    }

    // ---- 流式解析测试 ----

    #[test]
    fn test_parse_streaming_complete() {
        let raw = b"INVITE sip:bob@example.com SIP/2.0\r\n\
                    Via: SIP/2.0/UDP 192.168.1.1:5060;branch=z9hG4bK-abc123\r\n\
                    Call-ID: test@example.com\r\n\
                    CSeq: 1 INVITE\r\n\
                    Content-Length: 0\r\n\
                    \r\n";

        let parser = MessageParser::new(DEFAULT_MAX_MESSAGE_SIZE);
        let result = parser.parse_streaming(raw).unwrap();
        assert!(result.is_some());

        let (message, consumed) = result.unwrap();
        assert!(message.is_request());
        assert_eq!(consumed, raw.len());
    }

    #[test]
    fn test_parse_streaming_incomplete() {
        let raw = b"INVITE sip:bob@example.com SIP/2.0\r\n\
                    Via: SIP/2.0/UDP 192.168.1.1:5060;branch=z9hG4bK-abc123\r\n";

        let parser = MessageParser::new(DEFAULT_MAX_MESSAGE_SIZE);
        let result = parser.parse_streaming(raw).unwrap();
        // 数据不完整（没有空行结束 headers），返回 None
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_streaming_with_body() {
        let sdp_body = b"v=0\r\no=- 12345 1 IN IP4 192.168.1.1\r\n";
        let raw = format!(
            "INVITE sip:bob@example.com SIP/2.0\r\n\
             Via: SIP/2.0/UDP 192.168.1.1:5060;branch=z9hG4bK-abc123\r\n\
             Call-ID: test@example.com\r\n\
             CSeq: 1 INVITE\r\n\
             Content-Type: application/sdp\r\n\
             Content-Length: {}\r\n\
             \r\n",
            sdp_body.len()
        );

        let mut data = raw.into_bytes();
        data.extend_from_slice(sdp_body);

        let parser = MessageParser::new(DEFAULT_MAX_MESSAGE_SIZE);
        let result = parser.parse_streaming(&data).unwrap();
        assert!(result.is_some());

        let (message, consumed) = result.unwrap();
        assert!(message.is_request());
        assert_eq!(consumed, data.len());
    }

    #[test]
    fn test_parse_streaming_body_incomplete() {
        let sdp_body = b"v=0\r\no=- 12345 1 IN IP4 192.168.1.1\r\n";
        let raw = format!(
            "INVITE sip:bob@example.com SIP/2.0\r\n\
             Via: SIP/2.0/UDP 192.168.1.1:5060;branch=z9hG4bK-abc123\r\n\
             Call-ID: test@example.com\r\n\
             CSeq: 1 INVITE\r\n\
             Content-Type: application/sdp\r\n\
             Content-Length: {}\r\n\
             \r\n",
            sdp_body.len()
        );

        let mut data = raw.into_bytes();
        // 只添加部分 body
        data.extend_from_slice(&sdp_body[..5]);

        let parser = MessageParser::new(DEFAULT_MAX_MESSAGE_SIZE);
        let result = parser.parse_streaming(&data).unwrap();
        // Body 不完整，返回 None
        assert!(result.is_none());
    }

    // ---- 辅助函数测试 ----

    #[test]
    fn test_is_status_line() {
        assert!(is_status_line("SIP/2.0 200 OK"));
        assert!(is_status_line("  SIP/2.0 200 OK"));
        assert!(!is_status_line("INVITE sip:bob@example.com SIP/2.0"));
        assert!(!is_status_line("REGISTER sip:example.com SIP/2.0"));
    }

    #[test]
    fn test_split_first_line() {
        let (line, rest) = split_first_line("INVITE sip:bob@example.com SIP/2.0\r\nrest");
        assert_eq!(line, "INVITE sip:bob@example.com SIP/2.0");
        assert_eq!(rest, "rest");
    }

    #[test]
    fn test_split_headers_body() {
        let (headers, body) =
            split_headers_body("Header1: value1\r\nHeader2: value2\r\n\r\nbody content");
        assert!(headers.contains("Header1: value1"));
        assert!(headers.contains("Header2: value2"));
        assert_eq!(body, "body content");
    }

    #[test]
    fn test_split_headers_body_no_body() {
        let (headers, body) = split_headers_body("Header1: value1\r\n\r\n");
        assert!(headers.contains("Header1: value1"));
        assert!(body.is_empty());
    }
}
