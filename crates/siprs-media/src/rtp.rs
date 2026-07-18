//! RTP 包解析与构建
//!
//! 基于 RFC 3550 实现的 RTP (Real-time Transport Protocol) 包处理模块。
//! 支持 RFC 5285 RTP 头部扩展。
//!
//! # RTP 包格式 (RFC 3550)
//!
//! ```text
//!  0                   1                   2                   3
//!  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |V=2|P|X|  CC   |M|     PT      |       sequence number         |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                           timestamp                           |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |           synchronization source (SSRC) identifier            |
//! +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
//! |            contributing source (CSRC) identifiers             |
//! |                             ....                              |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |      RTP header extension (if X=1, RFC 5285)                 |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                         payload                               |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! ```

use crate::MediaError;

// ============================================================================
// 常量
// ============================================================================

/// RTP 固定头部大小（字节）
pub const RTP_HEADER_MIN_SIZE: usize = 12;

/// RTP 版本号（RFC 3550 规定为 2）
pub const RTP_VERSION: u8 = 2;

/// RTP 头部扩展 Profile (RFC 5285 one-byte header)
pub const RTP_EXTENSION_PROFILE_ONE_BYTE: u16 = 0xBEDE;

/// RTP 头部扩展 Profile (RFC 5285 two-byte header)
pub const RTP_EXTENSION_PROFILE_TWO_BYTE: u16 = 0x1000;

// ============================================================================
// RTP 头部扩展
// ============================================================================

/// RTP 头部扩展 (RFC 5285)
///
/// 支持一字节头部和两字节头部两种格式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtpHeaderExtension {
    /// 扩展 Profile 标识
    /// - `0xBEDE`: 一字节头部扩展 (RFC 5285)
    /// - `0x1000`: 两字节头部扩展 (RFC 5285)
    pub profile: u16,
    /// 扩展数据（不含 profile 和 length 字段）
    pub data: Vec<u8>,
}

impl RtpHeaderExtension {
    /// 创建一字节头部扩展 (RFC 5285)
    ///
    /// # 参数
    ///
    /// - `id`: 扩展 ID (0-15)
    /// - `data`: 扩展数据 (1-16 字节)
    pub fn one_byte(id: u8, data: Vec<u8>) -> Self {
        let mut ext_data = Vec::with_capacity(data.len() + 1);
        // 一字节头部: ID(4 bits) | length-1(4 bits)
        let len_minus_1 = (data.len() as u8).saturating_sub(1);
        ext_data.push((id << 4) | (len_minus_1 & 0x0F));
        ext_data.extend_from_slice(&data);
        // 填充到 32 位对齐
        while ext_data.len() % 4 != 0 {
            ext_data.push(0);
        }
        Self {
            profile: RTP_EXTENSION_PROFILE_ONE_BYTE,
            data: ext_data,
        }
    }

    /// 创建两字节头部扩展 (RFC 5285)
    ///
    /// # 参数
    ///
    /// - `id`: 扩展 ID (0-255)
    /// - `data`: 扩展数据 (0-255 字节)
    pub fn two_byte(id: u8, data: Vec<u8>) -> Self {
        let mut ext_data = Vec::with_capacity(data.len() + 2);
        // 两字节头部: ID(8 bits) | length(8 bits)
        ext_data.push(id);
        ext_data.push(data.len() as u8);
        ext_data.extend_from_slice(&data);
        // 填充到 32 位对齐
        while ext_data.len() % 4 != 0 {
            ext_data.push(0);
        }
        Self {
            profile: RTP_EXTENSION_PROFILE_TWO_BYTE,
            data: ext_data,
        }
    }

    /// 获取扩展数据占用的 32 位字数
    fn word_count(&self) -> u16 {
        (self.data.len() as u16).div_ceil(4)
    }
}

// ============================================================================
// RTP 头部
// ============================================================================

/// RTP 包头部
///
/// 对应 RFC 3550 Section 5.1 定义的 RTP 固定头部字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtpHeader {
    /// 版本号 (V)，RFC 3550 规定为 2
    pub version: u8,
    /// 填充标志 (P)，若为 1 则包尾有填充字节
    pub padding: bool,
    /// 扩展标志 (X)，若为 1 则头部后跟随一个头部扩展
    pub extension: bool,
    /// 标记位 (M)，由 profile 定义其含义
    pub marker: bool,
    /// 载荷类型 (PT)，标识 RTP 载荷的格式
    pub payload_type: u8,
    /// 序列号
    pub sequence_number: u16,
    /// 时间戳
    pub timestamp: u32,
    /// 同步源标识符 (SSRC)
    pub ssrc: u32,
    /// 贡献源标识符列表 (CSRC)
    pub csrc_list: Vec<u32>,
    /// 头部扩展（可选）
    pub extension_data: Option<RtpHeaderExtension>,
}

