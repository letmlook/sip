//! SDP 核心类型定义
//!
//! 基于 RFC 4566 定义的 SDP 会话描述协议数据结构。
//! 包含会话级别和媒体级别的所有标准字段。

use std::fmt;
use std::str::FromStr;

// ============================================================================
// 错误类型
// ============================================================================

/// SDP 解析/构建错误
#[derive(Debug, thiserror::Error)]
pub enum SdpError {
    /// 解析错误：行格式不合法
    #[error("parse error at line {line_no}: {detail}")]
    ParseError { line_no: usize, detail: String },

    /// 解析错误：缺少必需字段
    #[error("missing required field: {field}")]
    MissingField { field: String },

    /// 解析错误：非法值
    #[error("invalid value for field '{field}': {detail}")]
    InvalidValue { field: String, detail: String },

    /// 构建错误
    #[error("build error: {detail}")]
    BuildError { detail: String },
}

/// SDP 操作结果类型
pub type SdpResult<T> = Result<T, SdpError>;

// ============================================================================
// SDP 核心数据结构
// ============================================================================

/// SDP 会话描述
///
/// 对应 RFC 4566 Section 5 定义的完整会话描述，
/// 由会话级别信息和零或多个媒体描述组成。
#[derive(Debug, Clone)]
pub struct SessionDescription {
    /// 协议版本 (v=)，RFC 4566 规定必须为 0
    pub version: u32,
    /// 会话源 (o=)
    pub origin: Origin,
    /// 会话名称 (s=)
    pub session_name: String,
    /// 会话信息 (i=)，可选
    pub session_info: Option<String>,
    /// URI 描述 (u=)，可选
    pub uri: Option<String>,
    /// 邮箱地址 (e=)，可选
    pub email: Option<String>,
    /// 电话号码 (p=)，可选
    pub phone: Option<String>,
    /// 连接数据 (c=)，可选（若每个媒体描述都有则可省略）
    pub connection: Option<Connection>,
    /// 带宽信息 (b=)，可多个
    pub bandwidth: Vec<Bandwidth>,
    /// 时间描述 (t= + r=)，至少一个
    pub time_descriptions: Vec<TimeDescription>,
    /// 时区调整 (z=)，可选
    pub timezone: Option<String>,
    /// 加密密钥 (k=)，可选
    pub encryption: Option<Encryption>,
    /// 会话级属性行 (a=)，可多个
    pub attributes: Vec<Attribute>,
    /// 媒体描述列表 (m= + 子行)
    pub media_descriptions: Vec<MediaDescription>,
    /// GB28181 扩展字段：国标编码 (y=)
    pub ssrc: Option<String>,
    /// GB28181 扩展字段：媒体格式 (f=)
    pub media_format: Option<String>,
}

/// Origin 行 (o=)
///
/// 格式：`o=<username> <sess-id> <sess-version> <nettype> <addrtype> <unicast-address>`
#[derive(Debug, Clone)]
pub struct Origin {
    /// 用户名，`-` 表示无
    pub username: String,
    /// 会话 ID
    pub session_id: u64,
    /// 会话版本
    pub session_version: u64,
    /// 网络类型，通常为 "IN"
    pub network_type: String,
    /// 地址类型，"IP4" 或 "IP6"
    pub address_type: String,
    /// 单播地址
    pub unicast_address: String,
}

/// Connection 数据 (c=)
///
/// 格式：`c=<nettype> <addrtype> <connection-address>[/<ttl>[/<number of addresses>]]`
#[derive(Debug, Clone)]
pub struct Connection {
    /// 网络类型，通常为 "IN"
    pub network_type: String,
    /// 地址类型，"IP4" 或 "IP6"
    pub address_type: String,
    /// 连接地址
    pub connection_address: String,
    /// 生存时间（仅 IP4 组播有效）
    pub ttl: Option<u32>,
    /// 地址数量（仅组播有效）
    pub number_of_addresses: Option<u32>,
}

/// Bandwidth (b=)
///
/// 格式：`b=<bwtype>:<bandwidth>`
#[derive(Debug, Clone)]
pub struct Bandwidth {
    /// 带宽类型：AS（应用特定）、CT（会议总带宽）、RR（接收方带宽）、RS（发送方带宽）
    pub bandwidth_type: String,
    /// 带宽值，单位 kbps
    pub bandwidth: u64,
}

/// Time Description (t= + r=)
///
/// 格式：`t=<start-time> <stop-time>` 后跟零或多个 `r=` 行
#[derive(Debug, Clone)]
pub struct TimeDescription {
    /// 开始时间（NTP 时间戳）
    pub start_time: u64,
    /// 结束时间（NTP 时间戳），0 表示永久会话
    pub stop_time: u64,
    /// 重复时间列表
    pub repeat_times: Vec<RepeatTime>,
}

