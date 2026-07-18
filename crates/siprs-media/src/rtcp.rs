//! RTCP 包解析与构建
//!
//! 基于 RFC 3550 实现的 RTCP (RTP Control Protocol) 包处理模块。
//!
//! # 支持的 RTCP 包类型
//!
//! - **SR** (Sender Report) — 发送者报告
//! - **RR** (Receiver Report) — 接收者报告
//! - **SDES** (Source Description) — 源描述
//! - **BYE** — 离开会话通知
//! - **APP** — 应用定义的 RTCP 包
//!
//! # RTCP 包格式 (RFC 3550)
//!
//! ```text
//!  0                   1                   2                   3
//!  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |V=2|P|    RC   |       PT      |             length            |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                         payload                               |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! ```

use crate::MediaError;

// ============================================================================
// 常量
// ============================================================================

/// RTCP 版本号（RFC 3550 规定为 2）
pub const RTCP_VERSION: u8 = 2;

/// RTCP 包类型
pub mod packet_type {
    /// 发送者报告 (Sender Report)
    pub const SR: u8 = 200;
    /// 接收者报告 (Receiver Report)
    pub const RR: u8 = 201;
    /// 源描述 (Source Description)
    pub const SDES: u8 = 202;
    /// 离开通知 (BYE)
    pub const BYE: u8 = 203;
    /// 应用定义 (APP)
    pub const APP: u8 = 204;
}

/// RTCP 固定头部大小（4 字节）
const RTCP_HEADER_SIZE: usize = 4;

// ============================================================================
// 接收报告块
// ============================================================================

/// 接收报告块
///
/// 在 SR 和 RR 中使用，描述单个同步源的接收统计信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceptionReport {
    /// 此报告块所描述的 SSRC
    pub ssrc: u32,
    /// 自上次 SR 以来的丢包比例 (0-255, 1/256 精度)
    pub fraction_lost: u8,
    /// 累计丢包数
    pub cumulative_lost: u32,
    /// 已收到的最高扩展序列号
    pub extended_highest_seq: u32,
    /// 到达间隔抖动
    pub jitter: u32,
    /// 最近 SR 的时间戳 (NTP, 中间 32 位)
    pub last_sr: u32,
    /// 自最近 SR 以来的延迟 (1/65536 秒)
    pub delay_since_last_sr: u32,
}

impl ReceptionReport {
    /// 序列化为字节 (24 字节)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(24);
        buf.extend_from_slice(&self.ssrc.to_be_bytes());
        buf.push(self.fraction_lost);
        buf.extend_from_slice(&self.cumulative_lost.to_be_bytes()[1..4]); // 24 bits
        buf.extend_from_slice(&self.extended_highest_seq.to_be_bytes());
        buf.extend_from_slice(&self.jitter.to_be_bytes());
        buf.extend_from_slice(&self.last_sr.to_be_bytes());
        buf.extend_from_slice(&self.delay_since_last_sr.to_be_bytes());
        buf
    }

    /// 从字节解析
    pub fn parse(data: &[u8]) -> Result<Self, MediaError> {
        if data.len() < 24 {
            return Err(MediaError::RtcpParseError(
                "reception report too short".to_string(),
            ));
        }
        let ssrc = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let fraction_lost = data[4];
        let cumulative_lost = u32::from_be_bytes([0, data[5], data[6], data[7]]);
        let extended_highest_seq = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        let jitter = u32::from_be_bytes([data[12], data[13], data[14], data[15]]);
        let last_sr = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let delay_since_last_sr = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);

        Ok(Self {
            ssrc,
            fraction_lost,
            cumulative_lost,
            extended_highest_seq,
            jitter,
            last_sr,
            delay_since_last_sr,
        })
    }
}

// ============================================================================
// 发送者报告 (SR)
// ============================================================================

