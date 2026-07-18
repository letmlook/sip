//! 编解码协商
//!
//! 提供音视频编解码信息定义和协商功能，支持从 SDP 中提取编码列表
//! 并进行双向编码协商。
//!
//! # 支持的编码
//!
//! ## 音频编码
//!
//! | 编码 | PT | 时钟率 | 说明 |
//! |------|-----|--------|------|
//! | PCMU | 0 | 8000 | G.711 μ-law |
//! | PCMA | 8 | 8000 | G.711 A-law |
//! | G722 | 9 | 8000 | G.722 |
//! | OPUS | 111 | 48000 | Opus |
//!
//! ## 视频编码
//!
//! | 编码 | PT | 时钟率 | 说明 |
//! |------|-----|--------|------|
//! | H264 | 96 | 90000 | H.264/AVC |
//! | H265 | 97 | 90000 | H.265/HEVC |
//! | VP8 | 98 | 90000 | VP8 |
//! | VP9 | 99 | 90000 | VP9 |
//! | PS | 96 | 90000 | PS (GB28181 默认) |

use siprs_sdp::types::{MediaDescription, SessionDescription};

// ============================================================================
// 编解码信息
// ============================================================================

/// 编解码信息
///
/// 描述一个编解码器的完整信息，包括载荷类型、编码名称、
/// 时钟率和可选的编码参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodecInfo {
    /// 载荷类型 (Payload Type)
    pub payload_type: u8,
    /// 编码名称（如 "PCMU", "H264", "OPUS"）
    pub encoding_name: String,
    /// 时钟频率 (Hz)
    pub clock_rate: u32,
    /// 编码参数（如音频通道数，视频通常为空）
    pub encoding_params: Option<String>,
    /// fmtp 参数（如 profile-level-id 等）
    pub fmtp: Option<String>,
}

impl CodecInfo {
    /// 创建新的编解码信息
    pub fn new(pt: u8, name: impl Into<String>, clock_rate: u32) -> Self {
        Self {
            payload_type: pt,
            encoding_name: name.into(),
            clock_rate,
            encoding_params: None,
            fmtp: None,
        }
    }

    /// 创建带参数的编解码信息
    pub fn with_params(
        pt: u8,
        name: impl Into<String>,
        clock_rate: u32,
        params: impl Into<String>,
    ) -> Self {
        Self {
            payload_type: pt,
            encoding_name: name.into(),
            clock_rate,
            encoding_params: Some(params.into()),
            fmtp: None,
        }
    }

    /// 设置 fmtp 参数
    pub fn with_fmtp(mut self, fmtp: impl Into<String>) -> Self {
        self.fmtp = Some(fmtp.into());
        self
    }

    /// 判断是否为音频编码
    pub fn is_audio(&self) -> bool {
        matches!(
            self.encoding_name.to_uppercase().as_str(),
            "PCMU" | "PCMA" | "G722" | "G7221" | "OPUS" | "AAC" | "G711A" | "G711U"
        )
    }

    /// 判断是否为视频编码
    pub fn is_video(&self) -> bool {
        matches!(
            self.encoding_name.to_uppercase().as_str(),
            "H264" | "H265" | "VP8" | "VP9" | "PS" | "SVAC"
        )
    }

    /// 编码名称匹配（不区分大小写）
    pub fn name_matches(&self, name: &str) -> bool {
        self.encoding_name.eq_ignore_ascii_case(name)
    }
}

// ============================================================================
// 预定义编码
// ============================================================================

/// 预定义音频编码
impl CodecInfo {
    /// G.711 μ-law (PCMU), PT=0
    pub fn pcmu() -> Self {
        Self::new(0, "PCMU", 8000)
    }

    /// G.711 A-law (PCMA), PT=8
    pub fn pcma() -> Self {
        Self::new(8, "PCMA", 8000)
    }

    /// G.722, PT=9
    pub fn g722() -> Self {
        Self::new(9, "G722", 8000)
    }

    /// Opus, PT=111 (动态)
    pub fn opus() -> Self {
        Self::with_params(111, "OPUS", 48000, "2")
    }
}

/// 预定义视频编码
impl CodecInfo {
    /// H.264, PT=96 (动态)
    pub fn h264() -> Self {
        Self::new(96, "H264", 90000)
    }

    /// H.265, PT=97 (动态)
    pub fn h265() -> Self {
        Self::new(97, "H265", 90000)
    }

