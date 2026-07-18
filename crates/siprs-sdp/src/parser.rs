//! SDP 解析器
//!
//! 基于 RFC 4566 实现的 SDP 文本格式解析器。
//! 支持 GB28181 扩展属性行（y=、f=）。

use crate::types::*;

/// SDP 解析器
///
/// 将 SDP 文本字符串解析为 `SessionDescription` 结构体。
///
/// # 解析规则
///
/// - 每行格式为 `type=value`，type 为单字符
/// - `m=` 行开始新的媒体描述，后续的 i=, c=, b=, k=, a= 属于该媒体描述
/// - GB28181 扩展行 `y=` 和 `f=` 也被支持
///
/// # 示例
///
/// ```
/// use siprs_sdp::parser::SdpParser;
///
/// let sdp_text = "v=0\r\no=- 1234 1234 IN IP4 192.168.1.1\r\ns=Session\r\nt=0 0\r\n";
/// let sdp = SdpParser::parse(sdp_text).unwrap();
/// assert_eq!(sdp.version, 0);
/// ```
pub struct SdpParser;

impl SdpParser {
    /// 从字符串解析 SDP
    ///
    /// # 参数
    ///
    /// - `input`: SDP 文本内容，行以 `\r\n` 或 `\n` 分隔
    ///
    /// # 返回
    ///
    /// 解析成功返回 `SessionDescription`，失败返回 `SdpError`
    pub fn parse(input: &str) -> SdpResult<SessionDescription> {
        let lines = Self::split_lines(input);
        Self::parse_lines(&lines)
    }

    /// 将输入文本按行拆分，去除空行
    fn split_lines(input: &str) -> Vec<&str> {
        input
            .lines()
            .map(|l| l.trim_end_matches('\r'))
            .filter(|l| !l.is_empty())
            .collect()
    }

    /// 解析行列表
    fn parse_lines(lines: &[&str]) -> SdpResult<SessionDescription> {
        let mut sdp = SessionDescription {
            version: 0,
            origin: Origin {
                username: String::new(),
                session_id: 0,
                session_version: 0,
                network_type: String::new(),
                address_type: String::new(),
                unicast_address: String::new(),
            },
            session_name: String::new(),
            session_info: None,
            uri: None,
            email: None,
            phone: None,
            connection: None,
            bandwidth: Vec::new(),
            time_descriptions: Vec::new(),
            timezone: None,
            encryption: None,
            attributes: Vec::new(),
            media_descriptions: Vec::new(),
            ssrc: None,
            media_format: None,
        };

        let mut current_media: Option<MediaDescription> = None;
        let mut current_time: Option<TimeDescription> = None;

        for (line_no, line) in lines.iter().enumerate() {
            let line_no_1 = line_no + 1; // 1-based line number for error messages

            // 解析 type=value 格式
            let (line_type, value) = Self::parse_line_type(line, line_no_1)?;

            match line_type {
                'v' => {
                    sdp.version = Self::parse_version(value, line_no_1)?;
                }
                'o' => {
                    sdp.origin = Self::parse_origin(value, line_no_1)?;
                }
                's' => {
                    sdp.session_name = value.to_string();
                }
                'i' => {
                    if let Some(ref mut media) = current_media {
                        media.title = Some(value.to_string());
                    } else {
                        sdp.session_info = Some(value.to_string());
                    }
                }
                'u' => {
                    sdp.uri = Some(value.to_string());
                }
                'e' => {
                    sdp.email = Some(value.to_string());
                }
                'p' => {
                    sdp.phone = Some(value.to_string());
                }
                'c' => {
                    let conn = Self::parse_connection(value, line_no_1)?;
                    if let Some(ref mut media) = current_media {
                        media.connection = Some(conn);
                    } else {
                        sdp.connection = Some(conn);
                    }
                }
                'b' => {
                    let bw = Self::parse_bandwidth(value, line_no_1)?;
                    if let Some(ref mut media) = current_media {
                        media.bandwidth.push(bw);
                    } else {
                        sdp.bandwidth.push(bw);
                    }
                }
                't' => {
                    // 如果有未完成的时间描述，先保存
                    if let Some(td) = current_time.take() {
                        sdp.time_descriptions.push(td);
                    }
                    current_time = Some(Self::parse_time(value, line_no_1)?);
                }
                'r' => {
                    let repeat = Self::parse_repeat(value, line_no_1)?;
                    if let Some(ref mut td) = current_time {
                        td.repeat_times.push(repeat);
                    } else {
                        return Err(SdpError::ParseError {
                            line_no: line_no_1,
                            detail: "r= line without preceding t= line".to_string(),
                        });
                    }
                }
                'z' => {
                    sdp.timezone = Some(value.to_string());
                }
                'k' => {
                    let enc = Self::parse_encryption(value, line_no_1)?;
                    if let Some(ref mut media) = current_media {
                        media.encryption = Some(enc);
                    } else {
                        sdp.encryption = Some(enc);
                    }
                }
                'a' => {
                    let attr = Attribute::parse(value);
                    if let Some(ref mut media) = current_media {
                        media.attributes.push(attr);
                    } else {
                        sdp.attributes.push(attr);
                    }
                }
                'm' => {
                    // 保存当前媒体描述（如果有）
                    if let Some(media) = current_media.take() {
                        sdp.media_descriptions.push(media);
                    }
                    current_media = Some(Self::parse_media(value, line_no_1)?);
                }
                'y' => {
                    // GB28181 扩展：国标编码
                    if let Some(ref mut media) = current_media {
                        // y= 行属于媒体描述级别
                        media.attributes.push(Attribute::new("y", value));
                    }
                    sdp.ssrc = Some(value.to_string());
                }
                'f' => {
                    // GB28181 扩展：媒体格式
                    if let Some(ref mut media) = current_media {
                        media.attributes.push(Attribute::new("f", value));
                    }
                    sdp.media_format = Some(value.to_string());
                }
                _ => {
                    // 忽略未知行类型，保持向前兼容
                }
            }
        }

        // 保存最后一个时间描述
        if let Some(td) = current_time.take() {
            sdp.time_descriptions.push(td);
        }

        // 保存最后一个媒体描述
        if let Some(media) = current_media.take() {
            sdp.media_descriptions.push(media);
        }

        // 验证必需字段
        Self::validate(&sdp)?;

        Ok(sdp)
    }