/// 发送者报告 (Sender Report)
///
/// RFC 3550 Section 6.4.1
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderReport {
    /// 产生此 SR 的 SSRC
    pub ssrc: u32,
    /// NTP 时间戳（最高 32 位）
    pub ntp_timestamp_msw: u32,
    /// NTP 时间戳（最低 32 位）
    pub ntp_timestamp_lsw: u32,
    /// RTP 时间戳
    pub rtp_timestamp: u32,
    /// 已发送的 RTP 包数
    pub sender_packet_count: u32,
    /// 已发送的 RTP 载荷字节数
    pub sender_octet_count: u32,
    /// 接收报告块列表
    pub reports: Vec<ReceptionReport>,
}

impl SenderReport {
    /// 创建新的发送者报告
    pub fn new(ssrc: u32) -> Self {
        Self {
            ssrc,
            ntp_timestamp_msw: 0,
            ntp_timestamp_lsw: 0,
            rtp_timestamp: 0,
            sender_packet_count: 0,
            sender_octet_count: 0,
            reports: Vec::new(),
        }
    }

    /// 计算此 SR 的 32 位字长度（不含固定头部）
    fn word_count(&self) -> u16 {
        // SSRC(4) + NTP(8) + RTP ts(4) + packet count(4) + octet count(4) = 24 bytes = 6 words
        // + 每个报告块 24 bytes = 6 words
        6 + (self.reports.len() as u16) * 6
    }

    /// 序列化为字节
    pub fn to_bytes(&self) -> Vec<u8> {
        let rc = self.reports.len().min(31) as u8;
        let length = self.word_count();

        let mut buf = Vec::with_capacity(4 + (length as usize) * 4);

        // 固定头部
        let byte0 = (RTCP_VERSION << 6) | (rc & 0x1F);
        buf.push(byte0);
        buf.push(packet_type::SR);
        buf.extend_from_slice(&length.to_be_bytes());

        // SSRC
        buf.extend_from_slice(&self.ssrc.to_be_bytes());
        // NTP 时间戳
        buf.extend_from_slice(&self.ntp_timestamp_msw.to_be_bytes());
        buf.extend_from_slice(&self.ntp_timestamp_lsw.to_be_bytes());
        // RTP 时间戳
        buf.extend_from_slice(&self.rtp_timestamp.to_be_bytes());
        // 发送者包计数
        buf.extend_from_slice(&self.sender_packet_count.to_be_bytes());
        // 发送者字节计数
        buf.extend_from_slice(&self.sender_octet_count.to_be_bytes());

        // 接收报告块
        for report in &self.reports {
            buf.extend_from_slice(&report.to_bytes());
        }

        buf
    }

    /// 从字节解析（不含固定头部，data 指向 SSRC 开始）
    fn parse_body(data: &[u8], rc: u8) -> Result<Self, MediaError> {
        if data.len() < 24 {
            return Err(MediaError::RtcpParseError(
                "sender report body too short".to_string(),
            ));
        }

        let ssrc = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let ntp_timestamp_msw = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let ntp_timestamp_lsw = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        let rtp_timestamp = u32::from_be_bytes([data[12], data[13], data[14], data[15]]);
        let sender_packet_count = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let sender_octet_count = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);

        let mut reports = Vec::with_capacity(rc as usize);
        let mut offset = 24;
        for _ in 0..rc {
            if offset + 24 > data.len() {
                break;
            }
            reports.push(ReceptionReport::parse(&data[offset..offset + 24])?);
            offset += 24;
        }

        Ok(Self {
            ssrc,
            ntp_timestamp_msw,
            ntp_timestamp_lsw,
            rtp_timestamp,
            sender_packet_count,
            sender_octet_count,
            reports,
        })
    }
}

// ============================================================================
// 接收者报告 (RR)
// ============================================================================

/// 接收者报告 (Receiver Report)
///
/// RFC 3550 Section 6.4.2
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiverReport {
    /// 产生此 RR 的 SSRC
    pub ssrc: u32,
    /// 接收报告块列表
    pub reports: Vec<ReceptionReport>,
}

impl ReceiverReport {
    /// 创建新的接收者报告
    pub fn new(ssrc: u32) -> Self {
        Self {
            ssrc,
            reports: Vec::new(),
        }
    }

