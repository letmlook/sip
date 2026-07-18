//! GB28181 SDP 扩展
//!
//! GB/T 28181 国标在标准 SDP (RFC 4566) 基础上增加了以下扩展：
//!
//! - `y=` 行：国标编码（SSRC），20 位数字
//! - `f=` 行：媒体格式描述，如 `v/2/4///a/1/8///`
//!
//! 此外，GB28181 对 SDP 的使用有特定约定：
//!
//! - 媒体编码默认使用 PS (Program Stream) 封装
//! - 音频编码通常使用 G.711A (PCMA) 或 G.711U (PCMU)
//! - INVITE 请求中的 SDP 遵循国标格式

use crate::builder::SdpBuilder;
use crate::types::*;

// ============================================================================
// GB28181 枚举类型
// ============================================================================

/// GB28181 视频编码类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoEncoding {
    /// H.264 编码
    H264,
    /// H.265 编码
    H265,
    /// PS (Program Stream) 封装，GB28181 默认
    PS,
}

impl VideoEncoding {
    /// 获取 RTP payload type（GB28181 约定值）
    pub fn payload_type(&self) -> u32 {
        match self {
            VideoEncoding::PS => 96,
            VideoEncoding::H264 => 97,
            VideoEncoding::H265 => 98,
        }
    }

    /// 获取 rtpmap 编码名称
    pub fn encoding_name(&self) -> &str {
        match self {
            VideoEncoding::PS => "PS",
            VideoEncoding::H264 => "H264",
            VideoEncoding::H265 => "H265",
        }
    }

    /// 获取时钟频率
    pub fn clock_rate(&self) -> u32 {
        90000 // 视频统一使用 90000
    }

    /// 获取 f= 行中的视频编码标识
    pub fn format_id(&self) -> &str {
        match self {
            VideoEncoding::PS => "2",   // GB28181 中 PS 对应编码格式 2
            VideoEncoding::H264 => "4", // H264 对应编码格式 4
            VideoEncoding::H265 => "5", // H265 对应编码格式 5
        }
    }
}

/// GB28181 音频编码类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioEncoding {
    /// G.711A (PCMA)，payload type 8
    G711A,
    /// G.711U (PCMU)，payload type 0
    G711U,
    /// G.722.1
    G7221,
    /// AAC
    AAC,
}

impl AudioEncoding {
    /// 获取 RTP payload type
    pub fn payload_type(&self) -> u32 {
        match self {
            AudioEncoding::G711A => 8,
            AudioEncoding::G711U => 0,
            AudioEncoding::G7221 => 9,
            AudioEncoding::AAC => 10,
        }
    }

    /// 获取 rtpmap 编码名称
    pub fn encoding_name(&self) -> &str {
        match self {
            AudioEncoding::G711A => "PCMA",
            AudioEncoding::G711U => "PCMU",
            AudioEncoding::G7221 => "G7221",
            AudioEncoding::AAC => "AAC",
        }
    }

    /// 获取时钟频率
    pub fn clock_rate(&self) -> u32 {
        match self {
            AudioEncoding::G711A => 8000,
            AudioEncoding::G711U => 8000,
            AudioEncoding::G7221 => 16000,
            AudioEncoding::AAC => 48000,
        }
    }

    /// 获取 f= 行中的音频编码标识
    pub fn format_id(&self) -> &str {
        match self {
            AudioEncoding::G711A => "1",
            AudioEncoding::G711U => "2",
            AudioEncoding::G7221 => "3",
            AudioEncoding::AAC => "4",
        }
    }

    /// 获取 f= 行中的音频采样率标识（单位 kHz）
    pub fn sample_rate_id(&self) -> &str {
        match self {
            AudioEncoding::G711A => "8",  // 8kHz
            AudioEncoding::G711U => "8",  // 8kHz
            AudioEncoding::G7221 => "16", // 16kHz
            AudioEncoding::AAC => "48",   // 48kHz
        }
    }
}

/// GB28181 流类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamType {
    /// 实时流
    Live,
    /// 历史回放流
    History,
    /// 录像下载流
    Download,
}

impl StreamType {
    /// 获取 f= 行中的流类型标识
    pub fn format_id(&self) -> &str {
        match self {
            StreamType::Live => "1",
            StreamType::History => "2",
            StreamType::Download => "3",
        }
    }
}

/// GB28181 媒体参数
#[derive(Debug, Clone)]
pub struct MediaParam {
    /// 视频编码
    pub video_encoding: VideoEncoding,
    /// 音频编码
    pub audio_encoding: AudioEncoding,
    /// 流类型
    pub stream_type: StreamType,
}

// ============================================================================
// GB28181 SDP 提取函数
// ============================================================================

/// 从 SDP 中提取 GB28181 国标编码 (y= 行)
///
/// GB28181 使用非标准行 `y=XXXXXXXXXXXXXXXXXXXX` 表示国标编码，
/// 通常为 20 位数字字符串。
///
/// # 参数
///
/// - `sdp`: 解析后的 SDP 会话描述
///
/// # 返回
///
/// 如果存在 y= 行，返回国标编码字符串；否则返回 None
pub fn extract_ssrc(sdp: &SessionDescription) -> Option<String> {
    sdp.ssrc.clone()
}

/// 从 SDP 中提取 GB28181 媒体格式 (f= 行)
///
/// GB28181 使用非标准行 `f=v/2/4///a/1/8///` 表示媒体格式，
/// 格式为 `v/<流类型>/<视频编码>///a/<音频编码>/<音频采样率>///`。
///
/// # 参数
///
/// - `sdp`: 解析后的 SDP 会话描述
///
/// # 返回
///
/// 如果存在 f= 行，返回媒体格式字符串；否则返回 None
pub fn extract_media_format(sdp: &SessionDescription) -> Option<String> {
    sdp.media_format.clone()
}

/// 从媒体描述的属性中提取 y= 行
///
/// 某些 GB28181 实现将 y= 行放在媒体描述的属性中
pub fn extract_ssrc_from_media(sdp: &SessionDescription) -> Option<String> {
    for media in &sdp.media_descriptions {
        for attr in &media.attributes {
            if attr.name == "y" {
                return attr.value.clone();
            }
        }
    }
    None
}

/// 从媒体描述的属性中提取 f= 行
pub fn extract_media_format_from_media(sdp: &SessionDescription) -> Option<String> {
    for media in &sdp.media_descriptions {
        for attr in &media.attributes {
            if attr.name == "f" {
                return attr.value.clone();
            }
        }
    }
    None
}

// ============================================================================
// GB28181 SDP 构建函数
// ============================================================================

/// 构建 GB28181 f= 行
///
/// GB28181 f= 行格式: `v/<流类型>/<视频编码>///a/<音频编码>/<音频采样率>///`
fn build_f_line(media_param: &MediaParam) -> String {
    let mut f = String::with_capacity(32);
    f.push_str("v/");
    f.push_str(media_param.stream_type.format_id());
    f.push('/');
    f.push_str(media_param.video_encoding.format_id());
    f.push_str("///a/");
    f.push_str(media_param.audio_encoding.format_id());
    f.push('/');
    f.push_str(media_param.audio_encoding.sample_rate_id());
    f.push_str("///");
    f
}

