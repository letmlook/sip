//! SDP 构建器
//!
//! 提供 Builder 模式构建 SDP 会话描述，
//! 以及 `SessionDescription` 的序列化方法。

use crate::types::*;

/// SDP 构建器
///
/// 使用 Builder 模式逐步构建 `SessionDescription`。
///
/// # 示例
///
/// ```
/// use siprs_sdp::builder::SdpBuilder;
/// use siprs_sdp::types::*;
///
/// let origin = Origin {
///     username: "-".to_string(),
///     session_id: 1234,
///     session_version: 1234,
///     network_type: "IN".to_string(),
///     address_type: "IP4".to_string(),
///     unicast_address: "192.168.1.1".to_string(),
/// };
///
/// let sdp = SdpBuilder::new(origin, "Test Session")
///     .connection(Connection {
///         network_type: "IN".to_string(),
///         address_type: "IP4".to_string(),
///         connection_address: "192.168.1.1".to_string(),
///         ttl: None,
///         number_of_addresses: None,
///     })
///     .time(0, 0)
///     .attribute("recvonly", None)
///     .build();
///
/// let sdp_str = sdp.to_sdp_string();
/// assert!(sdp_str.starts_with("v=0"));
/// ```
pub struct SdpBuilder {
    sdp: SessionDescription,
}

impl SdpBuilder {
    /// 创建新的 SDP 构建器
    ///
    /// # 参数
    ///
    /// - `origin`: Origin 行数据
    /// - `session_name`: 会话名称 (s=)
    pub fn new(origin: Origin, session_name: impl Into<String>) -> Self {
        Self {
            sdp: SessionDescription {
                version: 0,
                origin,
                session_name: session_name.into(),
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
            },
        }
    }

    /// 设置协议版本 (v=)，默认为 0
    pub fn version(mut self, v: u32) -> Self {
        self.sdp.version = v;
        self
    }

    /// 设置会话信息 (i=)
    pub fn session_info(mut self, info: impl Into<String>) -> Self {
        self.sdp.session_info = Some(info.into());
        self
    }

    /// 设置 URI (u=)
    pub fn uri(mut self, uri: impl Into<String>) -> Self {
        self.sdp.uri = Some(uri.into());
        self
    }

    /// 设置邮箱 (e=)
    pub fn email(mut self, email: impl Into<String>) -> Self {
        self.sdp.email = Some(email.into());
        self
    }

    /// 设置电话 (p=)
    pub fn phone(mut self, phone: impl Into<String>) -> Self {
        self.sdp.phone = Some(phone.into());
        self
    }

    /// 设置连接数据 (c=)
    pub fn connection(mut self, c: Connection) -> Self {
        self.sdp.connection = Some(c);
        self
    }

    /// 添加带宽信息 (b=)
    pub fn bandwidth(mut self, bw_type: impl Into<String>, bandwidth: u64) -> Self {
        self.sdp.bandwidth.push(Bandwidth {
            bandwidth_type: bw_type.into(),
            bandwidth,
        });
        self
    }

    /// 添加时间描述 (t=)
    pub fn time(mut self, start: u64, stop: u64) -> Self {
        self.sdp.time_descriptions.push(TimeDescription {
            start_time: start,
            stop_time: stop,
            repeat_times: Vec::new(),
        });
        self
    }

    /// 添加时间描述并带重复时间 (t= + r=)
    pub fn time_with_repeat(
        mut self,
        start: u64,
        stop: u64,
        repeat_interval: u64,
        active_duration: u64,
        offsets: Vec<u64>,
    ) -> Self {
        self.sdp.time_descriptions.push(TimeDescription {
            start_time: start,
            stop_time: stop,
            repeat_times: vec![RepeatTime {
                repeat_interval,
                active_duration,
                offsets,
            }],
        });
        self
    }

    /// 设置时区 (z=)
    pub fn timezone(mut self, tz: impl Into<String>) -> Self {
        self.sdp.timezone = Some(tz.into());
        self
    }

    /// 设置加密密钥 (k=)
    pub fn encryption(mut self, enc: Encryption) -> Self {
        self.sdp.encryption = Some(enc);
        self
    }

    /// 添加会话级属性行 (a=)
    pub fn attribute(mut self, name: impl Into<String>, value: Option<String>) -> Self {
        self.sdp.attributes.push(Attribute {
            name: name.into(),
            value,
        });
        self
    }

    /// 添加媒体描述 (m= + 子行)
    pub fn media(mut self, media: MediaDescription) -> Self {
        self.sdp.media_descriptions.push(media);
        self
    }