    /// 计算此 RR 的 32 位字长度（不含固定头部）
    fn word_count(&self) -> u16 {
        // SSRC(4) = 1 word + 每个报告块 24 bytes = 6 words
        1 + (self.reports.len() as u16) * 6
    }

    /// 序列化为字节
    pub fn to_bytes(&self) -> Vec<u8> {
        let rc = self.reports.len().min(31) as u8;
        let length = self.word_count();

        let mut buf = Vec::with_capacity(4 + (length as usize) * 4);

        // 固定头部
        let byte0 = (RTCP_VERSION << 6) | (rc & 0x1F);
        buf.push(byte0);
        buf.push(packet_type::RR);
        buf.extend_from_slice(&length.to_be_bytes());

        // SSRC
        buf.extend_from_slice(&self.ssrc.to_be_bytes());

        // 接收报告块
        for report in &self.reports {
            buf.extend_from_slice(&report.to_bytes());
        }

        buf
    }

    /// 从字节解析（不含固定头部）
    fn parse_body(data: &[u8], rc: u8) -> Result<Self, MediaError> {
        if data.len() < 4 {
            return Err(MediaError::RtcpParseError(
                "receiver report body too short".to_string(),
            ));
        }

        let ssrc = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);

        let mut reports = Vec::with_capacity(rc as usize);
        let mut offset = 4;
        for _ in 0..rc {
            if offset + 24 > data.len() {
                break;
            }
            reports.push(ReceptionReport::parse(&data[offset..offset + 24])?);
            offset += 24;
        }

        Ok(Self { ssrc, reports })
    }
}

// ============================================================================
// 源描述 (SDES)
// ============================================================================

/// SDES 项目类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdesItemType {
    /// 结束标记
    End = 0,
    /// 规范名称 (CNAME)
    Cname = 1,
    /// 用户名
    Name = 2,
    /// 电子邮件
    Email = 3,
    /// 电话号码
    Phone = 4,
    /// 地理位置
    Loc = 5,
    /// 应用或工具名
    Tool = 6,
    /// 通知/状态
    Note = 7,
    /// 私有扩展
    Priv = 8,
}

impl SdesItemType {
    /// 从 u8 转换
    pub fn from_u8(val: u8) -> Self {
        match val {
            0 => SdesItemType::End,
            1 => SdesItemType::Cname,
            2 => SdesItemType::Name,
            3 => SdesItemType::Email,
            4 => SdesItemType::Phone,
            5 => SdesItemType::Loc,
            6 => SdesItemType::Tool,
            7 => SdesItemType::Note,
            8 => SdesItemType::Priv,
            _ => SdesItemType::Priv,
        }
    }

    /// 转换为 u8
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
}

/// SDES 项目
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdesItem {
    /// 项目类型
    pub item_type: SdesItemType,
    /// 项目值
    pub value: String,
}

impl SdesItem {
    /// 创建新的 SDES 项目
    pub fn new(item_type: SdesItemType, value: impl Into<String>) -> Self {
        Self {
            item_type,
            value: value.into(),
        }
    }

    /// 序列化为字节
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(self.item_type.as_u8());
        let val_bytes = self.value.as_bytes();
        buf.push(val_bytes.len() as u8);
        buf.extend_from_slice(val_bytes);
        buf
    }
}

/// SDES 块（对应一个 SSRC 的描述）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdesChunk {
    /// 此块描述的 SSRC
    pub ssrc: u32,
    /// SDES 项目列表
    pub items: Vec<SdesItem>,
}

impl SdesChunk {
    /// 创建新的 SDES 块
    pub fn new(ssrc: u32) -> Self {
        Self {
            ssrc,
            items: Vec::new(),
        }
    }

    /// 添加 SDES 项目
    pub fn add_item(&mut self, item_type: SdesItemType, value: impl Into<String>) {
        self.items.push(SdesItem::new(item_type, value));
    }