/// 构建 GB28181 视频点播 INVITE SDP
///
/// 生成符合 GB/T 28181 标准的 INVITE 请求 SDP 内容。
///
/// # 参数
///
/// - `device_id`: 20 位国标编码
/// - `server_ip`: 服务器 IP 地址
/// - `server_port`: 服务器媒体端口
/// - `media_param`: 媒体参数（视频编码、音频编码、流类型）
///
/// # 返回
///
/// 符合 GB28181 标准的 `SessionDescription`
///
/// # 示例
///
/// ```
/// use siprs_sdp::gb28181::*;
///
/// let sdp = build_invite_sdp(
///     "01234567890000000001",
///     "192.168.1.100",
///     5000,
///     &MediaParam {
///         video_encoding: VideoEncoding::PS,
///         audio_encoding: AudioEncoding::G711A,
///         stream_type: StreamType::Live,
///     },
/// );
///
/// let sdp_str = sdp.to_sdp_string();
/// assert!(sdp_str.contains("y=01234567890000000001"));
/// ```
pub fn build_invite_sdp(
    device_id: &str,
    server_ip: &str,
    server_port: u16,
    media_param: &MediaParam,
) -> SessionDescription {
    let origin = Origin {
        username: "-".to_string(),
        session_id: 0,
        session_version: 0,
        network_type: "IN".to_string(),
        address_type: "IP4".to_string(),
        unicast_address: server_ip.to_string(),
    };

    // 构建 f= 行
    // GB28181 f= 行格式: v/<流类型>/<视频编码>///a/<音频编码>/<音频采样率>///
    let f_line = build_f_line(media_param);

    // 构建媒体描述
    let video_pt = media_param.video_encoding.payload_type();
    let audio_pt = media_param.audio_encoding.payload_type();

    let media = MediaDescription::new(MediaType::Video, server_port as u32, "RTP/AVP")
        .with_formats(vec![video_pt.to_string(), audio_pt.to_string()])
        .with_rtpmap(
            video_pt,
            format!(
                "{}/{}",
                media_param.video_encoding.encoding_name(),
                media_param.video_encoding.clock_rate()
            ),
        )
        .with_rtpmap(
            audio_pt,
            format!(
                "{}/{}",
                media_param.audio_encoding.encoding_name(),
                media_param.audio_encoding.clock_rate()
            ),
        )
        .with_attribute("recvonly", None);

    SdpBuilder::new(origin, "Play")
        .connection(Connection::ipv4(server_ip))
        .time(0, 0)
        .media(media)
        .ssrc(device_id)
        .media_format(&f_line)
        .build()
}

/// 构建 GB28181 视频点播 200 OK 响应 SDP
///
/// 生成符合 GB/T 28181 标准的 200 OK 响应 SDP 内容。
///
/// # 参数
///
/// - `device_id`: 20 位国标编码
/// - `device_ip`: 设备 IP 地址
/// - `device_port`: 设备媒体端口
/// - `media_param`: 媒体参数
///
/// # 返回
///
/// 符合 GB28181 标准的 `SessionDescription`
pub fn build_ok_sdp(
    device_id: &str,
    device_ip: &str,
    device_port: u16,
    media_param: &MediaParam,
) -> SessionDescription {
    let origin = Origin {
        username: "-".to_string(),
        session_id: 0,
        session_version: 0,
        network_type: "IN".to_string(),
        address_type: "IP4".to_string(),
        unicast_address: device_ip.to_string(),
    };

    let f_line = build_f_line(media_param);

    let video_pt = media_param.video_encoding.payload_type();
    let audio_pt = media_param.audio_encoding.payload_type();

    let media = MediaDescription::new(MediaType::Video, device_port as u32, "RTP/AVP")
        .with_formats(vec![video_pt.to_string(), audio_pt.to_string()])
        .with_rtpmap(
            video_pt,
            format!(
                "{}/{}",
                media_param.video_encoding.encoding_name(),
                media_param.video_encoding.clock_rate()
            ),
        )
        .with_rtpmap(
            audio_pt,
            format!(
                "{}/{}",
                media_param.audio_encoding.encoding_name(),
                media_param.audio_encoding.clock_rate()
            ),
        )
        .with_attribute("sendonly", None);

    SdpBuilder::new(origin, "Play")
        .connection(Connection::ipv4(device_ip))
        .time(0, 0)
        .media(media)
        .ssrc(device_id)
        .media_format(&f_line)
        .build()
}

// ============================================================================
// NTP 时间戳与 ISO 8601 互转辅助函数
// ============================================================================

/// NTP 时间戳与 Unix 时间戳的偏移量（秒）
///
/// NTP 纪元从 1900-01-01 00:00:00 UTC 开始，
/// Unix 纪元从 1970-01-01 00:00:00 UTC 开始，
/// 两者相差 70 年 = 2208988800 秒。
const NTP_UNIX_OFFSET: u64 = 2_208_988_800;

/// 将 ISO 8601 格式的时间字符串转换为 NTP 时间戳
///
/// GB28181 SDP 的 `t=` 行使用 NTP 时间戳（自 1900-01-01 00:00:00 UTC 的秒数），
/// 而用户输入通常使用 ISO 8601 格式（如 `2024-01-01T00:00:00`）。
///
/// # 参数
///
/// - `datetime`: ISO 8601 格式的时间字符串，格式为 `YYYY-MM-DDTHH:MM:SS`
///
/// # 返回
///
/// 转换成功返回 NTP 时间戳，格式不合法返回 None
///
/// # 示例
///
/// ```
/// use siprs_sdp::gb28181::datetime_to_ntp;
///
/// let ntp = datetime_to_ntp("2024-01-01T00:00:00");
/// assert!(ntp.is_some());
/// // 2024-01-01T00:00:00 UTC 对应的 NTP 时间戳
/// assert_eq!(ntp.unwrap(), 3913056000);
/// ```
pub fn datetime_to_ntp(datetime: &str) -> Option<u64> {
    // 解析格式: YYYY-MM-DDTHH:MM:SS
    let parts: Vec<&str> = datetime.split('T').collect();
    if parts.len() != 2 {
        return None;
    }

    let date_parts: Vec<&str> = parts[0].split('-').collect();
    if date_parts.len() != 3 {
        return None;
    }

    let time_parts: Vec<&str> = parts[1].split(':').collect();
    if time_parts.len() != 3 {
        return None;
    }

    let year: u32 = date_parts[0].parse().ok()?;
    let month: u32 = date_parts[1].parse().ok()?;
    let day: u32 = date_parts[2].parse().ok()?;
    let hour: u32 = time_parts[0].parse().ok()?;
    let minute: u32 = time_parts[1].parse().ok()?;
    let second: u32 = time_parts[2].parse().ok()?;

    // 验证范围
    if !(1970..=2100).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    // 计算自 1970-01-01 00:00:00 UTC 以来的天数
    let days = days_since_unix_epoch(year, month, day)?;

    // 计算 Unix 时间戳
    let unix_ts = days * 86400 + (hour as u64) * 3600 + (minute as u64) * 60 + (second as u64);

    // 转换为 NTP 时间戳
    Some(unix_ts + NTP_UNIX_OFFSET)
}

