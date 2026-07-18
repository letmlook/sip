//! SIP TCP 流分帧编解码器
//!
//! 基于 `tokio_util::codec` 实现的 SIP 消息编解码器，用于 TCP/TLS 流式传输。
//!
//! # 分帧策略
//!
//! SIP 消息在 TCP 流中以连续字节流传输，需要正确分帧：
//!
//! 1. 先解析头部，查找 `CRLFCRLF` 分隔符确定头部结束位置
//! 2. 从头部中提取 `Content-Length` 值确定消息体长度
//! 3. 读取指定长度的消息体
//! 4. 处理粘包（多条消息在一次读取中到达）和拆包（一条消息分多次读取）
//! 5. `Content-Length` 缺失时按头部结束位置分帧（无消息体）

use bytes::BytesMut;
use sip_core::TransportError;
use tokio_util::codec::{Decoder, Encoder};

// ============================================================================
// SipCodec - SIP 消息编解码器
// ============================================================================

/// SIP 消息编解码器
///
/// 用于 `Framed<TcpStream, SipCodec>` 将字节流分帧为独立的 SIP 消息。
///
/// # 分帧逻辑
///
/// - 解码：基于 Content-Length 的消息分帧，处理粘包和拆包
/// - 编码：直接将消息字节写入输出缓冲区
pub struct SipCodec {
    /// 最大允许的消息大小（字节）
    max_message_size: usize,
}

impl SipCodec {
    /// 创建新的 SIP 编解码器
    ///
    /// # 参数
    ///
    /// - `max_message_size` - 最大允许的消息大小（字节），超过此大小的消息将返回错误
    pub fn new(max_message_size: usize) -> Self {
        Self { max_message_size }
    }

    /// 使用默认最大消息大小（65535 字节）创建编解码器
    pub fn default_codec() -> Self {
        Self::new(65535)
    }
}

impl Default for SipCodec {
    fn default() -> Self {
        Self::default_codec()
    }
}

impl Decoder for SipCodec {
    type Item = BytesMut;
    type Error = TransportError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<BytesMut>, TransportError> {
        // 检查消息大小限制
        if src.len() > self.max_message_size {
            return Err(TransportError::SendFailed {
                reason: format!(
                    "message too large: {} bytes, max {} bytes",
                    src.len(),
                    self.max_message_size
                ),
            });
        }

        // 如果缓冲区为空，等待更多数据
        if src.is_empty() {
            return Ok(None);
        }

        // 1. 查找头部结束标记（CRLFCRLF 或 LFLF）
        let header_end = find_header_end(src);
        let header_end_pos = match header_end {
            Some(pos) => pos,
            None => return Ok(None), // 头部不完整，等待更多数据
        };

        // 2. 从头部中提取 Content-Length
        let content_length = extract_content_length(&src[..header_end_pos]);

        // 3. 计算完整消息所需的总字节数
        let total_message_len = match content_length {
            Some(cl) => header_end_pos + cl,
            None => header_end_pos, // 无 Content-Length，消息体为空
        };

        // 4. 检查是否有足够的数据
        if src.len() < total_message_len {
            // 数据不完整，等待更多数据
            // 但先确保预留足够的空间
            src.reserve(total_message_len - src.len());
            return Ok(None);
        }

        // 5. 提取完整消息
        let message = src.split_to(total_message_len);

        Ok(Some(message))
    }
}

impl Encoder<BytesMut> for SipCodec {
    type Error = TransportError;

    fn encode(&mut self, item: BytesMut, dst: &mut BytesMut) -> Result<(), TransportError> {
        // 检查消息大小限制
        if item.len() > self.max_message_size {
            return Err(TransportError::SendFailed {
                reason: format!(
                    "message too large: {} bytes, max {} bytes",
                    item.len(),
                    self.max_message_size
                ),
            });
        }

        // 直接将消息字节写入输出缓冲区
        dst.extend_from_slice(&item);
        Ok(())
    }
}

// ============================================================================
// 内部辅助函数
// ============================================================================

/// 查找头部结束位置
///
/// 查找 `CRLFCRLF` 或 `LFLF` 分隔符，返回消息体开始位置（即分隔符之后的偏移量）。
/// 返回值是完整消息中消息体开始的字节偏移量。
///
/// # 返回值
///
/// - `Some(offset)` - 消息体开始的偏移量（包含分隔符本身）
/// - `None` - 未找到头部结束标记
fn find_header_end(src: &[u8]) -> Option<usize> {
    // 查找 CRLFCRLF
    for i in 0..src.len().saturating_sub(3) {
        if src[i] == b'\r' && src[i + 1] == b'\n' && src[i + 2] == b'\r' && src[i + 3] == b'\n' {
            return Some(i + 4); // CRLFCRLF 占 4 字节
        }
    }

    // 容错：查找 LFLF
    for i in 0..src.len().saturating_sub(1) {
        if src[i] == b'\n' && src[i + 1] == b'\n' {
            return Some(i + 2); // LFLF 占 2 字节
        }
    }

    None
}