    /// 序列化为字节（含填充到 32 位对齐）
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.ssrc.to_be_bytes());

        for item in &self.items {
            buf.extend_from_slice(&item.to_bytes());
        }

        // 结束标记
        buf.push(SdesItemType::End as u8);

        // 填充到 32 位对齐
        while buf.len() % 4 != 0 {
            buf.push(0);
        }

        buf
    }
}

/// 源描述 (Source Description)
///
/// RFC 3550 Section 6.5
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDescription {
    /// SDES 块列表
    pub chunks: Vec<SdesChunk>,
}

impl SourceDescription {
    /// 创建新的源描述
    pub fn new() -> Self {
        Self { chunks: Vec::new() }
    }

    /// 计算此 SDES 的 32 位字长度（不含固定头部）
    fn word_count(&self) -> u16 {
        let mut total: usize = 0;
        for chunk in &self.chunks {
            total += chunk.to_bytes().len();
        }
        total.div_ceil(4) as u16
    }

    /// 序列化为字节
    pub fn to_bytes(&self) -> Vec<u8> {
        let sc = self.chunks.len().min(31) as u8;
        let length = self.word_count();

        let mut buf = Vec::new();

        // 固定头部
        let byte0 = (RTCP_VERSION << 6) | (sc & 0x1F);
        buf.push(byte0);
        buf.push(packet_type::SDES);
        buf.extend_from_slice(&length.to_be_bytes());

        // SDES 块
        for chunk in &self.chunks {
            buf.extend_from_slice(&chunk.to_bytes());
        }

        buf
    }

    /// 从字节解析（不含固定头部）
    fn parse_body(data: &[u8], sc: u8) -> Result<Self, MediaError> {
        let mut chunks = Vec::with_capacity(sc as usize);
        let mut offset: usize = 0;

        for _ in 0..sc {
            if offset + 4 > data.len() {
                break;
            }
            let ssrc = u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            offset += 4;

            let mut items = Vec::new();
            loop {
                if offset >= data.len() {
                    break;
                }
                let item_type = SdesItemType::from_u8(data[offset]);
                offset += 1;

                if item_type == SdesItemType::End {
                    // 填充到 32 位对齐
                    while offset % 4 != 0 {
                        offset += 1;
                    }
                    break;
                }

                if offset >= data.len() {
                    break;
                }
                let item_len = data[offset] as usize;
                offset += 1;

                if offset + item_len > data.len() {
                    break;
                }
                let value = String::from_utf8_lossy(&data[offset..offset + item_len]).to_string();
                offset += item_len;

                items.push(SdesItem { item_type, value });
            }

            chunks.push(SdesChunk { ssrc, items });
        }

        Ok(Self { chunks })
    }
}

impl Default for SourceDescription {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// BYE 包
// ============================================================================

/// BYE 包
///
/// RFC 3550 Section 6.6
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByePacket {
    /// 离开的 SSRC 列表
    pub ssrc_list: Vec<u32>,
    /// 离开原因（可选）
    pub reason: Option<String>,
}

impl ByePacket {
    /// 创建新的 BYE 包
    pub fn new() -> Self {
        Self {
            ssrc_list: Vec::new(),
            reason: None,
        }
    }

    /// 添加 SSRC
    pub fn add_ssrc(&mut self, ssrc: u32) {
        self.ssrc_list.push(ssrc);
    }

    /// 设置离开原因
    pub fn set_reason(&mut self, reason: impl Into<String>) {
        self.reason = Some(reason.into());
    }

    /// 计算此 BYE 的 32 位字长度（不含固定头部）
    fn word_count(&self) -> u16 {
        let mut total = self.ssrc_list.len() * 4;
        if let Some(ref reason) = self.reason {
            // reason 长度字节(1) + reason 内容 + 填充
            let reason_bytes = reason.as_bytes();
            total += 1 + reason_bytes.len();
        }
        total.div_ceil(4) as u16
    }