/// 将 NTP 时间戳转换为 ISO 8601 格式的时间字符串
///
/// # 参数
///
/// - `ntp`: NTP 时间戳（自 1900-01-01 00:00:00 UTC 的秒数）
///
/// # 返回
///
/// ISO 8601 格式的时间字符串，格式为 `YYYY-MM-DDTHH:MM:SS`
///
/// # 示例
///
/// ```
/// use siprs_sdp::gb28181::ntp_to_datetime;
///
/// let datetime = ntp_to_datetime(3913056000);
/// assert_eq!(datetime, "2024-01-01T00:00:00");
/// ```
pub fn ntp_to_datetime(ntp: u64) -> String {
    let unix_ts = ntp.saturating_sub(NTP_UNIX_OFFSET);
    let (year, month, day, hour, minute, second) = unix_ts_to_datetime(unix_ts);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        year, month, day, hour, minute, second
    )
}

/// 计算自 Unix 纪元（1970-01-01）以来的天数
fn days_since_unix_epoch(year: u32, month: u32, day: u32) -> Option<u64> {
    // 每月天数（非闰年）
    const MONTH_DAYS: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    let mut days: u64 = 0;

    // 累加完整年份的天数
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }

    // 累加完整月份的天数
    for m in 1..month {
        days += MONTH_DAYS[(m - 1) as usize];
        if m == 2 && is_leap_year(year) {
            days += 1;
        }
    }

    // 加上当月天数
    days += (day - 1) as u64;

    Some(days)
}

/// 判断是否为闰年
fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// 将 Unix 时间戳转换为 (年, 月, 日, 时, 分, 秒)
fn unix_ts_to_datetime(unix_ts: u64) -> (u32, u32, u32, u32, u32, u32) {
    // 每月天数（非闰年）
    const MONTH_DAYS: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    let mut remaining = unix_ts;

    // 计算秒数
    let second = (remaining % 60) as u32;
    remaining /= 60;

    // 计算分钟
    let minute = (remaining % 60) as u32;
    remaining /= 60;

    // 计算小时
    let hour = (remaining % 24) as u32;
    remaining /= 24;

    // 计算年月日
    let mut year: u32 = 1970;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining < days_in_year as u64 {
            break;
        }
        remaining -= days_in_year as u64;
        year += 1;
    }

    let mut month: u32 = 1;
    loop {
        let days_in_month = if month == 2 && is_leap_year(year) {
            MONTH_DAYS[(month - 1) as usize] + 1
        } else {
            MONTH_DAYS[(month - 1) as usize]
        };
        if remaining < days_in_month as u64 {
            break;
        }
        remaining -= days_in_month as u64;
        month += 1;
    }

    let day = (remaining + 1) as u32;

    (year, month, day, hour, minute, second)
}

// ============================================================================
// 历史回放和录像下载 SDP 构建函数
// ============================================================================

/// 构建 GB28181 历史回放 INVITE SDP
///
/// 生成符合 GB/T 28181 标准的历史回放 INVITE 请求 SDP 内容。
/// 与实时点播的主要差异：
///
/// - `t=` 行包含时间范围（NTP 时间戳）
/// - `a=sendonly`（设备端发送，平台端接收）
/// - 会话名称为 "Playback"
/// - 流类型为 `StreamType::History`
///
/// # 参数
///
/// - `device_id`: 20 位国标编码
/// - `server_ip`: 服务器 IP 地址
/// - `server_port`: 服务器媒体端口
/// - `media_param`: 媒体参数（视频编码、音频编码、流类型）
/// - `start_time`: 开始时间，ISO 8601 格式（如 `2024-01-01T00:00:00`）
/// - `end_time`: 结束时间，ISO 8601 格式（如 `2024-01-01T23:59:59`）
///
/// # 返回
///
/// 符合 GB28181 标准的 `SessionDescription`
///
/// # 示例
///
/// ```
/// use siprs_sdp::gb28181::*;
///
/// let sdp = build_playback_invite_sdp(
///     "01234567890000000001",
///     "192.168.1.100",
///     5000,
///     &MediaParam {
///         video_encoding: VideoEncoding::PS,
///         audio_encoding: AudioEncoding::G711A,
///         stream_type: StreamType::History,
///     },
///     "2024-01-01T00:00:00",
///     "2024-01-01T23:59:59",
/// );
///
/// let sdp_str = sdp.to_sdp_string();
/// assert!(sdp_str.contains("y=01234567890000000001"));
/// assert!(sdp_str.contains("a=sendonly"));
/// ```
pub fn build_playback_invite_sdp(
    device_id: &str,
    server_ip: &str,
    server_port: u16,
    media_param: &MediaParam,
    start_time: &str,
    end_time: &str,
) -> SessionDescription {
    let origin = Origin {
        username: "-".to_string(),
        session_id: 0,
        session_version: 0,
        network_type: "IN".to_string(),
        address_type: "IP4".to_string(),
        unicast_address: server_ip.to_string(),
    };

    // 将 ISO 8601 时间转换为 NTP 时间戳
    // 如果转换失败，回退到 0（表示永久会话）
    let ntp_start = datetime_to_ntp(start_time).unwrap_or(0);
    let ntp_stop = datetime_to_ntp(end_time).unwrap_or(0);

    // 构建 f= 行
    // GB28181 f= 行格式: v/<流类型>/<视频编码>///a/<音频编码>/<音频采样率>///
    let f_line = build_f_line(media_param);

    // 构建媒体描述
    let video_pt = media_param.video_encoding.payload_type();
    let audio_pt = media_param.audio_encoding.payload_type();

    let media = MediaDescription::new(MediaType::Video, server_port as u32, "RTP/AVP")
        .with_formats(vec![video_pt.to_string(), audio_pt.to_string()])
        .with_rtpmap(
            video_pt,
            format!(
                "{}/{}",
                media_param.video_encoding.encoding_name(),
                media_param.video_encoding.clock_rate()
            ),
        )
        .with_rtpmap(
            audio_pt,
            format!(
                "{}/{}",
                media_param.audio_encoding.encoding_name(),
                media_param.audio_encoding.clock_rate()
            ),
        )
        // 历史回放：设备端发送，平台端接收
        .with_attribute("sendonly", None);

    SdpBuilder::new(origin, "Playback")
        .connection(Connection::ipv4(server_ip))
        .time(ntp_start, ntp_stop)
        .media(media)
        .ssrc(device_id)
        .media_format(&f_line)
        .build()
}