impl RtpHeader {
    /// 创建新的 RTP 头部
    pub fn new(payload_type: u8, ssrc: u32) -> Self {
        Self {
            version: RTP_VERSION,
            padding: false,
            extension: false,
            marker: false,
            payload_type,
            sequence_number: 0,
            timestamp: 0,
            ssrc,
            csrc_list: Vec::new(),
            extension_data: None,
        }
    }

    /// 获取 CSRC 计数（从 csrc_list 长度派生）
    pub fn csrc_count(&self) -> u8 {
        self.csrc_list.len().min(15) as u8
    }

    /// 计算头部占用的字节长度
    pub fn size_in_bytes(&self) -> usize {
        let mut size = RTP_HEADER_MIN_SIZE;
        size += self.csrc_list.len() * 4;
        if let Some(ref ext) = self.extension_data {
            // 扩展头部: profile(2) + length(2) + data
            size += 4 + ext.data.len();
        }
        size
    }

    /// 序列化头部为字节
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.size_in_bytes());

        // 第 1 字节: V(2) | P(1) | X(1) | CC(4)
        let byte0 = (self.version << 6)
            | ((self.padding as u8) << 5)
            | ((self.extension as u8) << 4)
            | (self.csrc_count() & 0x0F);
        buf.push(byte0);

        // 第 2 字节: M(1) | PT(7)
        let byte1 = ((self.marker as u8) << 7) | (self.payload_type & 0x7F);
        buf.push(byte1);

        // 序列号 (16 bits)
        buf.extend_from_slice(&self.sequence_number.to_be_bytes());

        // 时间戳 (32 bits)
        buf.extend_from_slice(&self.timestamp.to_be_bytes());

        // SSRC (32 bits)
        buf.extend_from_slice(&self.ssrc.to_be_bytes());

        // CSRC 列表
        for &csrc in &self.csrc_list {
            buf.extend_from_slice(&csrc.to_be_bytes());
        }

        // 头部扩展
        if let Some(ref ext) = self.extension_data {
            buf.extend_from_slice(&ext.profile.to_be_bytes());
            buf.extend_from_slice(&ext.word_count().to_be_bytes());
            buf.extend_from_slice(&ext.data);
        }

        buf
    }

    /// 从字节解析 RTP 头部
    ///
    /// 返回解析后的头部和消耗的字节数
    pub fn parse(data: &[u8]) -> Result<(Self, usize), MediaError> {
        if data.len() < RTP_HEADER_MIN_SIZE {
            return Err(MediaError::RtpParseError(
                "RTP packet too short: need at least 12 bytes".to_string(),
            ));
        }

        let byte0 = data[0];
        let version = (byte0 >> 6) & 0x03;
        if version != RTP_VERSION {
            return Err(MediaError::RtpParseError(format!(
                "invalid RTP version: expected 2, got {}",
                version
            )));
        }

        let padding = (byte0 & 0x20) != 0;
        let extension = (byte0 & 0x10) != 0;
        let csrc_count = byte0 & 0x0F;

        let byte1 = data[1];
        let marker = (byte1 & 0x80) != 0;
        let payload_type = byte1 & 0x7F;

        let sequence_number = u16::from_be_bytes([data[2], data[3]]);
        let timestamp = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let ssrc = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);

        // 解析 CSRC 列表
        let csrc_end = RTP_HEADER_MIN_SIZE + (csrc_count as usize) * 4;
        if data.len() < csrc_end {
            return Err(MediaError::RtpParseError(
                "RTP packet too short for CSRC list".to_string(),
            ));
        }

        let mut csrc_list = Vec::with_capacity(csrc_count as usize);
        for i in 0..csrc_count as usize {
            let offset = RTP_HEADER_MIN_SIZE + i * 4;
            csrc_list.push(u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]));
        }

        let mut offset = csrc_end;
        let mut extension_data = None;

        // 解析头部扩展
        if extension {
            if data.len() < offset + 4 {
                return Err(MediaError::RtpParseError(
                    "RTP packet too short for header extension".to_string(),
                ));
            }
            let profile = u16::from_be_bytes([data[offset], data[offset + 1]]);
            let length = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
            let ext_data_len = (length as usize) * 4;
            offset += 4;

            if data.len() < offset + ext_data_len {
                return Err(MediaError::RtpParseError(
                    "RTP packet too short for extension data".to_string(),
                ));
            }

            extension_data = Some(RtpHeaderExtension {
                profile,
                data: data[offset..offset + ext_data_len].to_vec(),
            });
            offset += ext_data_len;
        }

        let header = RtpHeader {
            version,
            padding,
            extension,

            marker,
            payload_type,
            sequence_number,
            timestamp,
            ssrc,
            csrc_list,
            extension_data,
        };

        Ok((header, offset))
    }
}