/// 从原始头部字节中提取 Content-Length 值
///
/// 在头部区域中查找 `Content-Length:` 头部行并解析其值。
/// 支持大小写不敏感匹配，支持简写形式 `l:`。
///
/// # 参数
///
/// - `header_bytes` - 头部区域的原始字节（不包含消息体）
///
/// # 返回值
///
/// - `Some(usize)` - Content-Length 值
/// - `None` - 未找到 Content-Length 头部或解析失败
fn extract_content_length(header_bytes: &[u8]) -> Option<usize> {
    let header_str = std::str::from_utf8(header_bytes).ok()?;

    for line in header_str.lines() {
        let line = line.trim();
        // 查找 Content-Length 头部行（大小写不敏感）
        if let Some(colon_pos) = line.find(':') {
            let name = line[..colon_pos].trim();
            let name_lower = name.to_lowercase();
            if name_lower == "content-length" || name_lower == "l" {
                let value = line[colon_pos + 1..].trim();
                return value.parse::<usize>().ok();
            }
        }
    }

    None
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_header_end_crlf() {
        let data = b"INVITE sip:bob@example.com SIP/2.0\r\n\
                      Content-Length: 0\r\n\
                      \r\n";
        let pos = find_header_end(data);
        assert!(pos.is_some());
        // 消息体应从 CRLFCRLF 之后开始
        let pos = pos.unwrap();
        assert_eq!(data.len(), pos); // Content-Length: 0, 所以消息体为空
    }

    #[test]
    fn test_find_header_end_lf() {
        let data = b"INVITE sip:bob@example.com SIP/2.0\n\
                      Content-Length: 0\n\
                      \n";
        let pos = find_header_end(data);
        assert!(pos.is_some());
    }

    #[test]
    fn test_find_header_end_incomplete() {
        let data = b"INVITE sip:bob@example.com SIP/2.0\r\n\
                      Content-Length: 0";
        let pos = find_header_end(data);
        assert!(pos.is_none());
    }

    #[test]
    fn test_extract_content_length() {
        let header = b"INVITE sip:bob@example.com SIP/2.0\r\n\
                       Via: SIP/2.0/UDP 192.168.1.1:5060\r\n\
                       Content-Length: 150\r\n";
        let cl = extract_content_length(header);
        assert_eq!(cl, Some(150));
    }

    #[test]
    fn test_extract_content_length_case_insensitive() {
        let header = b"INVITE sip:bob@example.com SIP/2.0\r\n\
                       content-length: 42\r\n";
        let cl = extract_content_length(header);
        assert_eq!(cl, Some(42));
    }

    #[test]
    fn test_extract_content_length_short_form() {
        let header = b"INVITE sip:bob@example.com SIP/2.0\r\n\
                       l: 99\r\n";
        let cl = extract_content_length(header);
        assert_eq!(cl, Some(99));
    }

    #[test]
    fn test_extract_content_length_missing() {
        let header = b"INVITE sip:bob@example.com SIP/2.0\r\n\
                       Via: SIP/2.0/UDP 192.168.1.1:5060\r\n";
        let cl = extract_content_length(header);
        assert!(cl.is_none());
    }

    #[test]
    fn test_sip_codec_decode_complete_message() {
        let mut codec = SipCodec::new(65535);
        let mut buf = BytesMut::from(
            b"INVITE sip:bob@example.com SIP/2.0\r\n\
              Content-Length: 0\r\n\
              \r\n"
                .as_slice(),
        );

        let result = codec.decode(&mut buf).unwrap();
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.starts_with(b"INVITE"));
    }

    #[test]
    fn test_sip_codec_decode_incomplete_header() {
        let mut codec = SipCodec::new(65535);
        let mut buf = BytesMut::from(b"INVITE sip:bob@example.com SIP/2.0\r\n".as_slice());

        let result = codec.decode(&mut buf).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_sip_codec_decode_incomplete_body() {
        let mut codec = SipCodec::new(65535);
        let mut buf = BytesMut::from(
            b"INVITE sip:bob@example.com SIP/2.0\r\n\
              Content-Length: 10\r\n\
              \r\n\
              hello"
                .as_slice(),
        );

        // 只有 5 字节消息体，需要 10 字节
        let result = codec.decode(&mut buf).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_sip_codec_decode_multiple_messages() {
        let mut codec = SipCodec::new(65535);
        let mut buf = BytesMut::from(
            b"INVITE sip:bob@example.com SIP/2.0\r\n\
              Content-Length: 0\r\n\
              \r\n\
              SIP/2.0 200 OK\r\n\
              Content-Length: 0\r\n\
              \r\n"
                .as_slice(),
        );

        // 解码第一条消息
        let result1 = codec.decode(&mut buf).unwrap();
        assert!(result1.is_some());
        let msg1 = result1.unwrap();
        assert!(msg1.starts_with(b"INVITE"));

        // 解码第二条消息
        let result2 = codec.decode(&mut buf).unwrap();
        assert!(result2.is_some());
        let msg2 = result2.unwrap();
        assert!(msg2.starts_with(b"SIP/2.0 200"));
    }

    #[test]
    fn test_sip_codec_decode_with_body() {
        let mut codec = SipCodec::new(65535);
        let body = b"v=0\r\no=- 12345 1 IN IP4 192.168.1.1\r\n";
        let header = format!(
            "INVITE sip:bob@example.com SIP/2.0\r\n\
             Content-Length: {}\r\n\
             \r\n",
            body.len()
        );
        let mut buf = BytesMut::from(header.as_bytes());
        buf.extend_from_slice(body);

        let result = codec.decode(&mut buf).unwrap();
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.ends_with(body));
    }

    #[test]
    fn test_sip_codec_encode() {
        let mut codec = SipCodec::new(65535);
        let mut dst = BytesMut::new();
        let item = BytesMut::from(b"INVITE sip:bob@example.com SIP/2.0\r\n\r\n".as_slice());

        codec.encode(item, &mut dst).unwrap();
        assert!(dst.starts_with(b"INVITE"));
    }

    #[test]
    fn test_sip_codec_decode_no_content_length() {
        // 无 Content-Length 时，消息体为空
        let mut codec = SipCodec::new(65535);
        let mut buf = BytesMut::from(
            b"INVITE sip:bob@example.com SIP/2.0\r\n\
              \r\n"
                .as_slice(),
        );

        let result = codec.decode(&mut buf).unwrap();
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.ends_with(b"\r\n\r\n"));
    }
}