/// 构建 GB28181 录像下载 INVITE SDP
///
/// 生成符合 GB/T 28181 标准的录像下载 INVITE 请求 SDP 内容。
/// 与历史回放的主要差异：
///
/// - 下载速度可配置（通过 `a=downloadspeed` 属性）
/// - 会话名称为 "Download"
/// - 流类型为 `StreamType::Download`
///
/// # 参数
///
/// - `device_id`: 20 位国标编码
/// - `server_ip`: 服务器 IP 地址
/// - `server_port`: 服务器媒体端口
/// - `media_param`: 媒体参数（视频编码、音频编码、流类型）
/// - `start_time`: 开始时间，ISO 8601 格式（如 `2024-01-01T00:00:00`）
/// - `end_time`: 结束时间，ISO 8601 格式（如 `2024-01-01T23:59:59`）
/// - `download_speed`: 下载倍速（1/2/4），None 表示默认速度
///
/// # 返回
///
/// 符合 GB28181 标准的 `SessionDescription`
///
/// # 示例
///
/// ```
/// use siprs_sdp::gb28181::*;
///
/// let sdp = build_download_invite_sdp(
///     "01234567890000000001",
///     "192.168.1.100",
///     5000,
///     &MediaParam {
///         video_encoding: VideoEncoding::PS,
///         audio_encoding: AudioEncoding::G711A,
///         stream_type: StreamType::Download,
///     },
///     "2024-01-01T00:00:00",
///     "2024-01-01T23:59:59",
///     Some(4),
/// );
///
/// let sdp_str = sdp.to_sdp_string();
/// assert!(sdp_str.contains("a=downloadspeed:4"));
/// assert!(sdp_str.contains("a=sendonly"));
/// ```
pub fn build_download_invite_sdp(
    device_id: &str,
    server_ip: &str,
    server_port: u16,
    media_param: &MediaParam,
    start_time: &str,
    end_time: &str,
    download_speed: Option<u32>,
) -> SessionDescription {
    let origin = Origin {
        username: "-".to_string(),
        session_id: 0,
        session_version: 0,
        network_type: "IN".to_string(),
        address_type: "IP4".to_string(),
        unicast_address: server_ip.to_string(),
    };

    // 将 ISO 8601 时间转换为 NTP 时间戳
    let ntp_start = datetime_to_ntp(start_time).unwrap_or(0);
    let ntp_stop = datetime_to_ntp(end_time).unwrap_or(0);

    // 构建 f= 行
    // GB28181 f= 行格式: v/<流类型>/<视频编码>///a/<音频编码>/<音频采样率>///
    let f_line = build_f_line(media_param);

    // 构建媒体描述
    let video_pt = media_param.video_encoding.payload_type();
    let audio_pt = media_param.audio_encoding.payload_type();

    let mut media = MediaDescription::new(MediaType::Video, server_port as u32, "RTP/AVP")
        .with_formats(vec![video_pt.to_string(), audio_pt.to_string()])
        .with_rtpmap(
            video_pt,
            format!(
                "{}/{}",
                media_param.video_encoding.encoding_name(),
                media_param.video_encoding.clock_rate()
            ),
        )
        .with_rtpmap(
            audio_pt,
            format!(
                "{}/{}",
                media_param.audio_encoding.encoding_name(),
                media_param.audio_encoding.clock_rate()
            ),
        )
        // 录像下载：设备端发送，平台端接收
        .with_attribute("sendonly", None);

    // 添加下载速度属性
    if let Some(speed) = download_speed {
        media = media.with_attribute("downloadspeed", Some(speed.to_string()));
    }

    SdpBuilder::new(origin, "Download")
        .connection(Connection::ipv4(server_ip))
        .time(ntp_start, ntp_stop)
        .media(media)
        .ssrc(device_id)
        .media_format(&f_line)
        .build()
}

// ============================================================================
// 历史回放/录像下载 SDP 提取函数
// ============================================================================

/// 从 SDP 中提取时间范围（用于历史回放/录像下载）
///
/// 从 SDP 的 `t=` 行提取开始和结束时间，并转换为 ISO 8601 格式。
///
/// # 参数
///
/// - `sdp`: 解析后的 SDP 会话描述
///
/// # 返回
///
/// 如果存在有效的时间范围（`t=` 行中时间不为 0），返回 `(开始时间, 结束时间)` 的元组；
/// 如果 `t=` 行为 `t=0 0`（实时点播），返回 None
///
/// # 示例
///
/// ```
/// use siprs_sdp::gb28181::*;
///
/// let sdp = build_playback_invite_sdp(
///     "01234567890000000001",
///     "192.168.1.100",
///     5000,
///     &MediaParam {
///         video_encoding: VideoEncoding::PS,
///         audio_encoding: AudioEncoding::G711A,
///         stream_type: StreamType::History,
///     },
///     "2024-01-01T00:00:00",
///     "2024-01-01T23:59:59",
/// );
///
/// let time_range = extract_time_range(&sdp);
/// assert!(time_range.is_some());
/// let (start, end) = time_range.unwrap();
/// assert!(start.starts_with("2024-01-01"));
/// ```
pub fn extract_time_range(sdp: &SessionDescription) -> Option<(String, String)> {
    // 查找第一个非零时间范围
    for td in &sdp.time_descriptions {
        // t=0 0 表示永久会话（实时点播），不返回时间范围
        if td.start_time != 0 || td.stop_time != 0 {
            let start = ntp_to_datetime(td.start_time);
            let end = ntp_to_datetime(td.stop_time);
            return Some((start, end));
        }
    }
    None
}

/// 从 SDP 中提取流类型（实时/回放/下载）
///
/// 通过以下方式判断流类型：
/// 1. 优先从 `f=` 行的流类型标识判断
/// 2. 如果 `f=` 行不存在，从会话名称判断
///
/// # 参数
///
/// - `sdp`: 解析后的 SDP 会话描述
///
/// # 返回
///
/// 如果能判断流类型，返回 `StreamType`；否则返回 None
///
/// # 示例
///
/// ```
/// use siprs_sdp::gb28181::*;
///
/// let sdp = build_invite_sdp(
///     "01234567890000000001",
///     "192.168.1.100",
///     5000,
///     &MediaParam {
///         video_encoding: VideoEncoding::PS,
///         audio_encoding: AudioEncoding::G711A,
///         stream_type: StreamType::Live,
///     },
/// );
///
/// assert_eq!(extract_stream_type(&sdp), Some(StreamType::Live));
/// ```
pub fn extract_stream_type(sdp: &SessionDescription) -> Option<StreamType> {
    // 优先从 f= 行判断
    if let Some(ref f_line) = sdp.media_format {
        let parsed = parse_media_format(f_line);
        if let Some(pm) = parsed {
            return match pm.stream_type.as_str() {
                "1" => Some(StreamType::Live),
                "2" => Some(StreamType::History),
                "3" => Some(StreamType::Download),
                _ => None,
            };
        }
    }

    // 从会话名称判断
    match sdp.session_name.as_str() {
        "Play" => Some(StreamType::Live),
        "Playback" => Some(StreamType::History),
        "Download" => Some(StreamType::Download),
        _ => None,
    }
}

/// 解析 f= 行获取媒体参数
///
/// 将 GB28181 f= 行解析为结构化的媒体参数信息。
///
/// # 参数
///
/// - `f_line`: f= 行内容，如 `v/2/4///a/1/8///`
///
/// # 返回
///
/// 解析成功返回 `ParsedMediaFormat`，失败返回 None
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMediaFormat {
    /// 流类型标识
    pub stream_type: String,
    /// 视频编码标识
    pub video_encoding: String,
    /// 音频编码标识
    pub audio_encoding: String,
    /// 音频采样率标识（kHz）
    pub audio_sample_rate: String,
}