    /// VP8, PT=98 (动态)
    pub fn vp8() -> Self {
        Self::new(98, "VP8", 90000)
    }

    /// VP9, PT=99 (动态)
    pub fn vp9() -> Self {
        Self::new(99, "VP9", 90000)
    }

    /// PS (GB28181), PT=96 (动态)
    pub fn ps() -> Self {
        Self::new(96, "PS", 90000)
    }
}

/// 获取所有预定义音频编码列表
pub fn default_audio_codecs() -> Vec<CodecInfo> {
    vec![
        CodecInfo::pcmu(),
        CodecInfo::pcma(),
        CodecInfo::g722(),
        CodecInfo::opus(),
    ]
}

/// 获取所有预定义视频编码列表
pub fn default_video_codecs() -> Vec<CodecInfo> {
    vec![
        CodecInfo::h264(),
        CodecInfo::h265(),
        CodecInfo::vp8(),
        CodecInfo::vp9(),
        CodecInfo::ps(),
    ]
}

// ============================================================================
// 从 SDP 提取编码列表
// ============================================================================

/// 从 SDP 会话描述中提取编解码信息列表
///
/// 解析 SDP 中每个媒体描述的 rtpmap 和 fmtp 属性，
/// 生成对应的 `CodecInfo` 列表。
///
/// # 参数
///
/// - `sdp`: 解析后的 SDP 会话描述
///
/// # 返回
///
/// 所有媒体描述中包含的编解码信息列表
pub fn extract_codecs_from_sdp(sdp: &SessionDescription) -> Vec<CodecInfo> {
    let mut codecs = Vec::new();

    for media in &sdp.media_descriptions {
        codecs.extend(extract_codecs_from_media(media));
    }

    codecs
}

/// 从单个媒体描述中提取编解码信息列表
///
/// 解析媒体描述中的 rtpmap 和 fmtp 属性。
pub fn extract_codecs_from_media(media: &MediaDescription) -> Vec<CodecInfo> {
    let mut codecs = Vec::new();

    // 从 rtpmap 属性提取编码信息
    for attr in &media.attributes {
        if attr.name == "rtpmap" {
            if let Some(ref value) = attr.value {
                if let Some(codec) = parse_rtpmap(value) {
                    codecs.push(codec);
                }
            }
        }
    }

    // 如果没有 rtpmap 属性，从格式列表中推断静态 PT
    if codecs.is_empty() {
        for fmt in &media.formats {
            if let Ok(pt) = fmt.parse::<u8>() {
                if let Some(codec) = static_codec_from_pt(pt) {
                    codecs.push(codec);
                }
            }
        }
    }

    // 补充 fmtp 参数
    for attr in &media.attributes {
        if attr.name == "fmtp" {
            if let Some(ref value) = attr.value {
                apply_fmtp(&mut codecs, value);
            }
        }
    }

    codecs
}

/// 解析 rtpmap 属性值
///
/// 格式: `<payload type> <encoding name>/<clock rate>[/<encoding parameters>]`
///
/// 示例:
/// - `96 PS/90000`
/// - `8 PCMA/8000`
/// - `111 OPUS/48000/2`
fn parse_rtpmap(value: &str) -> Option<CodecInfo> {
    let parts: Vec<&str> = value.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return None;
    }

    let pt: u8 = parts[0].trim().parse().ok()?;
    let encoding_spec = parts[1].trim();

    let spec_parts: Vec<&str> = encoding_spec.splitn(2, '/').collect();
    if spec_parts.len() < 2 {
        return None;
    }

    let encoding_name = spec_parts[0].trim().to_string();
    let clock_rate_and_params = spec_parts[1].trim();

    let rate_parts: Vec<&str> = clock_rate_and_params.splitn(2, '/').collect();
    let clock_rate: u32 = rate_parts[0].trim().parse().ok()?;
    let encoding_params = rate_parts.get(1).map(|s| s.trim().to_string());

    Some(CodecInfo {
        payload_type: pt,
        encoding_name,
        clock_rate,
        encoding_params,
        fmtp: None,
    })
}

/// 从静态载荷类型推断编解码信息
///
/// RFC 3551 定义的静态载荷类型映射。
fn static_codec_from_pt(pt: u8) -> Option<CodecInfo> {
    match pt {
        0 => Some(CodecInfo::pcmu()),
        8 => Some(CodecInfo::pcma()),
        9 => Some(CodecInfo::g722()),
        _ => None,
    }
}