/// Repeat Time (r=)
///
/// 格式：`r=<repeat-interval> <active-duration> <offsets from start-time>`
#[derive(Debug, Clone)]
pub struct RepeatTime {
    /// 重复间隔
    pub repeat_interval: u64,
    /// 活动持续时间
    pub active_duration: u64,
    /// 相对于开始时间的偏移列表
    pub offsets: Vec<u64>,
}

/// Encryption (k=)
///
/// 格式：`k=<method>` 或 `k=<method>:<encryption-key>`
#[derive(Debug, Clone)]
pub struct Encryption {
    /// 加密方法
    pub method: String,
    /// 加密密钥，可选
    pub encryption_key: Option<String>,
}

/// Attribute (a=)
///
/// 格式：`a=<attribute>` 或 `a=<attribute>:<value>`
#[derive(Debug, Clone)]
pub struct Attribute {
    /// 属性名
    pub name: String,
    /// 属性值，可选（属性标志无值）
    pub value: Option<String>,
}

impl Attribute {
    /// 创建带值的属性
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: Some(value.into()),
        }
    }

    /// 创建标志属性（无值）
    pub fn flag(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: None,
        }
    }

    /// 解析属性行内容
    ///
    /// `rtpmap:96 PS/90000` -> Attribute { name: "rtpmap", value: Some("96 PS/90000") }
    /// `recvonly` -> Attribute { name: "recvonly", value: None }
    pub fn parse(content: &str) -> Self {
        if let Some((name, value)) = content.split_once(':') {
            Attribute {
                name: name.to_string(),
                value: Some(value.to_string()),
            }
        } else {
            Attribute {
                name: content.to_string(),
                value: None,
            }
        }
    }

    /// 序列化为属性行内容
    pub fn to_attribute_value(&self) -> String {
        match &self.value {
            Some(v) => format!("{}:{}", self.name, v),
            None => self.name.clone(),
        }
    }
}

impl fmt::Display for Attribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a={}", self.to_attribute_value())
    }
}

/// Media Description (m= + 子行)
///
/// 格式：`m=<media> <port>[/<number of ports>] <proto> <fmt> ...`
/// 后跟零或多个 i=, c=, b=, k=, a= 行
#[derive(Debug, Clone)]
pub struct MediaDescription {
    /// 媒体类型
    pub media: MediaType,
    /// 端口号
    pub port: u32,
    /// 端口数量（用于组播），可选
    pub number_of_ports: Option<u32>,
    /// 传输协议，如 "RTP/AVP"、"udp"
    pub proto: String,
    /// 格式列表
    pub formats: Vec<String>,
    /// 媒体标题 (i=)，可选
    pub title: Option<String>,
    /// 连接数据 (c=)，可选
    pub connection: Option<Connection>,
    /// 带宽信息 (b=)
    pub bandwidth: Vec<Bandwidth>,
    /// 加密密钥 (k=)，可选
    pub encryption: Option<Encryption>,
    /// 属性行 (a=)
    pub attributes: Vec<Attribute>,
}

/// Media Type
///
/// RFC 4566 定义的五种标准媒体类型，以及扩展类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaType {
    /// 音频
    Audio,
    /// 视频
    Video,
    /// 文本
    Text,
    /// 应用
    Application,
    /// 消息
    Message,
    /// 其他/扩展类型
    Other(String),
}

impl FromStr for MediaType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "audio" => MediaType::Audio,
            "video" => MediaType::Video,
            "text" => MediaType::Text,
            "application" => MediaType::Application,
            "message" => MediaType::Message,
            other => MediaType::Other(other.to_string()),
        })
    }
}

impl MediaType {
    /// 转换为 SDP 文本中的媒体类型字符串
    pub fn as_str(&self) -> &str {
        match self {
            MediaType::Audio => "audio",
            MediaType::Video => "video",
            MediaType::Text => "text",
            MediaType::Application => "application",
            MediaType::Message => "message",
            MediaType::Other(s) => s.as_str(),
        }
    }
}

impl fmt::Display for MediaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {} {} {} {}",
            self.username,
            self.session_id,
            self.session_version,
            self.network_type,
            self.address_type,
            self.unicast_address
        )
    }
}

impl fmt::Display for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {}",
            self.network_type, self.address_type, self.connection_address
        )?;
        if let Some(ttl) = self.ttl {
            write!(f, "/{}", ttl)?;
            if let Some(num) = self.number_of_addresses {
                write!(f, "/{}", num)?;
            }
        }
        Ok(())
    }
}

impl fmt::Display for Bandwidth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.bandwidth_type, self.bandwidth)
    }
}