/// 解析 f= 行
pub fn parse_media_format(f_line: &str) -> Option<ParsedMediaFormat> {
    // GB28181 f= 行格式: v/<stream_type>/<video_encoding>///a/<audio_encoding>/<audio_sample_rate>///
    let parts: Vec<&str> = f_line.split('/').collect();

    // 期望格式: ["v", stream_type, video_encoding, "", "", "a", audio_encoding, audio_sample_rate, "", ""]
    if parts.len() < 10 {
        return None;
    }

    if parts[0] != "v" || parts[5] != "a" {
        return None;
    }

    Some(ParsedMediaFormat {
        stream_type: parts[1].to_string(),
        video_encoding: parts[2].to_string(),
        audio_encoding: parts[6].to_string(),
        audio_sample_rate: parts[7].to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_ssrc() {
        let sdp = SdpBuilder::new(Origin::ipv4("-", "192.168.1.1"), "Test")
            .time(0, 0)
            .ssrc("01234567890000000001")
            .build();

        assert_eq!(extract_ssrc(&sdp), Some("01234567890000000001".to_string()));
    }

    #[test]
    fn test_extract_ssrc_none() {
        let sdp = SdpBuilder::new(Origin::ipv4("-", "192.168.1.1"), "Test")
            .time(0, 0)
            .build();

        assert_eq!(extract_ssrc(&sdp), None);
    }

    #[test]
    fn test_extract_media_format() {
        let sdp = SdpBuilder::new(Origin::ipv4("-", "192.168.1.1"), "Test")
            .time(0, 0)
            .media_format("v/2/4///a/1/8///")
            .build();

        assert_eq!(
            extract_media_format(&sdp),
            Some("v/2/4///a/1/8///".to_string())
        );
    }

    #[test]
    fn test_extract_media_format_none() {
        let sdp = SdpBuilder::new(Origin::ipv4("-", "192.168.1.1"), "Test")
            .time(0, 0)
            .build();

        assert_eq!(extract_media_format(&sdp), None);
    }

    #[test]
    fn test_build_invite_sdp_ps_pcma() {
        let sdp = build_invite_sdp(
            "01234567890000000001",
            "192.168.1.100",
            5000,
            &MediaParam {
                video_encoding: VideoEncoding::PS,
                audio_encoding: AudioEncoding::G711A,
                stream_type: StreamType::Live,
            },
        );

        assert_eq!(sdp.version, 0);
        assert_eq!(sdp.session_name, "Play");
        assert!(sdp.connection.is_some());
        assert_eq!(sdp.media_descriptions.len(), 1);
        assert_eq!(sdp.ssrc.as_deref(), Some("01234567890000000001"));
        assert!(sdp.media_format.is_some());

        let media = &sdp.media_descriptions[0];
        assert_eq!(media.media, MediaType::Video);
        assert_eq!(media.port, 5000);
        assert_eq!(media.proto, "RTP/AVP");
        assert!(media.formats.contains(&"96".to_string())); // PS
        assert!(media.formats.contains(&"8".to_string())); // PCMA

        // 验证 rtpmap
        let rtpmap_96 = media.attributes.iter().find(|a| {
            a.name == "rtpmap" && a.value.as_ref().map_or(false, |v| v.starts_with("96"))
        });
        assert!(rtpmap_96.is_some());

        let rtpmap_8 = media
            .attributes
            .iter()
            .find(|a| a.name == "rtpmap" && a.value.as_ref().map_or(false, |v| v.starts_with("8")));
        assert!(rtpmap_8.is_some());

        // 验证 recvonly
        let recvonly = media.attributes.iter().find(|a| a.name == "recvonly");
        assert!(recvonly.is_some());
    }

    #[test]
    fn test_build_invite_sdp_h264_pcma() {
        let sdp = build_invite_sdp(
            "01234567890000000001",
            "192.168.1.100",
            5000,
            &MediaParam {
                video_encoding: VideoEncoding::H264,
                audio_encoding: AudioEncoding::G711A,
                stream_type: StreamType::Live,
            },
        );

        let media = &sdp.media_descriptions[0];
        assert!(media.formats.contains(&"97".to_string())); // H264
        assert!(media.formats.contains(&"8".to_string())); // PCMA
    }

    #[test]
    fn test_build_invite_sdp_h265_aac() {
        let sdp = build_invite_sdp(
            "01234567890000000001",
            "192.168.1.100",
            5000,
            &MediaParam {
                video_encoding: VideoEncoding::H265,
                audio_encoding: AudioEncoding::AAC,
                stream_type: StreamType::History,
            },
        );

        let media = &sdp.media_descriptions[0];
        assert!(media.formats.contains(&"98".to_string())); // H265
        assert!(media.formats.contains(&"10".to_string())); // AAC
    }

    #[test]
    fn test_build_invite_sdp_serialization() {
        let sdp = build_invite_sdp(
            "01234567890000000001",
            "192.168.1.100",
            5000,
            &MediaParam {
                video_encoding: VideoEncoding::PS,
                audio_encoding: AudioEncoding::G711A,
                stream_type: StreamType::Live,
            },
        );

        let sdp_str = sdp.to_sdp_string();

        assert!(sdp_str.contains("v=0\r\n"));
        assert!(sdp_str.contains("s=Play\r\n"));
        assert!(sdp_str.contains("c=IN IP4 192.168.1.100\r\n"));
        assert!(sdp_str.contains("t=0 0\r\n"));
        assert!(sdp_str.contains("m=video 5000 RTP/AVP"));
        assert!(sdp_str.contains("a=rtpmap:96 PS/90000\r\n"));
        assert!(sdp_str.contains("a=rtpmap:8 PCMA/8000\r\n"));
        assert!(sdp_str.contains("a=recvonly\r\n"));
        assert!(sdp_str.contains("y=01234567890000000001\r\n"));
        // f= 行格式: v/<流类型>/<视频编码>///a/<音频编码>/<音频采样率>///
        // Live=1, PS=2, G711A=1, 8kHz=8 -> v/1/2///a/1/8///
        assert!(sdp_str.contains("f=v/1/2///a/1/8///\r\n"));
    }

    #[test]
    fn test_build_ok_sdp() {
        let sdp = build_ok_sdp(
            "01234567890000000001",
            "192.168.1.200",
            6000,
            &MediaParam {
                video_encoding: VideoEncoding::PS,
                audio_encoding: AudioEncoding::G711A,
                stream_type: StreamType::Live,
            },
        );

        let media = &sdp.media_descriptions[0];
        assert_eq!(media.port, 6000);

        // 200 OK 应该是 sendonly
        let sendonly = media.attributes.iter().find(|a| a.name == "sendonly");
        assert!(sendonly.is_some());
    }

    #[test]
    fn test_video_encoding_properties() {
        assert_eq!(VideoEncoding::PS.payload_type(), 96);
        assert_eq!(VideoEncoding::PS.encoding_name(), "PS");
        assert_eq!(VideoEncoding::PS.clock_rate(), 90000);
        assert_eq!(VideoEncoding::PS.format_id(), "2");

        assert_eq!(VideoEncoding::H264.payload_type(), 97);
        assert_eq!(VideoEncoding::H264.encoding_name(), "H264");
        assert_eq!(VideoEncoding::H264.format_id(), "4");

        assert_eq!(VideoEncoding::H265.payload_type(), 98);
        assert_eq!(VideoEncoding::H265.encoding_name(), "H265");
        assert_eq!(VideoEncoding::H265.format_id(), "5");
    }

    #[test]
    fn test_audio_encoding_properties() {
        assert_eq!(AudioEncoding::G711A.payload_type(), 8);
        assert_eq!(AudioEncoding::G711A.encoding_name(), "PCMA");
        assert_eq!(AudioEncoding::G711A.clock_rate(), 8000);
        assert_eq!(AudioEncoding::G711A.format_id(), "1");

        assert_eq!(AudioEncoding::G711U.payload_type(), 0);
        assert_eq!(AudioEncoding::G711U.encoding_name(), "PCMU");
        assert_eq!(AudioEncoding::G711U.format_id(), "2");

        assert_eq!(AudioEncoding::G7221.payload_type(), 9);
        assert_eq!(AudioEncoding::G7221.clock_rate(), 16000);
        assert_eq!(AudioEncoding::G7221.format_id(), "3");

        assert_eq!(AudioEncoding::AAC.payload_type(), 10);
        assert_eq!(AudioEncoding::AAC.clock_rate(), 48000);
        assert_eq!(AudioEncoding::AAC.format_id(), "4");
    }

    #[test]
    fn test_stream_type_properties() {
        assert_eq!(StreamType::Live.format_id(), "1");
        assert_eq!(StreamType::History.format_id(), "2");
        assert_eq!(StreamType::Download.format_id(), "3");
    }

    #[test]
    fn test_parse_media_format() {
        let parsed = parse_media_format("v/2/4///a/1/8///").unwrap();
        assert_eq!(parsed.stream_type, "2");
        assert_eq!(parsed.video_encoding, "4");
        assert_eq!(parsed.audio_encoding, "1");
        assert_eq!(parsed.audio_sample_rate, "8");
    }

    #[test]
    fn test_parse_media_format_invalid() {
        assert!(parse_media_format("invalid").is_none());
        assert!(parse_media_format("v/1/2/a/3").is_none());
    }

    #[test]
    fn test_gb28181_roundtrip() {
        // 构建 -> 序列化 -> 解析 -> 验证
        let sdp = build_invite_sdp(
            "01234567890000000001",
            "192.168.1.100",
            5000,
            &MediaParam {
                video_encoding: VideoEncoding::PS,
                audio_encoding: AudioEncoding::G711A,
                stream_type: StreamType::Live,
            },
        );

        let sdp_str = sdp.to_sdp_string();
        let reparsed = crate::parser::SdpParser::parse(&sdp_str).unwrap();

        assert_eq!(reparsed.ssrc.as_deref(), Some("01234567890000000001"));
        assert!(reparsed.media_format.is_some());

        // 验证媒体描述
        assert_eq!(reparsed.media_descriptions.len(), 1);
        let media = &reparsed.media_descriptions[0];
        assert_eq!(media.media, MediaType::Video);
        assert_eq!(media.port, 5000);
    }

    #[test]
    fn test_extract_ssrc_from_parsed_sdp() {
        let sdp_text = "\
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

        let sdp = crate::parser::SdpParser::parse(sdp_text).unwrap();

        assert_eq!(extract_ssrc(&sdp), Some("01234567890000000001".to_string()));
        assert_eq!(
            extract_media_format(&sdp),
            Some("v/2/4///a/1/8///".to_string())
        );
    }

    // ========================================================================
    // NTP 时间戳转换测试
    // ========================================================================

    #[test]
    fn test_datetime_to_ntp_basic() {
        // 2024-01-01T00:00:00 UTC
        // Unix timestamp: 1704067200
        // NTP timestamp: 1704067200 + 2208988800 = 3913056000
        let ntp = datetime_to_ntp("2024-01-01T00:00:00").unwrap();
        assert_eq!(ntp, 3_913_056_000);
    }

    #[test]
    fn test_datetime_to_ntp_epoch() {
        // 1970-01-01T00:00:00 UTC = Unix epoch
        // Unix timestamp: 0
        // NTP timestamp: 2208988800
        let ntp = datetime_to_ntp("1970-01-01T00:00:00").unwrap();
        assert_eq!(ntp, NTP_UNIX_OFFSET);
    }

    #[test]
    fn test_datetime_to_ntp_invalid() {
        assert!(datetime_to_ntp("invalid").is_none());
        assert!(datetime_to_ntp("2024-01-01").is_none()); // 缺少时间部分
        assert!(datetime_to_ntp("2024-13-01T00:00:00").is_none()); // 无效月份
        assert!(datetime_to_ntp("2024-01-32T00:00:00").is_none()); // 无效日期
        assert!(datetime_to_ntp("2024-01-01T25:00:00").is_none()); // 无效小时
    }

    #[test]
    fn test_ntp_to_datetime_basic() {
        // 2024-01-01T00:00:00 UTC
        let datetime = ntp_to_datetime(3_913_056_000);
        assert_eq!(datetime, "2024-01-01T00:00:00");
    }

    #[test]
    fn test_ntp_to_datetime_epoch() {
        let datetime = ntp_to_datetime(NTP_UNIX_OFFSET);
        assert_eq!(datetime, "1970-01-01T00:00:00");
    }

    #[test]
    fn test_datetime_ntp_roundtrip() {
        // 测试多个时间点的往返转换
        let test_cases = [
            "2024-01-01T00:00:00",
            "2024-06-15T12:30:45",
            "2023-12-31T23:59:59",
            "2000-01-01T00:00:00",
        ];

        for datetime_str in &test_cases {
            let ntp = datetime_to_ntp(datetime_str).unwrap();
            let result = ntp_to_datetime(ntp);
            assert_eq!(&result, datetime_str, "往返转换失败: {}", datetime_str);
        }
    }

    // ========================================================================
    // 历史回放 SDP 测试
    // ========================================================================

    #[test]
    fn test_build_playback_invite_sdp_basic() {
        let sdp = build_playback_invite_sdp(
            "01234567890000000001",
            "192.168.1.100",
            5000,
            &MediaParam {
                video_encoding: VideoEncoding::PS,
                audio_encoding: AudioEncoding::G711A,
                stream_type: StreamType::History,
            },
            "2024-01-01T00:00:00",
            "2024-01-01T23:59:59",
        );

        // 验证基本字段
        assert_eq!(sdp.version, 0);
        assert_eq!(sdp.session_name, "Playback");
        assert!(sdp.connection.is_some());
        assert_eq!(sdp.media_descriptions.len(), 1);
        assert_eq!(sdp.ssrc.as_deref(), Some("01234567890000000001"));

        // 验证媒体描述
        let media = &sdp.media_descriptions[0];
        assert_eq!(media.media, MediaType::Video);
        assert_eq!(media.port, 5000);

        // 验证 sendonly（设备端发送）
        let sendonly = media.attributes.iter().find(|a| a.name == "sendonly");
        assert!(sendonly.is_some(), "历史回放 SDP 应包含 a=sendonly");

        // 验证 recvonly 不存在
        let recvonly = media.attributes.iter().find(|a| a.name == "recvonly");
        assert!(recvonly.is_none(), "历史回放 SDP 不应包含 a=recvonly");

        // 验证 f= 行中的流类型标识
        assert!(sdp.media_format.is_some());
        let f_line = sdp.media_format.as_ref().unwrap();
        assert!(f_line.starts_with("v/2/"), "历史回放 f= 行应以 v/2/ 开头");
    }

    #[test]
    fn test_build_playback_invite_sdp_time_range() {
        let sdp = build_playback_invite_sdp(
            "01234567890000000001",
            "192.168.1.100",
            5000,
            &MediaParam {
                video_encoding: VideoEncoding::PS,
                audio_encoding: AudioEncoding::G711A,
                stream_type: StreamType::History,
            },
            "2024-01-01T00:00:00",
            "2024-01-01T23:59:59",
        );

        // 验证 t= 行包含时间范围（非 0 0）
        assert_eq!(sdp.time_descriptions.len(), 1);
        let td = &sdp.time_descriptions[0];
        assert_ne!(td.start_time, 0, "历史回放 t= 行开始时间不应为 0");
        assert_ne!(td.stop_time, 0, "历史回放 t= 行结束时间不应为 0");

        // 验证 NTP 时间戳正确性
        let expected_start = datetime_to_ntp("2024-01-01T00:00:00").unwrap();
        let expected_stop = datetime_to_ntp("2024-01-01T23:59:59").unwrap();
        assert_eq!(td.start_time, expected_start);
        assert_eq!(td.stop_time, expected_stop);
    }

    #[test]
    fn test_build_playback_invite_sdp_serialization() {
        let sdp = build_playback_invite_sdp(
            "01234567890000000001",
            "192.168.1.100",
            5000,
            &MediaParam {
                video_encoding: VideoEncoding::PS,
                audio_encoding: AudioEncoding::G711A,
                stream_type: StreamType::History,
            },
            "2024-01-01T00:00:00",
            "2024-01-01T23:59:59",
        );

        let sdp_str = sdp.to_sdp_string();

        assert!(sdp_str.contains("v=0\r\n"));
        assert!(sdp_str.contains("s=Playback\r\n"));
        assert!(sdp_str.contains("c=IN IP4 192.168.1.100\r\n"));
        assert!(sdp_str.contains("m=video 5000 RTP/AVP"));
        assert!(sdp_str.contains("a=rtpmap:96 PS/90000\r\n"));
        assert!(sdp_str.contains("a=rtpmap:8 PCMA/8000\r\n"));
        assert!(sdp_str.contains("a=sendonly\r\n"));
        assert!(sdp_str.contains("y=01234567890000000001\r\n"));
        // f= 行格式: History=2, PS=2, G711A=1, 8kHz=8 -> v/2/2///a/1/8///
        assert!(sdp_str.contains("f=v/2/2///a/1/8///\r\n"));

        // 验证 t= 行不是 0 0
        assert!(!sdp_str.contains("t=0 0\r\n"), "历史回放 t= 行不应为 0 0");
    }

    // ========================================================================
    // 录像下载 SDP 测试
    // ========================================================================

    #[test]
    fn test_build_download_invite_sdp_basic() {
        let sdp = build_download_invite_sdp(
            "01234567890000000001",
            "192.168.1.100",
            5000,
            &MediaParam {
                video_encoding: VideoEncoding::PS,
                audio_encoding: AudioEncoding::G711A,
                stream_type: StreamType::Download,
            },
            "2024-01-01T00:00:00",
            "2024-01-01T23:59:59",
            Some(4),
        );

        // 验证基本字段
        assert_eq!(sdp.version, 0);
        assert_eq!(sdp.session_name, "Download");
        assert!(sdp.connection.is_some());
        assert_eq!(sdp.media_descriptions.len(), 1);
        assert_eq!(sdp.ssrc.as_deref(), Some("01234567890000000001"));

        // 验证媒体描述
        let media = &sdp.media_descriptions[0];
        assert_eq!(media.media, MediaType::Video);
        assert_eq!(media.port, 5000);

        // 验证 sendonly
        let sendonly = media.attributes.iter().find(|a| a.name == "sendonly");
        assert!(sendonly.is_some(), "录像下载 SDP 应包含 a=sendonly");

        // 验证下载速度属性
        let downloadspeed = media.attributes.iter().find(|a| a.name == "downloadspeed");
        assert!(
            downloadspeed.is_some(),
            "录像下载 SDP 应包含 a=downloadspeed"
        );
        assert_eq!(downloadspeed.unwrap().value.as_deref(), Some("4"));

        // 验证 f= 行中的流类型标识
        assert!(sdp.media_format.is_some());
        let f_line = sdp.media_format.as_ref().unwrap();
        assert!(f_line.starts_with("v/3/"), "录像下载 f= 行应以 v/3/ 开头");
    }

    #[test]
    fn test_build_download_invite_sdp_no_speed() {
        let sdp = build_download_invite_sdp(
            "01234567890000000001",
            "192.168.1.100",
            5000,
            &MediaParam {
                video_encoding: VideoEncoding::PS,
                audio_encoding: AudioEncoding::G711A,
                stream_type: StreamType::Download,
            },
            "2024-01-01T00:00:00",
            "2024-01-01T23:59:59",
            None,
        );

        // 不指定下载速度时不应有 downloadspeed 属性
        let media = &sdp.media_descriptions[0];
        let downloadspeed = media.attributes.iter().find(|a| a.name == "downloadspeed");
        assert!(
            downloadspeed.is_none(),
            "不指定下载速度时不应有 a=downloadspeed"
        );
    }

    #[test]
    fn test_build_download_invite_sdp_speed_values() {
        for speed in [1u32, 2, 4] {
            let sdp = build_download_invite_sdp(
                "01234567890000000001",
                "192.168.1.100",
                5000,
                &MediaParam {
                    video_encoding: VideoEncoding::PS,
                    audio_encoding: AudioEncoding::G711A,
                    stream_type: StreamType::Download,
                },
                "2024-01-01T00:00:00",
                "2024-01-01T23:59:59",
                Some(speed),
            );

            let media = &sdp.media_descriptions[0];
            let downloadspeed = media.attributes.iter().find(|a| a.name == "downloadspeed");
            assert!(downloadspeed.is_some());
            assert_eq!(
                downloadspeed.unwrap().value.as_deref(),
                Some(speed.to_string().as_str())
            );
        }
    }

    #[test]
    fn test_build_download_invite_sdp_serialization() {
        let sdp = build_download_invite_sdp(
            "01234567890000000001",
            "192.168.1.100",
            5000,
            &MediaParam {
                video_encoding: VideoEncoding::PS,
                audio_encoding: AudioEncoding::G711A,
                stream_type: StreamType::Download,
            },
            "2024-01-01T00:00:00",
            "2024-01-01T23:59:59",
            Some(4),
        );

        let sdp_str = sdp.to_sdp_string();

        assert!(sdp_str.contains("v=0\r\n"));
        assert!(sdp_str.contains("s=Download\r\n"));
        assert!(sdp_str.contains("c=IN IP4 192.168.1.100\r\n"));
        assert!(sdp_str.contains("m=video 5000 RTP/AVP"));
        assert!(sdp_str.contains("a=rtpmap:96 PS/90000\r\n"));
        assert!(sdp_str.contains("a=rtpmap:8 PCMA/8000\r\n"));
        assert!(sdp_str.contains("a=sendonly\r\n"));
        assert!(sdp_str.contains("a=downloadspeed:4\r\n"));
        assert!(sdp_str.contains("y=01234567890000000001\r\n"));
        // f= 行格式: Download=3, PS=2, G711A=1, 8kHz=8 -> v/3/2///a/1/8///
        assert!(sdp_str.contains("f=v/3/2///a/1/8///\r\n"));

        // 验证 t= 行不是 0 0
        assert!(!sdp_str.contains("t=0 0\r\n"), "录像下载 t= 行不应为 0 0");
    }

    // ========================================================================
    // extract_time_range 测试
    // ========================================================================

    #[test]
    fn test_extract_time_range_playback() {
        let sdp = build_playback_invite_sdp(
            "01234567890000000001",
            "192.168.1.100",
            5000,
            &MediaParam {
                video_encoding: VideoEncoding::PS,
                audio_encoding: AudioEncoding::G711A,
                stream_type: StreamType::History,
            },
            "2024-01-01T00:00:00",
            "2024-01-01T23:59:59",
        );

        let time_range = extract_time_range(&sdp);
        assert!(time_range.is_some());

        let (start, end) = time_range.unwrap();
        assert_eq!(start, "2024-01-01T00:00:00");
        assert_eq!(end, "2024-01-01T23:59:59");
    }

    #[test]
    fn test_extract_time_range_download() {
        let sdp = build_download_invite_sdp(
            "01234567890000000001",
            "192.168.1.100",
            5000,
            &MediaParam {
                video_encoding: VideoEncoding::PS,
                audio_encoding: AudioEncoding::G711A,
                stream_type: StreamType::Download,
            },
            "2024-06-15T08:30:00",
            "2024-06-15T10:00:00",
            Some(2),
        );

        let time_range = extract_time_range(&sdp);
        assert!(time_range.is_some());

        let (start, end) = time_range.unwrap();
        assert_eq!(start, "2024-06-15T08:30:00");
        assert_eq!(end, "2024-06-15T10:00:00");
    }

    #[test]
    fn test_extract_time_range_live_returns_none() {
        // 实时点播使用 t=0 0，不应返回时间范围
        let sdp = build_invite_sdp(
            "01234567890000000001",
            "192.168.1.100",
            5000,
            &MediaParam {
                video_encoding: VideoEncoding::PS,
                audio_encoding: AudioEncoding::G711A,
                stream_type: StreamType::Live,
            },
        );

        let time_range = extract_time_range(&sdp);
        assert!(time_range.is_none(), "实时点播不应返回时间范围");
    }

    // ========================================================================
    // extract_stream_type 测试
    // ========================================================================

    #[test]
    fn test_extract_stream_type_live() {
        let sdp = build_invite_sdp(
            "01234567890000000001",
            "192.168.1.100",
            5000,
            &MediaParam {
                video_encoding: VideoEncoding::PS,
                audio_encoding: AudioEncoding::G711A,
                stream_type: StreamType::Live,
            },
        );

        assert_eq!(extract_stream_type(&sdp), Some(StreamType::Live));
    }

    #[test]
    fn test_extract_stream_type_history() {
        let sdp = build_playback_invite_sdp(
            "01234567890000000001",
            "192.168.1.100",
            5000,
            &MediaParam {
                video_encoding: VideoEncoding::PS,
                audio_encoding: AudioEncoding::G711A,
                stream_type: StreamType::History,
            },
            "2024-01-01T00:00:00",
            "2024-01-01T23:59:59",
        );

        assert_eq!(extract_stream_type(&sdp), Some(StreamType::History));
    }

    #[test]
    fn test_extract_stream_type_download() {
        let sdp = build_download_invite_sdp(
            "01234567890000000001",
            "192.168.1.100",
            5000,
            &MediaParam {
                video_encoding: VideoEncoding::PS,
                audio_encoding: AudioEncoding::G711A,
                stream_type: StreamType::Download,
            },
            "2024-01-01T00:00:00",
            "2024-01-01T23:59:59",
            Some(4),
        );

        assert_eq!(extract_stream_type(&sdp), Some(StreamType::Download));
    }

    #[test]
    fn test_extract_stream_type_from_session_name() {
        // 当 f= 行不存在时，从会话名称判断
        let sdp = SdpBuilder::new(Origin::ipv4("-", "192.168.1.1"), "Playback")
            .time(0, 0)
            .build();

        assert_eq!(extract_stream_type(&sdp), Some(StreamType::History));

        let sdp = SdpBuilder::new(Origin::ipv4("-", "192.168.1.1"), "Download")
            .time(0, 0)
            .build();

        assert_eq!(extract_stream_type(&sdp), Some(StreamType::Download));
    }

    #[test]
    fn test_extract_stream_type_unknown() {
        let sdp = SdpBuilder::new(Origin::ipv4("-", "192.168.1.1"), "UnknownSession")
            .time(0, 0)
            .build();

        assert_eq!(extract_stream_type(&sdp), None);
    }

    // ========================================================================
    // 历史回放/录像下载 SDP 往返测试
    // ========================================================================

    #[test]
    fn test_playback_sdp_roundtrip() {
        let sdp = build_playback_invite_sdp(
            "01234567890000000001",
            "192.168.1.100",
            5000,
            &MediaParam {
                video_encoding: VideoEncoding::PS,
                audio_encoding: AudioEncoding::G711A,
                stream_type: StreamType::History,
            },
            "2024-01-01T00:00:00",
            "2024-01-01T23:59:59",
        );

        let sdp_str = sdp.to_sdp_string();
        let reparsed = crate::parser::SdpParser::parse(&sdp_str).unwrap();

        // 验证基本字段
        assert_eq!(reparsed.session_name, "Playback");
        assert_eq!(reparsed.ssrc.as_deref(), Some("01234567890000000001"));

        // 验证时间范围
        let time_range = extract_time_range(&reparsed);
        assert!(time_range.is_some());
        let (start, end) = time_range.unwrap();
        assert_eq!(start, "2024-01-01T00:00:00");
        assert_eq!(end, "2024-01-01T23:59:59");

        // 验证流类型
        assert_eq!(extract_stream_type(&reparsed), Some(StreamType::History));

        // 验证媒体属性
        let media = &reparsed.media_descriptions[0];
        let sendonly = media.attributes.iter().find(|a| a.name == "sendonly");
        assert!(sendonly.is_some());
    }

    #[test]
    fn test_download_sdp_roundtrip() {
        let sdp = build_download_invite_sdp(
            "01234567890000000001",
            "192.168.1.100",
            5000,
            &MediaParam {
                video_encoding: VideoEncoding::PS,
                audio_encoding: AudioEncoding::G711A,
                stream_type: StreamType::Download,
            },
            "2024-06-15T08:30:00",
            "2024-06-15T10:00:00",
            Some(4),
        );

        let sdp_str = sdp.to_sdp_string();
        let reparsed = crate::parser::SdpParser::parse(&sdp_str).unwrap();

        // 验证基本字段
        assert_eq!(reparsed.session_name, "Download");
        assert_eq!(reparsed.ssrc.as_deref(), Some("01234567890000000001"));

        // 验证时间范围
        let time_range = extract_time_range(&reparsed);
        assert!(time_range.is_some());
        let (start, end) = time_range.unwrap();
        assert_eq!(start, "2024-06-15T08:30:00");
        assert_eq!(end, "2024-06-15T10:00:00");

        // 验证流类型
        assert_eq!(extract_stream_type(&reparsed), Some(StreamType::Download));

        // 验证媒体属性
        let media = &reparsed.media_descriptions[0];
        let sendonly = media.attributes.iter().find(|a| a.name == "sendonly");
        assert!(sendonly.is_some());

        // 验证下载速度
        let downloadspeed = media.attributes.iter().find(|a| a.name == "downloadspeed");
        assert!(downloadspeed.is_some());
        assert_eq!(downloadspeed.unwrap().value.as_deref(), Some("4"));
    }
}