/// 应用 fmtp 参数到编解码列表
///
/// 格式: `<payload type> <format specific parameters>`
fn apply_fmtp(codecs: &mut [CodecInfo], value: &str) {
    let parts: Vec<&str> = value.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return;
    }

    if let Ok(pt) = parts[0].trim().parse::<u8>() {
        let fmtp_value = parts[1].trim().to_string();
        for codec in codecs.iter_mut() {
            if codec.payload_type == pt {
                codec.fmtp = Some(fmtp_value);
                break;
            }
        }
    }
}

// ============================================================================
// 编解码协商器
// ============================================================================

/// 编解码协商器
///
/// 负责在本地支持的编码列表和远端提供的编码列表之间
/// 找到共同支持的编码。
#[derive(Debug, Clone)]
pub struct CodecNegotiator {
    /// 本地支持的音频编码列表（按优先级排序）
    pub local_audio_codecs: Vec<CodecInfo>,
    /// 本地支持的视频编码列表（按优先级排序）
    pub local_video_codecs: Vec<CodecInfo>,
}

impl CodecNegotiator {
    /// 创建新的编解码协商器
    pub fn new() -> Self {
        Self {
            local_audio_codecs: default_audio_codecs(),
            local_video_codecs: default_video_codecs(),
        }
    }

    /// 使用自定义编码列表创建协商器
    pub fn with_codecs(audio: Vec<CodecInfo>, video: Vec<CodecInfo>) -> Self {
        Self {
            local_audio_codecs: audio,
            local_video_codecs: video,
        }
    }

    /// 与远端编码列表进行协商
    ///
    /// 按本地编码优先级顺序，在远端列表中查找匹配的编码。
    /// 匹配规则：编码名称相同且时钟频率相同。
    ///
    /// # 参数
    ///
    /// - `remote_codecs`: 远端支持的编码列表
    ///
    /// # 返回
    ///
    /// 协商成功的编码列表（按本地优先级排序）
    pub fn negotiate(&self, remote_codecs: &[CodecInfo]) -> Vec<CodecInfo> {
        let mut result = Vec::new();

        // 音频协商
        for local in &self.local_audio_codecs {
            if let Some(remote) = find_matching_codec(local, remote_codecs) {
                // 使用远端的 PT，本地编码名称和参数
                result.push(CodecInfo {
                    payload_type: remote.payload_type,
                    encoding_name: local.encoding_name.clone(),
                    clock_rate: local.clock_rate,
                    encoding_params: local.encoding_params.clone(),
                    fmtp: remote.fmtp.clone().or_else(|| local.fmtp.clone()),
                });
            }
        }

        // 视频协商
        for local in &self.local_video_codecs {
            if let Some(remote) = find_matching_codec(local, remote_codecs) {
                result.push(CodecInfo {
                    payload_type: remote.payload_type,
                    encoding_name: local.encoding_name.clone(),
                    clock_rate: local.clock_rate,
                    encoding_params: local.encoding_params.clone(),
                    fmtp: remote.fmtp.clone().or_else(|| local.fmtp.clone()),
                });
            }
        }

        result
    }

    /// 与远端 SDP 进行协商
    ///
    /// 从远端 SDP 中提取编码列表，然后进行协商。
    ///
    /// # 参数
    ///
    /// - `remote_sdp`: 远端的 SDP 会话描述
    ///
    /// # 返回
    ///
    /// 协商成功的编码列表
    pub fn negotiate_with_sdp(&self, remote_sdp: &SessionDescription) -> Vec<CodecInfo> {
        let remote_codecs = extract_codecs_from_sdp(remote_sdp);
        self.negotiate(&remote_codecs)
    }

    /// 检查是否支持指定编码
    pub fn supports_codec(&self, name: &str) -> bool {
        let name_upper = name.to_uppercase();
        self.local_audio_codecs
            .iter()
            .chain(self.local_video_codecs.iter())
            .any(|c| c.encoding_name.to_uppercase() == name_upper)
    }
}

impl Default for CodecNegotiator {
    fn default() -> Self {
        Self::new()
    }
}