    /// 设置 GB28181 国标编码 (y=)
    pub fn ssrc(mut self, ssrc: impl Into<String>) -> Self {
        self.sdp.ssrc = Some(ssrc.into());
        self
    }

    /// 设置 GB28181 媒体格式 (f=)
    pub fn media_format(mut self, format: impl Into<String>) -> Self {
        self.sdp.media_format = Some(format.into());
        self
    }

    /// 构建最终的 SessionDescription
    pub fn build(self) -> SessionDescription {
        self.sdp
    }
}

// ============================================================================
// 序列化
// ============================================================================

impl SessionDescription {
    /// 将 SDP 序列化为文本格式
    ///
    /// 按照 RFC 4566 规定的格式输出，每行以 `\r\n` 结尾。
    ///
    /// # 返回
    ///
    /// 符合 RFC 4566 格式的 SDP 文本字符串
    pub fn to_sdp_string(&self) -> String {
        let mut out = String::with_capacity(512);

        // v= 协议版本（必须第一个）
        out.push_str(&format!("v={}\r\n", self.version));

        // o= 会话源
        out.push_str(&format!("o={}\r\n", self.origin));

        // s= 会话名称
        out.push_str(&format!("s={}\r\n", self.session_name));

        // i= 会话信息（可选）
        if let Some(ref info) = self.session_info {
            out.push_str(&format!("i={}\r\n", info));
        }

        // u= URI（可选）
        if let Some(ref uri) = self.uri {
            out.push_str(&format!("u={}\r\n", uri));
        }

        // e= 邮箱（可选）
        if let Some(ref email) = self.email {
            out.push_str(&format!("e={}\r\n", email));
        }

        // p= 电话（可选）
        if let Some(ref phone) = self.phone {
            out.push_str(&format!("p={}\r\n", phone));
        }

        // c= 连接数据（可选）
        if let Some(ref conn) = self.connection {
            out.push_str(&format!("c={}\r\n", conn));
        }

        // b= 带宽（可多个）
        for bw in &self.bandwidth {
            out.push_str(&format!("b={}\r\n", bw));
        }

        // t= + r= 时间描述
        for td in &self.time_descriptions {
            out.push_str(&format!("t={} {}\r\n", td.start_time, td.stop_time));
            for rt in &td.repeat_times {
                out.push_str(&format!("r={}\r\n", rt));
            }
        }

        // z= 时区（可选）
        if let Some(ref tz) = self.timezone {
            out.push_str(&format!("z={}\r\n", tz));
        }

        // k= 加密密钥（可选）
        if let Some(ref enc) = self.encryption {
            out.push_str(&format!("k={}\r\n", enc));
        }

        // a= 会话级属性
        for attr in &self.attributes {
            out.push_str(&format!("a={}\r\n", attr.to_attribute_value()));
        }

        // 媒体描述
        for media in &self.media_descriptions {
            // m= 行
            out.push_str(&format!("m={}\r\n", media));

            // i= 媒体标题（可选）
            if let Some(ref title) = media.title {
                out.push_str(&format!("i={}\r\n", title));
            }

            // c= 连接数据（可选）
            if let Some(ref conn) = media.connection {
                out.push_str(&format!("c={}\r\n", conn));
            }

            // b= 带宽
            for bw in &media.bandwidth {
                out.push_str(&format!("b={}\r\n", bw));
            }

            // k= 加密密钥（可选）
            if let Some(ref enc) = media.encryption {
                out.push_str(&format!("k={}\r\n", enc));
            }

            // a= 属性
            for attr in &media.attributes {
                out.push_str(&format!("a={}\r\n", attr.to_attribute_value()));
            }
        }

        // GB28181 扩展字段
        // y= 和 f= 行放在媒体描述之外（会话级别）
        // 注意：GB28181 实际上 y= 和 f= 是在媒体描述之后的
        if let Some(ref ssrc) = self.ssrc {
            out.push_str(&format!("y={}\r\n", ssrc));
        }
        if let Some(ref format) = self.media_format {
            out.push_str(&format!("f={}\r\n", format));
        }

        out
    }
}

// ============================================================================
// 辅助构建方法
// ============================================================================

impl Connection {
    /// 创建 IPv4 连接
    pub fn ipv4(address: impl Into<String>) -> Self {
        Connection {
            network_type: "IN".to_string(),
            address_type: "IP4".to_string(),
            connection_address: address.into(),
            ttl: None,
            number_of_addresses: None,
        }
    }