    /// 序列化为字节
    pub fn to_bytes(&self) -> Vec<u8> {
        let sc = self.ssrc_list.len().min(31) as u8;
        let length = self.word_count();

        let mut buf = Vec::new();

        // 固定头部
        let byte0 = (RTCP_VERSION << 6) | (sc & 0x1F);
        buf.push(byte0);
        buf.push(packet_type::BYE);
        buf.extend_from_slice(&length.to_be_bytes());

        // SSRC 列表
        for &ssrc in &self.ssrc_list {
            buf.extend_from_slice(&ssrc.to_be_bytes());
        }

        // 离开原因
        if let Some(ref reason) = self.reason {
            let reason_bytes = reason.as_bytes();
            buf.push(reason_bytes.len() as u8);
            buf.extend_from_slice(reason_bytes);
        }

        // 填充到 32 位对齐
        while buf.len() % 4 != 0 {
            buf.push(0);
        }

        buf
    }

    /// 从字节解析（不含固定头部）
    fn parse_body(data: &[u8], sc: u8) -> Result<Self, MediaError> {
        let ssrc_size = (sc as usize) * 4;
        if data.len() < ssrc_size {
            return Err(MediaError::RtcpParseError(
                "BYE packet too short for SSRC list".to_string(),
            ));
        }

        let mut ssrc_list = Vec::with_capacity(sc as usize);
        for i in 0..sc as usize {
            let offset = i * 4;
            ssrc_list.push(u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]));
        }

        let mut reason = None;
        let mut offset = ssrc_size;
        if offset < data.len() {
            let reason_len = data[offset] as usize;
            offset += 1;
            if offset + reason_len <= data.len() {
                reason =
                    Some(String::from_utf8_lossy(&data[offset..offset + reason_len]).to_string());
            }
        }

        Ok(Self { ssrc_list, reason })
    }
}

impl Default for ByePacket {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// APP 包
// ============================================================================

/// APP 包
///
/// RFC 3550 Section 6.7
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPacket {
    /// SSRC
    pub ssrc: u32,
    /// 应用名称 (4 字节 ASCII)
    pub name: [u8; 4],
    /// 应用特定数据
    pub data: Vec<u8>,
}

impl AppPacket {
    /// 创建新的 APP 包
    pub fn new(ssrc: u32, name: [u8; 4]) -> Self {
        Self {
            ssrc,
            name,
            data: Vec::new(),
        }
    }

    /// 计算此 APP 的 32 位字长度（不含固定头部）
    fn word_count(&self) -> u16 {
        // SSRC(4) + name(4) + data
        let total = 8 + self.data.len();
        total.div_ceil(4) as u16
    }

    /// 序列化为字节
    pub fn to_bytes(&self) -> Vec<u8> {
        let length = self.word_count();

        let mut buf = Vec::new();

        // 固定头部
        let byte0 = RTCP_VERSION << 6; // subtype = 0
        buf.push(byte0);
        buf.push(packet_type::APP);
        buf.extend_from_slice(&length.to_be_bytes());

        // SSRC
        buf.extend_from_slice(&self.ssrc.to_be_bytes());
        // Name
        buf.extend_from_slice(&self.name);
        // Data
        buf.extend_from_slice(&self.data);

        // 填充到 32 位对齐
        while buf.len() % 4 != 0 {
            buf.push(0);
        }

        buf
    }

    /// 从字节解析（不含固定头部）
    fn parse_body(data: &[u8]) -> Result<Self, MediaError> {
        if data.len() < 8 {
            return Err(MediaError::RtcpParseError(
                "APP packet body too short".to_string(),
            ));
        }

        let ssrc = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let name = [data[4], data[5], data[6], data[7]];
        let app_data = data[8..].to_vec();

        Ok(Self {
            ssrc,
            name,
            data: app_data,
        })
    }
}

// ============================================================================
// RTCP 包枚举
// ============================================================================

/// RTCP 包
///
/// 支持 RFC 3550 定义的五种标准 RTCP 包类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RtcpPacket {
    /// 发送者报告 (SR)
    SenderReport(SenderReport),
    /// 接收者报告 (RR)
    ReceiverReport(ReceiverReport),
    /// 源描述 (SDES)
    SourceDescription(SourceDescription),
    /// 离开通知 (BYE)
    Bye(ByePacket),
    /// 应用定义 (APP)
    App(AppPacket),
}