// ============================================================================
// RTP 包
// ============================================================================

/// RTP 包
///
/// 包含 RTP 头部和载荷数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtpPacket {
    /// RTP 头部
    pub header: RtpHeader,
    /// 载荷数据
    pub payload: Vec<u8>,
}

impl RtpPacket {
    /// 创建新的 RTP 包
    pub fn new(payload_type: u8, ssrc: u32, payload: Vec<u8>) -> Self {
        Self {
            header: RtpHeader::new(payload_type, ssrc),
            payload,
        }
    }

    /// 从字节解析 RTP 包
    ///
    /// # 参数
    ///
    /// - `data`: 包含完整 RTP 包的字节切片
    ///
    /// # 返回
    ///
    /// 解析成功返回 `RtpPacket`，失败返回 `MediaError`
    pub fn parse(data: &[u8]) -> Result<Self, MediaError> {
        let (header, header_size) = RtpHeader::parse(data)?;

        let payload_start = header_size;
        let mut payload_end = data.len();

        // 处理填充
        if header.padding && !data.is_empty() {
            let pad_len = data[data.len() - 1] as usize;
            if pad_len > data.len() - payload_start {
                return Err(MediaError::RtpParseError(
                    "invalid RTP padding length".to_string(),
                ));
            }
            payload_end = data.len() - pad_len;
        }

        if payload_start > payload_end {
            return Err(MediaError::RtpParseError(
                "invalid RTP payload boundaries".to_string(),
            ));
        }

        let payload = data[payload_start..payload_end].to_vec();

        Ok(Self { header, payload })
    }

    /// 序列化 RTP 包为字节
    ///
    /// # 返回
    ///
    /// 包含完整 RTP 包的字节向量
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = self.header.to_bytes();
        buf.extend_from_slice(&self.payload);

        // 处理填充
        if self.header.padding {
            let pad_len = 4 - (buf.len() % 4);
            if pad_len < 4 {
                for i in 0..pad_len {
                    buf.push(if i == pad_len - 1 { pad_len as u8 } else { 0 });
                }
            }
        }