    /// 解析行类型和值
    ///
    /// 格式：`<type>=<value>`
    fn parse_line_type(line: &str, line_no: usize) -> SdpResult<(char, &str)> {
        if line.len() < 2 {
            return Err(SdpError::ParseError {
                line_no,
                detail: format!("line too short: '{}'", line),
            });
        }

        let line_type = line.chars().next().unwrap();
        let separator = line.chars().nth(1).unwrap();

        if separator != '=' {
            return Err(SdpError::ParseError {
                line_no,
                detail: format!("expected '=' after type character, found '{}'", separator),
            });
        }

        let value = &line[2..];
        Ok((line_type, value))
    }

    /// 解析版本行 (v=)
    fn parse_version(value: &str, line_no: usize) -> SdpResult<u32> {
        value
            .trim()
            .parse::<u32>()
            .map_err(|_| SdpError::ParseError {
                line_no,
                detail: format!("invalid version: '{}'", value),
            })
    }

    /// 解析 Origin 行 (o=)
    ///
    /// 格式：`<username> <sess-id> <sess-version> <nettype> <addrtype> <unicast-address>`
    fn parse_origin(value: &str, line_no: usize) -> SdpResult<Origin> {
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.len() != 6 {
            return Err(SdpError::ParseError {
                line_no,
                detail: format!(
                    "origin line requires 6 fields, found {}: '{}'",
                    parts.len(),
                    value
                ),
            });
        }

        let session_id = parts[1].parse::<u64>().map_err(|_| SdpError::ParseError {
            line_no,
            detail: format!("invalid session-id: '{}'", parts[1]),
        })?;

        let session_version = parts[2].parse::<u64>().map_err(|_| SdpError::ParseError {
            line_no,
            detail: format!("invalid session-version: '{}'", parts[2]),
        })?;

        Ok(Origin {
            username: parts[0].to_string(),
            session_id,
            session_version,
            network_type: parts[3].to_string(),
            address_type: parts[4].to_string(),
            unicast_address: parts[5].to_string(),
        })
    }

    /// 解析 Connection 行 (c=)
    ///
    /// 格式：`<nettype> <addrtype> <connection-address>[/<ttl>[/<number>]]`
    fn parse_connection(value: &str, line_no: usize) -> SdpResult<Connection> {
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.len() < 3 {
            return Err(SdpError::ParseError {
                line_no,
                detail: format!(
                    "connection line requires at least 3 fields, found {}: '{}'",
                    parts.len(),
                    value
                ),
            });
        }

        let (connection_address, ttl, number_of_addresses) =
            Self::parse_connection_address(parts[2], line_no)?;

        Ok(Connection {
            network_type: parts[0].to_string(),
            address_type: parts[1].to_string(),
            connection_address,
            ttl,
            number_of_addresses,
        })
    }