impl RtcpPacket {
    /// 从字节解析 RTCP 包
    ///
    /// 解析单个 RTCP 包（不支持复合包，复合包需调用方拆分）。
    pub fn parse(data: &[u8]) -> Result<Self, MediaError> {
        if data.len() < RTCP_HEADER_SIZE {
            return Err(MediaError::RtcpParseError(
                "RTCP packet too short".to_string(),
            ));
        }

        let byte0 = data[0];
        let version = (byte0 >> 6) & 0x03;
        if version != RTCP_VERSION {
            return Err(MediaError::RtcpParseError(format!(
                "invalid RTCP version: expected 2, got {}",
                version
            )));
        }

        let rc_sc = byte0 & 0x1F;
        let pt = data[1];
        let length = u16::from_be_bytes([data[2], data[3]]) as usize;
        let body_len = length * 4;

        if data.len() < RTCP_HEADER_SIZE + body_len {
            return Err(MediaError::RtcpParseError(
                "RTCP packet body too short".to_string(),
            ));
        }

        let body = &data[RTCP_HEADER_SIZE..RTCP_HEADER_SIZE + body_len];

        match pt {
            packet_type::SR => {
                let sr = SenderReport::parse_body(body, rc_sc)?;
                Ok(RtcpPacket::SenderReport(sr))
            }
            packet_type::RR => {
                let rr = ReceiverReport::parse_body(body, rc_sc)?;
                Ok(RtcpPacket::ReceiverReport(rr))
            }
            packet_type::SDES => {
                let sdes = SourceDescription::parse_body(body, rc_sc)?;
                Ok(RtcpPacket::SourceDescription(sdes))
            }
            packet_type::BYE => {
                let bye = ByePacket::parse_body(body, rc_sc)?;
                Ok(RtcpPacket::Bye(bye))
            }
            packet_type::APP => {
                let app = AppPacket::parse_body(body)?;
                Ok(RtcpPacket::App(app))
            }
            _ => Err(MediaError::RtcpParseError(format!(
                "unsupported RTCP packet type: {}",
                pt
            ))),
        }
    }