impl fmt::Display for Encryption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.encryption_key {
            Some(key) => write!(f, "{}:{}", self.method, key),
            None => write!(f, "{}", self.method),
        }
    }
}

impl fmt::Display for RepeatTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.repeat_interval, self.active_duration)?;
        for offset in &self.offsets {
            write!(f, " {}", offset)?;
        }
        Ok(())
    }
}

impl fmt::Display for MediaDescription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 仅输出 m= 行的值部分（不含 "m=" 前缀，由 to_sdp_string 添加）
        write!(f, "{} {}", self.media, self.port)?;
        if let Some(num) = self.number_of_ports {
            write!(f, "/{}", num)?;
        }
        write!(f, " {}", self.proto)?;
        for fmt in &self.formats {
            write!(f, " {}", fmt)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attribute_parse_with_value() {
        let attr = Attribute::parse("rtpmap:96 PS/90000");
        assert_eq!(attr.name, "rtpmap");
        assert_eq!(attr.value.as_deref(), Some("96 PS/90000"));
    }

    #[test]
    fn test_attribute_parse_flag() {
        let attr = Attribute::parse("recvonly");
        assert_eq!(attr.name, "recvonly");
        assert_eq!(attr.value, None);
    }

    #[test]
    fn test_attribute_to_value() {
        let attr = Attribute::new("rtpmap", "96 PS/90000");
        assert_eq!(attr.to_attribute_value(), "rtpmap:96 PS/90000");

        let attr = Attribute::flag("recvonly");
        assert_eq!(attr.to_attribute_value(), "recvonly");
    }

    #[test]
    fn test_media_type_from_str() {
        assert_eq!("audio".parse::<MediaType>().unwrap(), MediaType::Audio);
        assert_eq!("VIDEO".parse::<MediaType>().unwrap(), MediaType::Video);
        assert_eq!("text".parse::<MediaType>().unwrap(), MediaType::Text);
        assert_eq!(
            "application".parse::<MediaType>().unwrap(),
            MediaType::Application
        );
        assert_eq!("message".parse::<MediaType>().unwrap(), MediaType::Message);
        assert!(matches!(
            "other".parse::<MediaType>().unwrap(),
            MediaType::Other(_)
        ));
    }

    #[test]
    fn test_media_type_as_str() {
        assert_eq!(MediaType::Audio.as_str(), "audio");
        assert_eq!(MediaType::Video.as_str(), "video");
        assert_eq!(MediaType::Other("custom".into()).as_str(), "custom");
    }

    #[test]
    fn test_origin_display() {
        let origin = Origin {
            username: "-".to_string(),
            session_id: 1234,
            session_version: 1234,
            network_type: "IN".to_string(),
            address_type: "IP4".to_string(),
            unicast_address: "192.168.1.1".to_string(),
        };
        assert_eq!(format!("{}", origin), "- 1234 1234 IN IP4 192.168.1.1");
    }

    #[test]
    fn test_connection_display() {
        let conn = Connection {
            network_type: "IN".to_string(),
            address_type: "IP4".to_string(),
            connection_address: "224.2.1.1".to_string(),
            ttl: Some(127),
            number_of_addresses: Some(3),
        };
        assert_eq!(format!("{}", conn), "IN IP4 224.2.1.1/127/3");

        let conn_no_ttl = Connection {
            network_type: "IN".to_string(),
            address_type: "IP4".to_string(),
            connection_address: "192.168.1.1".to_string(),
            ttl: None,
            number_of_addresses: None,
        };
        assert_eq!(format!("{}", conn_no_ttl), "IN IP4 192.168.1.1");
    }

    #[test]
    fn test_bandwidth_display() {
        let bw = Bandwidth {
            bandwidth_type: "AS".to_string(),
            bandwidth: 8000,
        };
        assert_eq!(format!("{}", bw), "AS:8000");
    }

    #[test]
    fn test_encryption_display() {
        let enc = Encryption {
            method: "prompt".to_string(),
            encryption_key: None,
        };
        assert_eq!(format!("{}", enc), "prompt");

        let enc_with_key = Encryption {
            method: "base64".to_string(),
            encryption_key: Some("key123".to_string()),
        };
        assert_eq!(format!("{}", enc_with_key), "base64:key123");
    }

    #[test]
    fn test_sdp_error_display() {
        let err = SdpError::ParseError {
            line_no: 5,
            detail: "bad format".to_string(),
        };
        assert_eq!(format!("{}", err), "parse error at line 5: bad format");

        let err = SdpError::MissingField {
            field: "origin".to_string(),
        };
        assert_eq!(format!("{}", err), "missing required field: origin");
    }
}