    /// 创建 IPv6 连接
    pub fn ipv6(address: impl Into<String>) -> Self {
        Connection {
            network_type: "IN".to_string(),
            address_type: "IP6".to_string(),
            connection_address: address.into(),
            ttl: None,
            number_of_addresses: None,
        }
    }
}

impl Origin {
    /// 创建 IPv4 Origin
    pub fn ipv4(username: impl Into<String>, address: impl Into<String>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Origin {
            username: username.into(),
            session_id: now,
            session_version: now,
            network_type: "IN".to_string(),
            address_type: "IP4".to_string(),
            unicast_address: address.into(),
        }
    }
}

impl MediaDescription {
    /// 创建新的媒体描述
    pub fn new(media: MediaType, port: u32, proto: impl Into<String>) -> Self {
        MediaDescription {
            media,
            port,
            number_of_ports: None,
            proto: proto.into(),
            formats: Vec::new(),
            title: None,
            connection: None,
            bandwidth: Vec::new(),
            encryption: None,
            attributes: Vec::new(),
        }
    }

    /// 添加格式
    pub fn with_format(mut self, fmt: impl Into<String>) -> Self {
        self.formats.push(fmt.into());
        self
    }

    /// 添加多个格式
    pub fn with_formats(mut self, fmts: Vec<String>) -> Self {
        self.formats.extend(fmts);
        self
    }

    /// 添加属性
    pub fn with_attribute(mut self, name: impl Into<String>, value: Option<String>) -> Self {
        self.attributes.push(Attribute {
            name: name.into(),
            value,
        });
        self
    }

    /// 添加 rtpmap 属性
    pub fn with_rtpmap(mut self, payload: u32, encoding: impl Into<String>) -> Self {
        self.attributes.push(Attribute::new(
            "rtpmap",
            format!("{} {}", payload, encoding.into()),
        ));
        self
    }

    /// 设置连接数据
    pub fn with_connection(mut self, conn: Connection) -> Self {
        self.connection = Some(conn);
        self
    }