    /// 解析连接地址部分，可能包含 TTL 和地址数量
    fn parse_connection_address(
        addr_spec: &str,
        line_no: usize,
    ) -> SdpResult<(String, Option<u32>, Option<u32>)> {
        let parts: Vec<&str> = addr_spec.split('/').collect();

        match parts.len() {
            1 => Ok((parts[0].to_string(), None, None)),
            2 => {
                let ttl = parts[1].parse::<u32>().map_err(|_| SdpError::ParseError {
                    line_no,
                    detail: format!("invalid TTL: '{}'", parts[1]),
                })?;
                Ok((parts[0].to_string(), Some(ttl), None))
            }
            3 => {
                let ttl = parts[1].parse::<u32>().map_err(|_| SdpError::ParseError {
                    line_no,
                    detail: format!("invalid TTL: '{}'", parts[1]),
                })?;
                let num = parts[2].parse::<u32>().map_err(|_| SdpError::ParseError {
                    line_no,
                    detail: format!("invalid number of addresses: '{}'", parts[2]),
                })?;
                Ok((parts[0].to_string(), Some(ttl), Some(num)))
            }
            _ => Err(SdpError::ParseError {
                line_no,
                detail: format!("invalid connection address: '{}'", addr_spec),
            }),
        }
    }

    /// 解析 Bandwidth 行 (b=)
    ///
    /// 格式：`<bwtype>:<bandwidth>`
    fn parse_bandwidth(value: &str, line_no: usize) -> SdpResult<Bandwidth> {
        let (bw_type, bw_value) = value.split_once(':').ok_or_else(|| SdpError::ParseError {
            line_no,
            detail: format!("bandwidth line missing ':': '{}'", value),
        })?;

        let bandwidth = bw_value
            .trim()
            .parse::<u64>()
            .map_err(|_| SdpError::ParseError {
                line_no,
                detail: format!("invalid bandwidth value: '{}'", bw_value),
            })?;

        Ok(Bandwidth {
            bandwidth_type: bw_type.trim().to_string(),
            bandwidth,
        })
    }

    /// 解析 Time 行 (t=)
    ///
    /// 格式：`<start-time> <stop-time>`
    fn parse_time(value: &str, line_no: usize) -> SdpResult<TimeDescription> {
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.len() != 2 {
            return Err(SdpError::ParseError {
                line_no,
                detail: format!(
                    "time line requires 2 fields, found {}: '{}'",
                    parts.len(),
                    value
                ),
            });
        }

        let start_time = parts[0].parse::<u64>().map_err(|_| SdpError::ParseError {
            line_no,
            detail: format!("invalid start-time: '{}'", parts[0]),
        })?;

        let stop_time = parts[1].parse::<u64>().map_err(|_| SdpError::ParseError {
            line_no,
            detail: format!("invalid stop-time: '{}'", parts[1]),
        })?;

        Ok(TimeDescription {
            start_time,
            stop_time,
            repeat_times: Vec::new(),
        })
    }

    /// 解析 Repeat Time 行 (r=)
    ///
    /// 格式：`<repeat-interval> <active-duration> <offset1> <offset2> ...`
    fn parse_repeat(value: &str, line_no: usize) -> SdpResult<RepeatTime> {
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(SdpError::ParseError {
                line_no,
                detail: format!(
                    "repeat line requires at least 2 fields, found {}: '{}'",
                    parts.len(),
                    value
                ),
            });
        }

        let repeat_interval = parts[0].parse::<u64>().map_err(|_| SdpError::ParseError {
            line_no,
            detail: format!("invalid repeat-interval: '{}'", parts[0]),
        })?;

        let active_duration = parts[1].parse::<u64>().map_err(|_| SdpError::ParseError {
            line_no,
            detail: format!("invalid active-duration: '{}'", parts[1]),
        })?;

        let mut offsets = Vec::new();
        for (i, part) in parts[2..].iter().enumerate() {
            let offset = part.parse::<u64>().map_err(|_| SdpError::ParseError {
                line_no,
                detail: format!("invalid offset at position {}: '{}'", i + 3, part),
            })?;
            offsets.push(offset);
        }