    /// 序列化 RTCP 包为字节
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            RtcpPacket::SenderReport(sr) => sr.to_bytes(),
            RtcpPacket::ReceiverReport(rr) => rr.to_bytes(),
            RtcpPacket::SourceDescription(sdes) => sdes.to_bytes(),
            RtcpPacket::Bye(bye) => bye.to_bytes(),
            RtcpPacket::App(app) => app.to_bytes(),
        }
    }

    /// 解析复合 RTCP 包
    ///
    /// RTCP 包通常以复合包的形式发送（RFC 3550 Section 6.1）。
    /// 此方法解析一个复合包中的所有 RTCP 子包。
    pub fn parse_compound(data: &[u8]) -> Result<Vec<Self>, MediaError> {
        let mut packets = Vec::new();
        let mut offset: usize = 0;

        while offset + RTCP_HEADER_SIZE <= data.len() {
            let length = u16::from_be_bytes([data[offset + 2], data[offset + 3]]) as usize;
            let packet_len = RTCP_HEADER_SIZE + length * 4;

            if offset + packet_len > data.len() {
                return Err(MediaError::RtcpParseError(
                    "compound RTCP packet truncated".to_string(),
                ));
            }

            packets.push(Self::parse(&data[offset..offset + packet_len])?);
            offset += packet_len;
        }

        Ok(packets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sender_report_roundtrip() {
        let mut sr = SenderReport::new(0x12345678);
        sr.ntp_timestamp_msw = 0xDEADBEEF;
        sr.ntp_timestamp_lsw = 0xCAFEBABE;
        sr.rtp_timestamp = 160;
        sr.sender_packet_count = 100;
        sr.sender_octet_count = 6400;

        let mut report = ReceptionReport {
            ssrc: 0xAABBCCDD,
            fraction_lost: 0,
            cumulative_lost: 0,
            extended_highest_seq: 100,
            jitter: 5,
            last_sr: 0,
            delay_since_last_sr: 0,
        };
        sr.reports.push(report.clone());

        let bytes = sr.to_bytes();
        let parsed = RtcpPacket::parse(&bytes).unwrap();

        if let RtcpPacket::SenderReport(parsed_sr) = parsed {
            assert_eq!(parsed_sr.ssrc, 0x12345678);
            assert_eq!(parsed_sr.ntp_timestamp_msw, 0xDEADBEEF);
            assert_eq!(parsed_sr.ntp_timestamp_lsw, 0xCAFEBABE);
            assert_eq!(parsed_sr.rtp_timestamp, 160);
            assert_eq!(parsed_sr.sender_packet_count, 100);
            assert_eq!(parsed_sr.sender_octet_count, 6400);
            assert_eq!(parsed_sr.reports.len(), 1);
            assert_eq!(parsed_sr.reports[0].ssrc, 0xAABBCCDD);
        } else {
            panic!("expected SenderReport");
        }
    }

    #[test]
    fn test_receiver_report_roundtrip() {
        let mut rr = ReceiverReport::new(0x11223344);
        rr.reports.push(ReceptionReport {
            ssrc: 0x55667788,
            fraction_lost: 1,
            cumulative_lost: 5,
            extended_highest_seq: 200,
            jitter: 10,
            last_sr: 12345,
            delay_since_last_sr: 678,
        });

        let bytes = rr.to_bytes();
        let parsed = RtcpPacket::parse(&bytes).unwrap();

        if let RtcpPacket::ReceiverReport(parsed_rr) = parsed {
            assert_eq!(parsed_rr.ssrc, 0x11223344);
            assert_eq!(parsed_rr.reports.len(), 1);
            assert_eq!(parsed_rr.reports[0].ssrc, 0x55667788);
            assert_eq!(parsed_rr.reports[0].fraction_lost, 1);
            assert_eq!(parsed_rr.reports[0].cumulative_lost, 5);
            assert_eq!(parsed_rr.reports[0].extended_highest_seq, 200);
            assert_eq!(parsed_rr.reports[0].jitter, 10);
        } else {
            panic!("expected ReceiverReport");
        }
    }

    #[test]
    fn test_sdes_roundtrip() {
        let mut sdes = SourceDescription::new();
        let mut chunk = SdesChunk::new(0x12345678);
        chunk.add_item(SdesItemType::Cname, "user@example.com");
        chunk.add_item(SdesItemType::Name, "Test User");
        sdes.chunks.push(chunk);

        let bytes = sdes.to_bytes();
        let parsed = RtcpPacket::parse(&bytes).unwrap();

        if let RtcpPacket::SourceDescription(parsed_sdes) = parsed {
            assert_eq!(parsed_sdes.chunks.len(), 1);
            assert_eq!(parsed_sdes.chunks[0].ssrc, 0x12345678);
            assert_eq!(parsed_sdes.chunks[0].items.len(), 2);
            assert_eq!(
                parsed_sdes.chunks[0].items[0].item_type,
                SdesItemType::Cname
            );
            assert_eq!(parsed_sdes.chunks[0].items[0].value, "user@example.com");
            assert_eq!(parsed_sdes.chunks[0].items[1].item_type, SdesItemType::Name);
            assert_eq!(parsed_sdes.chunks[0].items[1].value, "Test User");
        } else {
            panic!("expected SourceDescription");
        }
    }

    #[test]
    fn test_bye_roundtrip() {
        let mut bye = ByePacket::new();
        bye.add_ssrc(0x12345678);
        bye.add_ssrc(0xAABBCCDD);
        bye.set_reason("leaving");

        let bytes = bye.to_bytes();
        let parsed = RtcpPacket::parse(&bytes).unwrap();

        if let RtcpPacket::Bye(parsed_bye) = parsed {
            assert_eq!(parsed_bye.ssrc_list, vec![0x12345678, 0xAABBCCDD]);
            assert_eq!(parsed_bye.reason.as_deref(), Some("leaving"));
        } else {
            panic!("expected ByePacket");
        }
    }

    #[test]
    fn test_bye_no_reason() {
        let mut bye = ByePacket::new();
        bye.add_ssrc(0x12345678);

        let bytes = bye.to_bytes();
        let parsed = RtcpPacket::parse(&bytes).unwrap();

        if let RtcpPacket::Bye(parsed_bye) = parsed {
            assert_eq!(parsed_bye.ssrc_list, vec![0x12345678]);
            assert!(parsed_bye.reason.is_none());
        } else {
            panic!("expected ByePacket");
        }
    }

    #[test]
    fn test_app_roundtrip() {
        let mut app = AppPacket::new(0x12345678, *b"TEST");
        app.data.extend_from_slice(&[1, 2, 3, 4]);

        let bytes = app.to_bytes();
        let parsed = RtcpPacket::parse(&bytes).unwrap();

        if let RtcpPacket::App(parsed_app) = parsed {
            assert_eq!(parsed_app.ssrc, 0x12345678);
            assert_eq!(&parsed_app.name, b"TEST");
            assert_eq!(parsed_app.data, vec![1, 2, 3, 4]);
        } else {
            panic!("expected AppPacket");
        }
    }

    #[test]
    fn test_compound_rtcp() {
        // 构建复合 RTCP: SR + SDES
        let mut sr = SenderReport::new(0x12345678);
        sr.rtp_timestamp = 160;
        sr.sender_packet_count = 10;

        let mut sdes = SourceDescription::new();
        let mut chunk = SdesChunk::new(0x12345678);
        chunk.add_item(SdesItemType::Cname, "test@example.com");
        sdes.chunks.push(chunk);

        let mut compound = Vec::new();
        compound.extend_from_slice(&sr.to_bytes());
        compound.extend_from_slice(&sdes.to_bytes());

        let packets = RtcpPacket::parse_compound(&compound).unwrap();
        assert_eq!(packets.len(), 2);

        assert!(matches!(packets[0], RtcpPacket::SenderReport(_)));
        assert!(matches!(packets[1], RtcpPacket::SourceDescription(_)));
    }

    #[test]
    fn test_rtcp_parse_invalid_version() {
        let data = [0x40, 0xC8, 0x00, 0x01]; // version = 1
        let result = RtcpPacket::parse(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_rtcp_parse_too_short() {
        let data = [0x80, 0xC8]; // only 2 bytes
        let result = RtcpPacket::parse(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_reception_report_roundtrip() {
        let report = ReceptionReport {
            ssrc: 0xDEADBEEF,
            fraction_lost: 5,
            cumulative_lost: 100,
            extended_highest_seq: 65535,
            jitter: 42,
            last_sr: 0x12345678,
            delay_since_last_sr: 0x9ABCDEF0,
        };

        let bytes = report.to_bytes();
        assert_eq!(bytes.len(), 24);

        let parsed = ReceptionReport::parse(&bytes).unwrap();
        assert_eq!(parsed.ssrc, 0xDEADBEEF);
        assert_eq!(parsed.fraction_lost, 5);
        assert_eq!(parsed.cumulative_lost, 100);
        assert_eq!(parsed.extended_highest_seq, 65535);
        assert_eq!(parsed.jitter, 42);
        assert_eq!(parsed.last_sr, 0x12345678);
        assert_eq!(parsed.delay_since_last_sr, 0x9ABCDEF0);
    }

    #[test]
    fn test_sdes_item_type_conversion() {
        assert_eq!(SdesItemType::from_u8(1), SdesItemType::Cname);
        assert_eq!(SdesItemType::from_u8(2), SdesItemType::Name);
        assert_eq!(SdesItemType::from_u8(0), SdesItemType::End);
        assert_eq!(SdesItemType::Cname.as_u8(), 1);
        assert_eq!(SdesItemType::End.as_u8(), 0);
    }
}