    /// 添加带宽
    pub fn with_bandwidth(mut self, bw_type: impl Into<String>, bandwidth: u64) -> Self {
        self.bandwidth.push(Bandwidth {
            bandwidth_type: bw_type.into(),
            bandwidth,
        });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_origin() -> Origin {
        Origin {
            username: "-".to_string(),
            session_id: 1234,
            session_version: 1234,
            network_type: "IN".to_string(),
            address_type: "IP4".to_string(),
            unicast_address: "192.168.1.1".to_string(),
        }
    }

    #[test]
    fn test_builder_basic() {
        let sdp = SdpBuilder::new(test_origin(), "Test Session")
            .connection(Connection::ipv4("192.168.1.1"))
            .time(0, 0)
            .build();

        assert_eq!(sdp.version, 0);
        assert_eq!(sdp.session_name, "Test Session");
        assert!(sdp.connection.is_some());
        assert_eq!(sdp.time_descriptions.len(), 1);
    }

    #[test]
    fn test_builder_with_attributes() {
        let sdp = SdpBuilder::new(test_origin(), "Test")
            .time(0, 0)
            .attribute("recvonly", None)
            .attribute("tool", Some("testapp".to_string()))
            .build();

        assert_eq!(sdp.attributes.len(), 2);
        assert_eq!(sdp.attributes[0].name, "recvonly");
        assert_eq!(sdp.attributes[1].name, "tool");
        assert_eq!(sdp.attributes[1].value.as_deref(), Some("testapp"));
    }

    #[test]
    fn test_builder_with_media() {
        let media = MediaDescription::new(MediaType::Video, 5000, "RTP/AVP")
            .with_format("96")
            .with_rtpmap(96, "PS/90000")
            .with_attribute("recvonly", None);

        let sdp = SdpBuilder::new(test_origin(), "Test")
            .time(0, 0)
            .media(media)
            .build();

        assert_eq!(sdp.media_descriptions.len(), 1);
        assert_eq!(sdp.media_descriptions[0].port, 5000);
        assert_eq!(sdp.media_descriptions[0].formats, vec!["96"]);
    }

    #[test]
    fn test_builder_with_bandwidth() {
        let sdp = SdpBuilder::new(test_origin(), "Test")
            .time(0, 0)
            .bandwidth("AS", 8000)
            .build();

        assert_eq!(sdp.bandwidth.len(), 1);
        assert_eq!(sdp.bandwidth[0].bandwidth_type, "AS");
        assert_eq!(sdp.bandwidth[0].bandwidth, 8000);
    }

    #[test]
    fn test_serialize_basic() {
        let sdp = SdpBuilder::new(test_origin(), "Test Session")
            .connection(Connection::ipv4("192.168.1.1"))
            .time(0, 0)
            .build();

        let sdp_str = sdp.to_sdp_string();
        assert!(sdp_str.contains("v=0\r\n"));
        assert!(sdp_str.contains("o=- 1234 1234 IN IP4 192.168.1.1\r\n"));
        assert!(sdp_str.contains("s=Test Session\r\n"));
        assert!(sdp_str.contains("c=IN IP4 192.168.1.1\r\n"));
        assert!(sdp_str.contains("t=0 0\r\n"));
    }

    #[test]
    fn test_serialize_with_media() {
        let media = MediaDescription::new(MediaType::Video, 5000, "RTP/AVP")
            .with_format("96")
            .with_rtpmap(96, "PS/90000")
            .with_attribute("recvonly", None);

        let sdp = SdpBuilder::new(test_origin(), "Test")
            .time(0, 0)
            .media(media)
            .build();

        let sdp_str = sdp.to_sdp_string();
        assert!(sdp_str.contains("m=video 5000 RTP/AVP 96\r\n"));
        assert!(sdp_str.contains("a=rtpmap:96 PS/90000\r\n"));
        assert!(sdp_str.contains("a=recvonly\r\n"));
    }

    #[test]
    fn test_serialize_with_optional_fields() {
        let sdp = SdpBuilder::new(test_origin(), "Test")
            .session_info("A test session")
            .uri("http://example.com")
            .email("user@example.com")
            .phone("+1234567890")
            .time(0, 0)
            .timezone("2882844526 -1h 2898848070 0")
            .build();

        let sdp_str = sdp.to_sdp_string();
        assert!(sdp_str.contains("i=A test session\r\n"));
        assert!(sdp_str.contains("u=http://example.com\r\n"));
        assert!(sdp_str.contains("e=user@example.com\r\n"));
        assert!(sdp_str.contains("p=+1234567890\r\n"));
        assert!(sdp_str.contains("z=2882844526 -1h 2898848070 0\r\n"));
    }

    #[test]
    fn test_serialize_with_encryption() {
        let sdp = SdpBuilder::new(test_origin(), "Test")
            .time(0, 0)
            .encryption(Encryption {
                method: "base64".to_string(),
                encryption_key: Some("key123".to_string()),
            })
            .build();

        let sdp_str = sdp.to_sdp_string();
        assert!(sdp_str.contains("k=base64:key123\r\n"));
    }

    #[test]
    fn test_serialize_with_repeat_time() {
        let sdp = SdpBuilder::new(test_origin(), "Test")
            .time_with_repeat(3034423619, 3042462419, 604800, 3600, vec![0, 90000])
            .build();

        let sdp_str = sdp.to_sdp_string();
        assert!(sdp_str.contains("t=3034423619 3042462419\r\n"));
        assert!(sdp_str.contains("r=604800 3600 0 90000\r\n"));
    }

    #[test]
    fn test_serialize_gb28181() {
        let sdp = SdpBuilder::new(test_origin(), "Play")
            .connection(Connection::ipv4("192.168.1.1"))
            .time(0, 0)
            .ssrc("01234567890000000001")
            .media_format("v/2/4///a/1/8///")
            .build();

        let sdp_str = sdp.to_sdp_string();
        assert!(sdp_str.contains("y=01234567890000000001\r\n"));
        assert!(sdp_str.contains("f=v/2/4///a/1/8///\r\n"));
    }

    #[test]
    fn test_roundtrip() {
        // SDP 往返一致性测试：解析 -> 序列化 -> 再解析
        let original = "\
v=0\r\n\
o=- 1234 1234 IN IP4 192.168.1.1\r\n\
s=Session\r\n\
c=IN IP4 192.168.1.1\r\n\
t=0 0\r\n\
m=video 5000 RTP/AVP 96\r\n\
a=rtpmap:96 PS/90000\r\n\
a=recvonly\r\n";

        let sdp1 = crate::parser::SdpParser::parse(original).unwrap();
        let serialized = sdp1.to_sdp_string();
        let sdp2 = crate::parser::SdpParser::parse(&serialized).unwrap();

        // 验证关键字段一致
        assert_eq!(sdp1.version, sdp2.version);
        assert_eq!(sdp1.origin.username, sdp2.origin.username);
        assert_eq!(sdp1.origin.session_id, sdp2.origin.session_id);
        assert_eq!(sdp1.session_name, sdp2.session_name);
        assert_eq!(sdp1.connection.is_some(), sdp2.connection.is_some());
        assert_eq!(sdp1.time_descriptions.len(), sdp2.time_descriptions.len());
        assert_eq!(sdp1.media_descriptions.len(), sdp2.media_descriptions.len());

        if let (Some(c1), Some(c2)) = (&sdp1.connection, &sdp2.connection) {
            assert_eq!(c1.connection_address, c2.connection_address);
        }

        assert_eq!(
            sdp1.media_descriptions[0].media,
            sdp2.media_descriptions[0].media
        );
        assert_eq!(
            sdp1.media_descriptions[0].port,
            sdp2.media_descriptions[0].port
        );
        assert_eq!(
            sdp1.media_descriptions[0].attributes.len(),
            sdp2.media_descriptions[0].attributes.len()
        );
    }

    #[test]
    fn test_roundtrip_gb28181() {
        // GB28181 SDP 往返一致性测试
        let original = "\
v=0\r\n\
o=- 1234 1234 IN IP4 192.168.1.1\r\n\
s=Play\r\n\
c=IN IP4 192.168.1.1\r\n\
t=0 0\r\n\
m=video 5000 RTP/AVP 96\r\n\
a=rtpmap:96 PS/90000\r\n\
a=recvonly\r\n\
y=01234567890000000001\r\n\
f=v/2/4///a/1/8///\r\n";

        let sdp1 = crate::parser::SdpParser::parse(original).unwrap();
        let serialized = sdp1.to_sdp_string();
        let sdp2 = crate::parser::SdpParser::parse(&serialized).unwrap();

        assert_eq!(sdp1.ssrc, sdp2.ssrc);
        assert_eq!(sdp1.media_format, sdp2.media_format);
    }

    #[test]
    fn test_connection_ipv4_helper() {
        let conn = Connection::ipv4("192.168.1.1");
        assert_eq!(conn.network_type, "IN");
        assert_eq!(conn.address_type, "IP4");
        assert_eq!(conn.connection_address, "192.168.1.1");
    }

    #[test]
    fn test_connection_ipv6_helper() {
        let conn = Connection::ipv6("::1");
        assert_eq!(conn.network_type, "IN");
        assert_eq!(conn.address_type, "IP6");
        assert_eq!(conn.connection_address, "::1");
    }

    #[test]
    fn test_media_description_builder() {
        let media = MediaDescription::new(MediaType::Audio, 5002, "RTP/AVP")
            .with_formats(vec!["8".to_string(), "0".to_string()])
            .with_rtpmap(8, "PCMA/8000")
            .with_rtpmap(0, "PCMU/8000")
            .with_connection(Connection::ipv4("10.0.0.1"))
            .with_bandwidth("AS", 64);

        assert_eq!(media.media, MediaType::Audio);
        assert_eq!(media.port, 5002);
        assert_eq!(media.formats, vec!["8", "0"]);
        assert_eq!(media.attributes.len(), 2);
        assert!(media.connection.is_some());
        assert_eq!(media.bandwidth.len(), 1);
    }

    #[test]
    fn test_full_sdp_serialization() {
        let media = MediaDescription::new(MediaType::Video, 5000, "RTP/AVP")
            .with_format("96")
            .with_rtpmap(96, "PS/90000")
            .with_attribute("recvonly", None);

        let sdp = SdpBuilder::new(test_origin(), "Play")
            .connection(Connection::ipv4("192.168.1.1"))
            .time(0, 0)
            .attribute("tool", Some("sip-sdp".to_string()))
            .media(media)
            .ssrc("01234567890000000001")
            .media_format("v/2/4///a/1/8///")
            .build();

        let sdp_str = sdp.to_sdp_string();

        // 验证输出包含所有必要行
        assert!(sdp_str.contains("v=0\r\n"));
        assert!(sdp_str.contains("o=- 1234 1234 IN IP4 192.168.1.1\r\n"));
        assert!(sdp_str.contains("s=Play\r\n"));
        assert!(sdp_str.contains("c=IN IP4 192.168.1.1\r\n"));
        assert!(sdp_str.contains("t=0 0\r\n"));
        assert!(sdp_str.contains("a=tool:sip-sdp\r\n"));
        assert!(sdp_str.contains("m=video 5000 RTP/AVP 96\r\n"));
        assert!(sdp_str.contains("a=rtpmap:96 PS/90000\r\n"));
        assert!(sdp_str.contains("a=recvonly\r\n"));
        assert!(sdp_str.contains("y=01234567890000000001\r\n"));
        assert!(sdp_str.contains("f=v/2/4///a/1/8///\r\n"));
    }
}