/// 在编码列表中查找匹配的编码
///
/// 匹配规则：编码名称相同（不区分大小写）且时钟频率相同。
fn find_matching_codec<'a>(target: &CodecInfo, codecs: &'a [CodecInfo]) -> Option<&'a CodecInfo> {
    codecs.iter().find(|c| {
        c.encoding_name.eq_ignore_ascii_case(&target.encoding_name)
            && c.clock_rate == target.clock_rate
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use siprs_sdp::builder::SdpBuilder;
    use siprs_sdp::types::*;

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
    fn test_codec_info_creation() {
        let codec = CodecInfo::new(96, "H264", 90000);
        assert_eq!(codec.payload_type, 96);
        assert_eq!(codec.encoding_name, "H264");
        assert_eq!(codec.clock_rate, 90000);
        assert!(codec.encoding_params.is_none());
        assert!(codec.fmtp.is_none());
    }

    #[test]
    fn test_codec_info_with_params() {
        let codec = CodecInfo::with_params(111, "OPUS", 48000, "2");
        assert_eq!(codec.encoding_params.as_deref(), Some("2"));
    }

    #[test]
    fn test_codec_info_with_fmtp() {
        let codec = CodecInfo::new(96, "H264", 90000).with_fmtp("profile-level-id=42e01f");
        assert_eq!(codec.fmtp.as_deref(), Some("profile-level-id=42e01f"));
    }

    #[test]
    fn test_predefined_codecs() {
        let pcmu = CodecInfo::pcmu();
        assert_eq!(pcmu.payload_type, 0);
        assert_eq!(pcmu.encoding_name, "PCMU");
        assert_eq!(pcmu.clock_rate, 8000);
        assert!(pcmu.is_audio());
        assert!(!pcmu.is_video());

        let pcma = CodecInfo::pcma();
        assert_eq!(pcma.payload_type, 8);
        assert!(pcma.is_audio());

        let h264 = CodecInfo::h264();
        assert_eq!(h264.payload_type, 96);
        assert!(h264.is_video());
        assert!(!h264.is_audio());

        let ps = CodecInfo::ps();
        assert!(ps.is_video());
    }

    #[test]
    fn test_name_matches() {
        let codec = CodecInfo::new(96, "H264", 90000);
        assert!(codec.name_matches("H264"));
        assert!(codec.name_matches("h264"));
        assert!(codec.name_matches("H264"));
        assert!(!codec.name_matches("H265"));
    }

    #[test]
    fn test_parse_rtpmap() {
        // 标准格式
        let codec = parse_rtpmap("96 PS/90000").unwrap();
        assert_eq!(codec.payload_type, 96);
        assert_eq!(codec.encoding_name, "PS");
        assert_eq!(codec.clock_rate, 90000);

        // 带编码参数
        let codec = parse_rtpmap("111 OPUS/48000/2").unwrap();
        assert_eq!(codec.payload_type, 111);
        assert_eq!(codec.encoding_name, "OPUS");
        assert_eq!(codec.clock_rate, 48000);
        assert_eq!(codec.encoding_params.as_deref(), Some("2"));

        // 静态编码
        let codec = parse_rtpmap("8 PCMA/8000").unwrap();
        assert_eq!(codec.payload_type, 8);
        assert_eq!(codec.encoding_name, "PCMA");
        assert_eq!(codec.clock_rate, 8000);
    }

    #[test]
    fn test_parse_rtpmap_invalid() {
        assert!(parse_rtpmap("").is_none());
        assert!(parse_rtpmap("96").is_none());
        assert!(parse_rtpmap("invalid").is_none());
    }

    #[test]
    fn test_extract_codecs_from_sdp() {
        let media = MediaDescription::new(MediaType::Video, 5000, "RTP/AVP")
            .with_format("96")
            .with_format("8")
            .with_rtpmap(96, "PS/90000")
            .with_rtpmap(8, "PCMA/8000");

        let sdp = SdpBuilder::new(test_origin(), "Test")
            .time(0, 0)
            .media(media)
            .build();

        let codecs = extract_codecs_from_sdp(&sdp);
        assert_eq!(codecs.len(), 2);

        // 检查 PS 编码
        assert!(codecs
            .iter()
            .any(|c| c.encoding_name == "PS" && c.payload_type == 96));
        // 检查 PCMA 编码
        assert!(codecs
            .iter()
            .any(|c| c.encoding_name == "PCMA" && c.payload_type == 8));
    }

    #[test]
    fn test_extract_codecs_static_pt() {
        // 没有 rtpmap，只有静态 PT
        let media = MediaDescription::new(MediaType::Audio, 8000, "RTP/AVP")
            .with_format("0")
            .with_format("8");

        let sdp = SdpBuilder::new(test_origin(), "Test")
            .time(0, 0)
            .media(media)
            .build();

        let codecs = extract_codecs_from_sdp(&sdp);
        assert_eq!(codecs.len(), 2);
        assert!(codecs
            .iter()
            .any(|c| c.payload_type == 0 && c.encoding_name == "PCMU"));
        assert!(codecs
            .iter()
            .any(|c| c.payload_type == 8 && c.encoding_name == "PCMA"));
    }

    #[test]
    fn test_negotiator_basic() {
        let negotiator = CodecNegotiator::new();

        let remote_codecs = vec![
            CodecInfo::new(96, "PS", 90000),
            CodecInfo::new(8, "PCMA", 8000),
        ];

        let result = negotiator.negotiate(&remote_codecs);
        assert!(!result.is_empty());

        // 应该协商出 PCMA 和 PS
        assert!(result.iter().any(|c| c.name_matches("PCMA")));
        assert!(result.iter().any(|c| c.name_matches("PS")));
    }

    #[test]
    fn test_negotiator_no_common() {
        let local_audio = vec![CodecInfo::opus()];
        let local_video = vec![CodecInfo::vp9()];
        let negotiator = CodecNegotiator::with_codecs(local_audio, local_video);

        let remote_codecs = vec![
            CodecInfo::new(96, "PS", 90000),
            CodecInfo::new(8, "PCMA", 8000),
        ];

        let result = negotiator.negotiate(&remote_codecs);
        assert!(result.is_empty());
    }

    #[test]
    fn test_negotiator_with_sdp() {
        let negotiator = CodecNegotiator::new();

        let media = MediaDescription::new(MediaType::Video, 5000, "RTP/AVP")
            .with_format("96")
            .with_format("8")
            .with_rtpmap(96, "PS/90000")
            .with_rtpmap(8, "PCMA/8000");

        let sdp = SdpBuilder::new(test_origin(), "Test")
            .time(0, 0)
            .media(media)
            .build();

        let result = negotiator.negotiate_with_sdp(&sdp);
        assert!(!result.is_empty());
        assert!(result.iter().any(|c| c.name_matches("PS")));
        assert!(result.iter().any(|c| c.name_matches("PCMA")));
    }

    #[test]
    fn test_negotiator_uses_remote_pt() {
        let negotiator = CodecNegotiator::new();

        // 远端使用非标准 PT
        let remote_codecs = vec![CodecInfo::new(100, "PCMA", 8000)];

        let result = negotiator.negotiate(&remote_codecs);
        assert!(!result.is_empty());

        // 协商结果应使用远端的 PT
        assert_eq!(result[0].payload_type, 100);
        assert_eq!(result[0].encoding_name, "PCMA");
    }

    #[test]
    fn test_negotiator_supports_codec() {
        let negotiator = CodecNegotiator::new();
        assert!(negotiator.supports_codec("PCMA"));
        assert!(negotiator.supports_codec("pcma"));
        assert!(negotiator.supports_codec("H264"));
        assert!(negotiator.supports_codec("PS"));
        assert!(!negotiator.supports_codec("AV1"));
    }

    #[test]
    fn test_apply_fmtp() {
        let mut codecs = vec![CodecInfo::new(96, "H264", 90000)];
        apply_fmtp(
            &mut codecs,
            "96 profile-level-id=42e01f;packetization-mode=1",
        );

        assert_eq!(
            codecs[0].fmtp.as_deref(),
            Some("profile-level-id=42e01f;packetization-mode=1")
        );
    }

    #[test]
    fn test_default_codecs() {
        let audio = default_audio_codecs();
        assert!(audio.len() >= 4);
        assert!(audio.iter().any(|c| c.name_matches("PCMU")));
        assert!(audio.iter().any(|c| c.name_matches("PCMA")));
        assert!(audio.iter().any(|c| c.name_matches("G722")));
        assert!(audio.iter().any(|c| c.name_matches("OPUS")));

        let video = default_video_codecs();
        assert!(video.len() >= 5);
        assert!(video.iter().any(|c| c.name_matches("H264")));
        assert!(video.iter().any(|c| c.name_matches("H265")));
        assert!(video.iter().any(|c| c.name_matches("VP8")));
        assert!(video.iter().any(|c| c.name_matches("VP9")));
        assert!(video.iter().any(|c| c.name_matches("PS")));
    }
}