        Ok(RepeatTime {
            repeat_interval,
            active_duration,
            offsets,
        })
    }

    /// 解析 Encryption 行 (k=)
    ///
    /// 格式：`<method>` 或 `<method>:<encryption-key>`
    fn parse_encryption(value: &str, _line_no: usize) -> SdpResult<Encryption> {
        if let Some((method, key)) = value.split_once(':') {
            Ok(Encryption {
                method: method.trim().to_string(),
                encryption_key: Some(key.trim().to_string()),
            })
        } else {
            Ok(Encryption {
                method: value.trim().to_string(),
                encryption_key: None,
            })
        }
    }

    /// 解析 Media 行 (m=)
    ///
    /// 格式：`<media> <port>[/<number of ports>] <proto> <fmt> ...`
    fn parse_media(value: &str, line_no: usize) -> SdpResult<MediaDescription> {
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.len() < 4 {
            return Err(SdpError::ParseError {
                line_no,
                detail: format!(
                    "media line requires at least 4 fields, found {}: '{}'",
                    parts.len(),
                    value
                ),
            });
        }

        let media = parts[0].parse::<MediaType>().unwrap();

        // 解析端口，可能包含 /<number of ports>
        let (port, number_of_ports) = Self::parse_port(parts[1], line_no)?;

        let proto = parts[2].to_string();

        let formats: Vec<String> = parts[3..].iter().map(|s| s.to_string()).collect();

        Ok(MediaDescription {
            media,
            port,
            number_of_ports,
            proto,
            formats,
            title: None,
            connection: None,
            bandwidth: Vec::new(),
            encryption: None,
            attributes: Vec::new(),
        })
    }

    /// 解析端口规格
    ///
    /// 格式：`<port>` 或 `<port>/<number of ports>`
    fn parse_port(port_spec: &str, line_no: usize) -> SdpResult<(u32, Option<u32>)> {
        if let Some((port_str, num_str)) = port_spec.split_once('/') {
            let port = port_str.parse::<u32>().map_err(|_| SdpError::ParseError {
                line_no,
                detail: format!("invalid port: '{}'", port_str),
            })?;
            let num = num_str.parse::<u32>().map_err(|_| SdpError::ParseError {
                line_no,
                detail: format!("invalid number of ports: '{}'", num_str),
            })?;
            Ok((port, Some(num)))
        } else {
            let port = port_spec.parse::<u32>().map_err(|_| SdpError::ParseError {
                line_no,
                detail: format!("invalid port: '{}'", port_spec),
            })?;
            Ok((port, None))
        }
    }

    /// 验证 SDP 必需字段
    fn validate(sdp: &SessionDescription) -> SdpResult<()> {
        if sdp.session_name.is_empty() {
            return Err(SdpError::MissingField {
                field: "session_name (s=)".to_string(),
            });
        }

        if sdp.time_descriptions.is_empty() {
            return Err(SdpError::MissingField {
                field: "time_description (t=)".to_string(),
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_sdp() {
        let sdp_text = "v=0\r\no=- 1234 1234 IN IP4 192.168.1.1\r\ns=Session\r\nc=IN IP4 192.168.1.1\r\nt=0 0\r\nm=video 5000 RTP/AVP 96\r\na=rtpmap:96 PS/90000\r\na=recvonly\r\n";
        let sdp = SdpParser::parse(sdp_text).unwrap();

        assert_eq!(sdp.version, 0);
        assert_eq!(sdp.origin.username, "-");
        assert_eq!(sdp.origin.session_id, 1234);
        assert_eq!(sdp.origin.session_version, 1234);
        assert_eq!(sdp.origin.network_type, "IN");
        assert_eq!(sdp.origin.address_type, "IP4");
        assert_eq!(sdp.origin.unicast_address, "192.168.1.1");
        assert_eq!(sdp.session_name, "Session");
        assert!(sdp.connection.is_some());
        assert_eq!(sdp.time_descriptions.len(), 1);
        assert_eq!(sdp.time_descriptions[0].start_time, 0);
        assert_eq!(sdp.time_descriptions[0].stop_time, 0);
        assert_eq!(sdp.media_descriptions.len(), 1);

        let media = &sdp.media_descriptions[0];
        assert_eq!(media.media, MediaType::Video);
        assert_eq!(media.port, 5000);
        assert_eq!(media.proto, "RTP/AVP");
        assert_eq!(media.formats, vec!["96"]);
        assert_eq!(media.attributes.len(), 2);
        assert_eq!(media.attributes[0].name, "rtpmap");
        assert_eq!(media.attributes[0].value.as_deref(), Some("96 PS/90000"));
        assert_eq!(media.attributes[1].name, "recvonly");
        assert_eq!(media.attributes[1].value, None);
    }

    #[test]
    fn test_parse_sdp_with_multiple_media() {
        let sdp_text = "\
v=0\r\n\
o=- 1234 1234 IN IP4 192.168.1.1\r\n\
s=Session\r\n\
t=0 0\r\n\
m=audio 5002 RTP/AVP 8 0\r\n\
a=rtpmap:8 PCMA/8000\r\n\
m=video 5000 RTP/AVP 96\r\n\
a=rtpmap:96 PS/90000\r\n";

        let sdp = SdpParser::parse(sdp_text).unwrap();
        assert_eq!(sdp.media_descriptions.len(), 2);

        // 音频媒体
        let audio = &sdp.media_descriptions[0];
        assert_eq!(audio.media, MediaType::Audio);
        assert_eq!(audio.port, 5002);
        assert_eq!(audio.formats, vec!["8", "0"]);

        // 视频媒体
        let video = &sdp.media_descriptions[1];
        assert_eq!(video.media, MediaType::Video);
        assert_eq!(video.port, 5000);
    }

    #[test]
    fn test_parse_sdp_with_bandwidth() {
        let sdp_text = "\
v=0\r\n\
o=- 1234 1234 IN IP4 192.168.1.1\r\n\
s=Session\r\n\
b=AS:8000\r\n\
t=0 0\r\n";

        let sdp = SdpParser::parse(sdp_text).unwrap();
        assert_eq!(sdp.bandwidth.len(), 1);
        assert_eq!(sdp.bandwidth[0].bandwidth_type, "AS");
        assert_eq!(sdp.bandwidth[0].bandwidth, 8000);
    }

    #[test]
    fn test_parse_sdp_with_connection_multicast() {
        let sdp_text = "\
v=0\r\n\
o=- 1234 1234 IN IP4 192.168.1.1\r\n\
s=Session\r\n\
c=IN IP4 224.2.1.1/127/3\r\n\
t=0 0\r\n";

        let sdp = SdpParser::parse(sdp_text).unwrap();
        let conn = sdp.connection.unwrap();
        assert_eq!(conn.connection_address, "224.2.1.1");
        assert_eq!(conn.ttl, Some(127));
        assert_eq!(conn.number_of_addresses, Some(3));
    }

    #[test]
    fn test_parse_sdp_with_repeat_time() {
        let sdp_text = "\
v=0\r\n\
o=- 1234 1234 IN IP4 192.168.1.1\r\n\
s=Session\r\n\
t=3034423619 3042462419\r\n\
r=604800 3600 0 90000\r\n";

        let sdp = SdpParser::parse(sdp_text).unwrap();
        assert_eq!(sdp.time_descriptions.len(), 1);
        let td = &sdp.time_descriptions[0];
        assert_eq!(td.start_time, 3034423619);
        assert_eq!(td.stop_time, 3042462419);
        assert_eq!(td.repeat_times.len(), 1);
        assert_eq!(td.repeat_times[0].repeat_interval, 604800);
        assert_eq!(td.repeat_times[0].active_duration, 3600);
        assert_eq!(td.repeat_times[0].offsets, vec![0, 90000]);
    }

    #[test]
    fn test_parse_sdp_with_encryption() {
        let sdp_text = "\
v=0\r\n\
o=- 1234 1234 IN IP4 192.168.1.1\r\n\
s=Session\r\n\
k=base64:key123\r\n\
t=0 0\r\n";

        let sdp = SdpParser::parse(sdp_text).unwrap();
        let enc = sdp.encryption.unwrap();
        assert_eq!(enc.method, "base64");
        assert_eq!(enc.encryption_key.as_deref(), Some("key123"));
    }

    #[test]
    fn test_parse_sdp_with_media_encryption() {
        let sdp_text = "\
v=0\r\n\
o=- 1234 1234 IN IP4 192.168.1.1\r\n\
s=Session\r\n\
t=0 0\r\n\
m=video 5000 RTP/AVP 96\r\n\
k=prompt\r\n";

        let sdp = SdpParser::parse(sdp_text).unwrap();
        let enc = sdp.media_descriptions[0].encryption.as_ref().unwrap();
        assert_eq!(enc.method, "prompt");
        assert_eq!(enc.encryption_key, None);
    }

    #[test]
    fn test_parse_sdp_with_media_title() {
        let sdp_text = "\
v=0\r\n\
o=- 1234 1234 IN IP4 192.168.1.1\r\n\
s=Session\r\n\
t=0 0\r\n\
m=video 5000 RTP/AVP 96\r\n\
i=Video Channel\r\n";

        let sdp = SdpParser::parse(sdp_text).unwrap();
        assert_eq!(
            sdp.media_descriptions[0].title.as_deref(),
            Some("Video Channel")
        );
    }

    #[test]
    fn test_parse_sdp_with_session_info() {
        let sdp_text = "\
v=0\r\n\
o=- 1234 1234 IN IP4 192.168.1.1\r\n\
s=Session\r\n\
i=A test session\r\n\
t=0 0\r\n";

        let sdp = SdpParser::parse(sdp_text).unwrap();
        assert_eq!(sdp.session_info.as_deref(), Some("A test session"));
    }

    #[test]
    fn test_parse_sdp_with_uri_email_phone() {
        let sdp_text = "\
v=0\r\n\
o=- 1234 1234 IN IP4 192.168.1.1\r\n\
s=Session\r\n\
u=http://example.com\r\n\
e=user@example.com\r\n\
p=+1234567890\r\n\
t=0 0\r\n";

        let sdp = SdpParser::parse(sdp_text).unwrap();
        assert_eq!(sdp.uri.as_deref(), Some("http://example.com"));
        assert_eq!(sdp.email.as_deref(), Some("user@example.com"));
        assert_eq!(sdp.phone.as_deref(), Some("+1234567890"));
    }

    #[test]
    fn test_parse_sdp_with_timezone() {
        let sdp_text = "\
v=0\r\n\
o=- 1234 1234 IN IP4 192.168.1.1\r\n\
s=Session\r\n\
t=0 0\r\n\
z=2882844526 -1h 2898848070 0\r\n";

        let sdp = SdpParser::parse(sdp_text).unwrap();
        assert_eq!(sdp.timezone.as_deref(), Some("2882844526 -1h 2898848070 0"));
    }

    #[test]
    fn test_parse_gb28181_sdp() {
        let sdp_text = "\
v=0\r\n\
o=- 1234 1234 IN IP4 192.168.1.1\r\n\
s=Play\r\n\
c=IN IP4 192.168.1.1\r\n\
t=0 0\r\n\
m=video 5000 RTP/AVP 96 97 98\r\n\
a=rtpmap:96 PS/90000\r\n\
a=rtpmap:97 H264/90000\r\n\
a=rtpmap:98 H265/90000\r\n\
a=recvonly\r\n\
y=01234567890000000001\r\n\
f=v/2/4///a/1/8///\r\n";

        let sdp = SdpParser::parse(sdp_text).unwrap();
        assert_eq!(sdp.ssrc.as_deref(), Some("01234567890000000001"));
        assert_eq!(sdp.media_format.as_deref(), Some("v/2/4///a/1/8///"));

        // y= 和 f= 也应该出现在媒体描述的属性中
        let media = &sdp.media_descriptions[0];
        let y_attr = media.attributes.iter().find(|a| a.name == "y");
        assert!(y_attr.is_some());
        assert_eq!(
            y_attr.unwrap().value.as_deref(),
            Some("01234567890000000001")
        );

        let f_attr = media.attributes.iter().find(|a| a.name == "f");
        assert!(f_attr.is_some());
        assert_eq!(f_attr.unwrap().value.as_deref(), Some("v/2/4///a/1/8///"));
    }

    #[test]
    fn test_parse_sdp_with_media_connection() {
        let sdp_text = "\
v=0\r\n\
o=- 1234 1234 IN IP4 192.168.1.1\r\n\
s=Session\r\n\
t=0 0\r\n\
m=video 5000 RTP/AVP 96\r\n\
c=IN IP4 10.0.0.1\r\n\
a=rtpmap:96 PS/90000\r\n";

        let sdp = SdpParser::parse(sdp_text).unwrap();
        let conn = sdp.media_descriptions[0].connection.as_ref().unwrap();
        assert_eq!(conn.connection_address, "10.0.0.1");
    }

    #[test]
    fn test_parse_sdp_with_media_bandwidth() {
        let sdp_text = "\
v=0\r\n\
o=- 1234 1234 IN IP4 192.168.1.1\r\n\
s=Session\r\n\
t=0 0\r\n\
m=video 5000 RTP/AVP 96\r\n\
b=AS:4000\r\n\
a=rtpmap:96 PS/90000\r\n";

        let sdp = SdpParser::parse(sdp_text).unwrap();
        assert_eq!(sdp.media_descriptions[0].bandwidth.len(), 1);
        assert_eq!(sdp.media_descriptions[0].bandwidth[0].bandwidth, 4000);
    }

    #[test]
    fn test_parse_sdp_with_port_count() {
        let sdp_text = "\
v=0\r\n\
o=- 1234 1234 IN IP4 192.168.1.1\r\n\
s=Session\r\n\
t=0 0\r\n\
m=video 5000/2 RTP/AVP 96\r\n";

        let sdp = SdpParser::parse(sdp_text).unwrap();
        assert_eq!(sdp.media_descriptions[0].port, 5000);
        assert_eq!(sdp.media_descriptions[0].number_of_ports, Some(2));
    }

    #[test]
    fn test_parse_error_missing_equals() {
        let sdp_text = "v0\r\no=- 1234 1234 IN IP4 192.168.1.1\r\ns=Session\r\nt=0 0\r\n";
        let result = SdpParser::parse(sdp_text);
        assert!(result.is_err());
        if let Err(SdpError::ParseError { detail, .. }) = result {
            assert!(detail.contains("expected '='"));
        } else {
            panic!("Expected ParseError");
        }
    }

    #[test]
    fn test_parse_error_invalid_origin() {
        let sdp_text = "v=0\r\no=bad\r\ns=Session\r\nt=0 0\r\n";
        let result = SdpParser::parse(sdp_text);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_error_missing_time() {
        let sdp_text = "v=0\r\no=- 1234 1234 IN IP4 192.168.1.1\r\ns=Session\r\n";
        let result = SdpParser::parse(sdp_text);
        assert!(result.is_err());
        if let Err(SdpError::MissingField { field }) = result {
            assert!(field.contains("time_description"));
        } else {
            panic!("Expected MissingField error");
        }
    }

    #[test]
    fn test_parse_error_repeat_without_time() {
        let sdp_text =
            "v=0\r\no=- 1234 1234 IN IP4 192.168.1.1\r\ns=Session\r\nr=604800 3600 0\r\n";
        let result = SdpParser::parse(sdp_text);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_sdp_lf_only() {
        // 支持 \n 换行（非标准但常见）
        let sdp_text = "v=0\no=- 1234 1234 IN IP4 192.168.1.1\ns=Session\nt=0 0\n";
        let sdp = SdpParser::parse(sdp_text).unwrap();
        assert_eq!(sdp.version, 0);
        assert_eq!(sdp.session_name, "Session");
    }

    #[test]
    fn test_parse_sdp_session_level_attributes() {
        let sdp_text = "\
v=0\r\n\
o=- 1234 1234 IN IP4 192.168.1.1\r\n\
s=Session\r\n\
a=tool:testapp\r\n\
a=recvonly\r\n\
t=0 0\r\n";

        let sdp = SdpParser::parse(sdp_text).unwrap();
        assert_eq!(sdp.attributes.len(), 2);
        assert_eq!(sdp.attributes[0].name, "tool");
        assert_eq!(sdp.attributes[0].value.as_deref(), Some("testapp"));
        assert_eq!(sdp.attributes[1].name, "recvonly");
    }

    #[test]
    fn test_parse_sdp_multiple_time_descriptions() {
        let sdp_text = "\
v=0\r\n\
o=- 1234 1234 IN IP4 192.168.1.1\r\n\
s=Session\r\n\
t=3034423619 3042462419\r\n\
r=604800 3600 0 90000\r\n\
t=3042462420 3043462420\r\n";

        let sdp = SdpParser::parse(sdp_text).unwrap();
        assert_eq!(sdp.time_descriptions.len(), 2);
        assert_eq!(sdp.time_descriptions[0].start_time, 3034423619);
        assert_eq!(sdp.time_descriptions[0].repeat_times.len(), 1);
        assert_eq!(sdp.time_descriptions[1].start_time, 3042462420);
        assert_eq!(sdp.time_descriptions[1].repeat_times.len(), 0);
    }
}