        buf
    }

    /// 获取载荷类型
    pub fn payload_type(&self) -> u8 {
        self.header.payload_type
    }

    /// 获取序列号
    pub fn sequence_number(&self) -> u16 {
        self.header.sequence_number
    }

    /// 获取时间戳
    pub fn timestamp(&self) -> u32 {
        self.header.timestamp
    }

    /// 获取 SSRC
    pub fn ssrc(&self) -> u32 {
        self.header.ssrc
    }

    /// 获取标记位
    pub fn marker(&self) -> bool {
        self.header.marker
    }

    /// 获取载荷数据引用
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// 设置标记位
    pub fn set_marker(&mut self, marker: bool) {
        self.header.marker = marker;
    }

    /// 设置序列号
    pub fn set_sequence_number(&mut self, seq: u16) {
        self.header.sequence_number = seq;
    }

    /// 设置时间戳
    pub fn set_timestamp(&mut self, ts: u32) {
        self.header.timestamp = ts;
    }

    /// 添加 CSRC
    pub fn add_csrc(&mut self, csrc: u32) {
        if self.header.csrc_list.len() < 15 {
            self.header.csrc_list.push(csrc);
        }
    }

    /// 设置头部扩展
    pub fn set_extension(&mut self, ext: RtpHeaderExtension) {
        self.header.extension = true;
        self.header.extension_data = Some(ext);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtp_header_new() {
        let header = RtpHeader::new(96, 0x12345678);
        assert_eq!(header.version, 2);
        assert!(!header.padding);
        assert!(!header.extension);
        assert_eq!(header.csrc_count(), 0);
        assert!(!header.marker);
        assert_eq!(header.payload_type, 96);
        assert_eq!(header.ssrc, 0x12345678);
    }

    #[test]
    fn test_rtp_header_serialize_parse_roundtrip() {
        let mut header = RtpHeader::new(96, 0x12345678);
        header.marker = true;
        header.sequence_number = 42;
        header.timestamp = 123456;
        header.csrc_list.push(0xAABBCCDD);

        let bytes = header.to_bytes();
        let (parsed, consumed) = RtpHeader::parse(&bytes).unwrap();

        assert_eq!(consumed, bytes.len());
        assert_eq!(parsed.version, header.version);
        assert_eq!(parsed.padding, header.padding);
        assert_eq!(parsed.extension, header.extension);
        assert_eq!(parsed.csrc_count(), header.csrc_count());
        assert_eq!(parsed.marker, header.marker);
        assert_eq!(parsed.payload_type, header.payload_type);
        assert_eq!(parsed.sequence_number, header.sequence_number);
        assert_eq!(parsed.timestamp, header.timestamp);
        assert_eq!(parsed.ssrc, header.ssrc);
        assert_eq!(parsed.csrc_list, header.csrc_list);
    }

    #[test]
    fn test_rtp_header_with_extension() {
        let mut header = RtpHeader::new(96, 0x12345678);
        let ext = RtpHeaderExtension::one_byte(1, vec![0xAB, 0xCD]);
        header.extension = true;
        header.extension_data = Some(ext);

        let bytes = header.to_bytes();
        let (parsed, _) = RtpHeader::parse(&bytes).unwrap();

        assert!(parsed.extension);
        assert!(parsed.extension_data.is_some());
        let parsed_ext = parsed.extension_data.unwrap();
        assert_eq!(parsed_ext.profile, RTP_EXTENSION_PROFILE_ONE_BYTE);
    }

    #[test]
    fn test_rtp_packet_new() {
        let packet = RtpPacket::new(96, 0x12345678, vec![1, 2, 3, 4]);
        assert_eq!(packet.payload_type(), 96);
        assert_eq!(packet.ssrc(), 0x12345678);
        assert_eq!(packet.payload(), &[1, 2, 3, 4]);
    }

    #[test]
    fn test_rtp_packet_roundtrip() {
        let mut packet = RtpPacket::new(96, 0x12345678, vec![1, 2, 3, 4, 5]);
        packet.set_marker(true);
        packet.set_sequence_number(100);
        packet.set_timestamp(90000);
        packet.add_csrc(0xDEADBEEF);

        let bytes = packet.to_bytes();
        let parsed = RtpPacket::parse(&bytes).unwrap();

        assert_eq!(parsed.header.version, 2);
        assert!(parsed.header.marker);
        assert_eq!(parsed.payload_type(), 96);
        assert_eq!(parsed.sequence_number(), 100);
        assert_eq!(parsed.timestamp(), 90000);
        assert_eq!(parsed.ssrc(), 0x12345678);
        assert_eq!(parsed.payload(), &[1, 2, 3, 4, 5]);
        assert_eq!(parsed.header.csrc_list, vec![0xDEADBEEF]);
    }

    #[test]
    fn test_rtp_packet_with_extension_roundtrip() {
        let mut packet = RtpPacket::new(97, 0xAABBCCDD, vec![0xFF; 100]);
        let ext = RtpHeaderExtension::one_byte(3, vec![0x10, 0x20, 0x30]);
        packet.set_extension(ext);
        packet.set_sequence_number(200);
        packet.set_timestamp(180000);

        let bytes = packet.to_bytes();
        let parsed = RtpPacket::parse(&bytes).unwrap();

        assert_eq!(parsed.payload_type(), 97);
        assert_eq!(parsed.sequence_number(), 200);
        assert_eq!(parsed.timestamp(), 180000);
        assert!(parsed.header.extension);
        assert!(parsed.header.extension_data.is_some());
        assert_eq!(parsed.payload().len(), 100);
    }

    #[test]
    fn test_rtp_packet_min_size() {
        let packet = RtpPacket::new(0, 0, vec![]);
        let bytes = packet.to_bytes();
        assert_eq!(bytes.len(), RTP_HEADER_MIN_SIZE);

        let parsed = RtpPacket::parse(&bytes).unwrap();
        assert!(parsed.payload().is_empty());
    }

    #[test]
    fn test_rtp_parse_invalid_version() {
        let data = [0x80, 0x60, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]; // version = 2, ok
        let _ = RtpPacket::parse(&data).unwrap();

        let data = [0x40, 0x60, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]; // version = 1, invalid
        let result = RtpPacket::parse(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_rtp_parse_too_short() {
        let data = [0x80, 0x60, 0, 0]; // only 4 bytes
        let result = RtpPacket::parse(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_rtp_extension_one_byte() {
        let ext = RtpHeaderExtension::one_byte(1, vec![0xAA, 0xBB]);
        assert_eq!(ext.profile, RTP_EXTENSION_PROFILE_ONE_BYTE);
        // 一字节头部: (1 << 4) | (2-1) = 0x11, 然后 0xAA, 0xBB, 0x00(padding)
        assert_eq!(ext.data[0], 0x11);
        assert_eq!(ext.data[1], 0xAA);
        assert_eq!(ext.data[2], 0xBB);
        assert_eq!(ext.data[3], 0x00); // padding
    }

    #[test]
    fn test_rtp_extension_two_byte() {
        let ext = RtpHeaderExtension::two_byte(10, vec![0xCC, 0xDD, 0xEE]);
        assert_eq!(ext.profile, RTP_EXTENSION_PROFILE_TWO_BYTE);
        // 两字节头部: id=10, length=3
        assert_eq!(ext.data[0], 10);
        assert_eq!(ext.data[1], 3);
        assert_eq!(ext.data[2], 0xCC);
        assert_eq!(ext.data[3], 0xDD);
        assert_eq!(ext.data[4], 0xEE);
        assert_eq!(ext.data[5], 0x00); // padding
    }

    #[test]
    fn test_rtp_header_size_calculation() {
        let header = RtpHeader::new(96, 0x12345678);
        assert_eq!(header.size_in_bytes(), 12); // minimum header

        let mut header_with_csrc = header.clone();
        header_with_csrc.csrc_list.push(0x1);

        assert_eq!(header_with_csrc.size_in_bytes(), 16); // 12 + 4

        let mut header_with_ext = header;
        header_with_ext.extension = true;
        header_with_ext.extension_data = Some(RtpHeaderExtension::one_byte(1, vec![0xAB]));
        // extension: 4 (profile+length) + 4 (data, padded) = 8
        assert_eq!(header_with_ext.size_in_bytes(), 20); // 12 + 8
    }

    #[test]
    fn test_rtp_packet_with_padding() {
        let mut packet = RtpPacket::new(0, 0, vec![1, 2, 3, 4, 5]);
        packet.header.padding = true;

        let bytes = packet.to_bytes();
        let parsed = RtpPacket::parse(&bytes).unwrap();

        // 填充后载荷应与原始一致
        assert_eq!(parsed.payload(), &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_rtp_packet_known_bytes() {
        // 手动构造一个已知的 RTP 包
        // V=2, P=0, X=0, CC=0, M=1, PT=96, seq=1, ts=160, ssrc=0x12345678
        let data: Vec<u8> = vec![
            0x80, 0xE0, // V=2,P=0,X=0,CC=0 | M=1,PT=96
            0x00, 0x01, // seq=1
            0x00, 0x00, 0x00, 0xA0, // ts=160
            0x12, 0x34, 0x56, 0x78, // ssrc
            0xAA, 0xBB, 0xCC, 0xDD, // payload
        ];

        let packet = RtpPacket::parse(&data).unwrap();
        assert_eq!(packet.header.version, 2);
        assert!(!packet.header.padding);
        assert!(!packet.header.extension);
        assert_eq!(packet.header.csrc_count(), 0);
        assert!(packet.header.marker);
        assert_eq!(packet.payload_type(), 96);
        assert_eq!(packet.sequence_number(), 1);
        assert_eq!(packet.timestamp(), 160);
        assert_eq!(packet.ssrc(), 0x12345678);
        assert_eq!(packet.payload(), &[0xAA, 0xBB, 0xCC, 0xDD]);
    }
}
