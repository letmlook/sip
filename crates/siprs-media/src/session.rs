//! 媒体会话管理
//!
//! 提供媒体会话的创建、修改、终止等管理功能，
//! 关联 SDP 协商结果与 RTP 端点信息。
//!
//! # GB28181 场景
//!
//! 在 GB28181 场景下，媒体流通常由流媒体服务器（如 ZLMediaKit、MediaMTX）处理，
//! SIP 信令服务器只需完成 SDP 协商并告知双方媒体地址。
//!
//! 典型流程：
//! 1. SIP 服务器收到 INVITE，解析 SDP 获取媒体信息
//! 2. 创建 `MediaSession`，关联设备与流媒体服务器
//! 3. 将流媒体服务器的媒体地址通过 200 OK SDP 告知设备
//! 4. 设备开始向流媒体服务器发送 RTP 媒体流
//! 5. SIP BYE 终止会话

use siprs_sdp::types::SessionDescription;

use crate::codec::{self, CodecInfo};
use crate::MediaError;

// ============================================================================
// 媒体方向
// ============================================================================

/// 媒体方向
///
/// 描述媒体流的发送/接收方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaDirection {
    /// 仅发送
    SendOnly,
    /// 仅接收
    RecvOnly,
    /// 发送和接收
    SendRecv,
    /// 既不发送也不接收（保持）
    Inactive,
}

impl MediaDirection {
    /// 从 SDP 属性名解析
    pub fn from_attribute(name: &str) -> Option<Self> {
        match name {
            "sendonly" => Some(MediaDirection::SendOnly),
            "recvonly" => Some(MediaDirection::RecvOnly),
            "sendrecv" => Some(MediaDirection::SendRecv),
            "inactive" => Some(MediaDirection::Inactive),
            _ => None,
        }
    }

    /// 转换为 SDP 属性名
    pub fn to_attribute(self) -> &'static str {
        match self {
            MediaDirection::SendOnly => "sendonly",
            MediaDirection::RecvOnly => "recvonly",
            MediaDirection::SendRecv => "sendrecv",
            MediaDirection::Inactive => "inactive",
        }
    }
}

// ============================================================================
// 媒体会话配置
// ============================================================================

/// 媒体会话配置
///
/// 描述媒体会话的参数，包括编码、码率、分辨率等。
#[derive(Debug, Clone)]
pub struct MediaSessionConfig {
    /// 音频编码列表
    pub audio_codecs: Vec<CodecInfo>,
    /// 视频编码列表
    pub video_codecs: Vec<CodecInfo>,
    /// 媒体方向
    pub direction: MediaDirection,
    /// 媒体端口（RTP）
    pub rtp_port: u16,
    /// RTCP 端口（通常为 RTP 端口 + 1）
    pub rtcp_port: u16,
    /// 媒体地址
    pub media_address: String,
}

impl MediaSessionConfig {
    /// 创建新的媒体会话配置
    pub fn new(address: impl Into<String>, rtp_port: u16) -> Self {
        Self {
            audio_codecs: codec::default_audio_codecs(),
            video_codecs: codec::default_video_codecs(),
            direction: MediaDirection::SendRecv,
            rtp_port,
            rtcp_port: rtp_port + 1,
            media_address: address.into(),
        }
    }

    /// 创建 GB28181 接收配置（服务器接收设备推流）
    pub fn gb28181_recv(address: impl Into<String>, rtp_port: u16) -> Self {
        Self {
            audio_codecs: codec::default_audio_codecs(),
            video_codecs: vec![CodecInfo::ps(), CodecInfo::h264(), CodecInfo::h265()],
            direction: MediaDirection::RecvOnly,
            rtp_port,
            rtcp_port: rtp_port + 1,
            media_address: address.into(),
        }
    }

    /// 创建 GB28181 发送配置（设备发送推流）
    pub fn gb28181_send(address: impl Into<String>, rtp_port: u16) -> Self {
        Self {
            audio_codecs: codec::default_audio_codecs(),
            video_codecs: vec![CodecInfo::ps(), CodecInfo::h264(), CodecInfo::h265()],
            direction: MediaDirection::SendOnly,
            rtp_port,
            rtcp_port: rtp_port + 1,
            media_address: address.into(),
        }
    }

    /// 设置媒体方向
    pub fn with_direction(mut self, direction: MediaDirection) -> Self {
        self.direction = direction;
        self
    }

    /// 设置音频编码列表
    pub fn with_audio_codecs(mut self, codecs: Vec<CodecInfo>) -> Self {
        self.audio_codecs = codecs;
        self
    }

    /// 设置视频编码列表
    pub fn with_video_codecs(mut self, codecs: Vec<CodecInfo>) -> Self {
        self.video_codecs = codecs;
        self
    }
}

// ============================================================================
// 媒体端点信息
// ============================================================================

/// 媒体端点信息
///
/// 描述一个 RTP 媒体端点的地址和端口信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaEndpoint {
    /// IP 地址
    pub address: String,
    /// RTP 端口
    pub rtp_port: u16,
    /// RTCP 端口
    pub rtcp_port: u16,
    /// SSRC
    pub ssrc: Option<u32>,
}

impl MediaEndpoint {
    /// 创建新的媒体端点
    pub fn new(address: impl Into<String>, rtp_port: u16) -> Self {
        Self {
            address: address.into(),
            rtp_port,
            rtcp_port: rtp_port + 1,
            ssrc: None,
        }
    }

    /// 设置 SSRC
    pub fn with_ssrc(mut self, ssrc: u32) -> Self {
        self.ssrc = Some(ssrc);
        self
    }

    /// 从 SDP 提取媒体端点信息
    ///
    /// 从 SDP 的连接地址和媒体描述的端口信息构建端点。
    pub fn from_sdp(sdp: &SessionDescription) -> Option<Self> {
        // 获取连接地址
        let address = sdp
            .connection
            .as_ref()
            .map(|c| c.connection_address.clone())
            .or_else(|| {
                sdp.media_descriptions
                    .first()
                    .and_then(|m| m.connection.as_ref())
                    .map(|c| c.connection_address.clone())
            })?;

        // 获取端口
        let rtp_port = sdp.media_descriptions.first().map(|m| m.port as u16)?;

        // 获取 SSRC (GB28181 y= 行)
        let ssrc = sdp.ssrc.as_ref().and_then(|s| {
            // GB28181 SSRC 通常是 20 位数字，取低 32 位
            let ssrc_val: u64 = s.parse().ok()?;
            Some(ssrc_val as u32)
        });

        Some(Self {
            address,
            rtp_port,
            rtcp_port: rtp_port + 1,
            ssrc,
        })
    }
}

// ============================================================================
// 媒体会话状态
// ============================================================================

/// 媒体会话状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaSessionState {
    /// 初始状态
    Initial,
    /// SDP 协商中
    Negotiating,
    /// 活跃（媒体流传输中）
    Active,
    /// 暂停
    Paused,
    /// 已终止
    Terminated,
}

// ============================================================================
// 媒体会话
// ============================================================================

/// 媒体会话
///
/// 关联 SDP 协商结果与 RTP 端点信息，管理媒体会话的完整生命周期。
///
/// 在 GB28181 场景中，一个媒体会话通常对应一次 INVITE 对话，
/// 设备向流媒体服务器推送 RTP 媒体流。
#[derive(Debug, Clone)]
pub struct MediaSession {
    /// 会话 ID
    pub session_id: String,
    /// 会话状态
    pub state: MediaSessionState,
    /// 本地媒体端点
    pub local_endpoint: Option<MediaEndpoint>,
    /// 远端媒体端点
    pub remote_endpoint: Option<MediaEndpoint>,
    /// 本地支持的音频编码列表
    pub local_audio_codecs: Vec<CodecInfo>,
    /// 本地支持的视频编码列表
    pub local_video_codecs: Vec<CodecInfo>,
    /// 协商后的编解码列表
    pub negotiated_codecs: Vec<CodecInfo>,
    /// 媒体方向
    pub direction: MediaDirection,
    /// 本地 SDP
    pub local_sdp: Option<SessionDescription>,
    /// 远端 SDP
    pub remote_sdp: Option<SessionDescription>,
}

impl MediaSession {
    /// 创建新的媒体会话
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            state: MediaSessionState::Initial,
            local_endpoint: None,
            remote_endpoint: None,
            local_audio_codecs: codec::default_audio_codecs(),
            local_video_codecs: codec::default_video_codecs(),
            negotiated_codecs: Vec::new(),
            direction: MediaDirection::SendRecv,
            local_sdp: None,
            remote_sdp: None,
        }
    }

    /// 使用配置创建媒体会话
    pub fn with_config(session_id: impl Into<String>, config: &MediaSessionConfig) -> Self {
        Self {
            session_id: session_id.into(),
            state: MediaSessionState::Initial,
            local_endpoint: Some(MediaEndpoint::new(&config.media_address, config.rtp_port)),
            remote_endpoint: None,
            local_audio_codecs: config.audio_codecs.clone(),
            local_video_codecs: config.video_codecs.clone(),
            negotiated_codecs: Vec::new(),
            direction: config.direction,
            local_sdp: None,
            remote_sdp: None,
        }
    }

    /// 设置远端 SDP 并提取媒体信息
    ///
    /// 解析远端 SDP，提取端点信息和编解码列表，
    /// 并进行编解码协商。
    pub fn set_remote_sdp(&mut self, sdp: SessionDescription) -> Result<(), MediaError> {
        // 提取远端端点信息
        self.remote_endpoint = MediaEndpoint::from_sdp(&sdp);

        // 提取远端编解码列表
        let remote_codecs = codec::extract_codecs_from_sdp(&sdp);

        // 提取媒体方向
        for media in &sdp.media_descriptions {
            for attr in &media.attributes {
                if let Some(dir) = MediaDirection::from_attribute(&attr.name) {
                    self.direction = dir;
                    break;
                }
            }
        }

        // 进行编解码协商
        let negotiator = codec::CodecNegotiator::with_codecs(
            self.local_audio_codecs.clone(),
            self.local_video_codecs.clone(),
        );
        self.negotiated_codecs = negotiator.negotiate(&remote_codecs);

        if self.negotiated_codecs.is_empty() {
            return Err(MediaError::CodecNegotiationFailed(
                "no common codecs found".to_string(),
            ));
        }

        self.remote_sdp = Some(sdp);
        self.state = MediaSessionState::Negotiating;

        Ok(())
    }

    /// 设置本地 SDP
    pub fn set_local_sdp(&mut self, sdp: SessionDescription) {
        self.local_endpoint = MediaEndpoint::from_sdp(&sdp);
        self.local_sdp = Some(sdp);
    }

    /// 激活会话
    ///
    /// SDP 协商完成后，将会话状态设置为活跃。
    pub fn activate(&mut self) -> Result<(), MediaError> {
        if self.state == MediaSessionState::Terminated {
            return Err(MediaError::SessionError(
                "cannot activate a terminated session".to_string(),
            ));
        }
        self.state = MediaSessionState::Active;
        Ok(())
    }

    /// 暂停会话
    pub fn pause(&mut self) -> Result<(), MediaError> {
        if self.state != MediaSessionState::Active {
            return Err(MediaError::SessionError(
                "can only pause an active session".to_string(),
            ));
        }
        self.state = MediaSessionState::Paused;
        Ok(())
    }

    /// 恢复会话
    pub fn resume(&mut self) -> Result<(), MediaError> {
        if self.state != MediaSessionState::Paused {
            return Err(MediaError::SessionError(
                "can only resume a paused session".to_string(),
            ));
        }
        self.state = MediaSessionState::Active;
        Ok(())
    }

    /// 终止会话
    pub fn terminate(&mut self) {
        self.state = MediaSessionState::Terminated;
    }

    /// 检查会话是否活跃
    pub fn is_active(&self) -> bool {
        self.state == MediaSessionState::Active
    }

    /// 检查会话是否已终止
    pub fn is_terminated(&self) -> bool {
        self.state == MediaSessionState::Terminated
    }

    /// 获取协商后的音频编码列表
    pub fn audio_codecs(&self) -> Vec<&CodecInfo> {
        self.negotiated_codecs
            .iter()
            .filter(|c| c.is_audio())
            .collect()
    }

    /// 获取协商后的视频编码列表
    pub fn video_codecs(&self) -> Vec<&CodecInfo> {
        self.negotiated_codecs
            .iter()
            .filter(|c| c.is_video())
            .collect()
    }

    /// 获取远端媒体地址
    pub fn remote_address(&self) -> Option<&str> {
        self.remote_endpoint.as_ref().map(|e| e.address.as_str())
    }

    /// 获取远端 RTP 端口
    pub fn remote_rtp_port(&self) -> Option<u16> {
        self.remote_endpoint.as_ref().map(|e| e.rtp_port)
    }

    /// 获取远端 SSRC
    pub fn remote_ssrc(&self) -> Option<u32> {
        self.remote_endpoint.as_ref().and_then(|e| e.ssrc)
    }
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
    fn test_media_direction_from_attribute() {
        assert_eq!(
            MediaDirection::from_attribute("sendonly"),
            Some(MediaDirection::SendOnly)
        );
        assert_eq!(
            MediaDirection::from_attribute("recvonly"),
            Some(MediaDirection::RecvOnly)
        );
        assert_eq!(
            MediaDirection::from_attribute("sendrecv"),
            Some(MediaDirection::SendRecv)
        );
        assert_eq!(
            MediaDirection::from_attribute("inactive"),
            Some(MediaDirection::Inactive)
        );
        assert_eq!(MediaDirection::from_attribute("unknown"), None);
    }

    #[test]
    fn test_media_direction_to_attribute() {
        assert_eq!(MediaDirection::SendOnly.to_attribute(), "sendonly");
        assert_eq!(MediaDirection::RecvOnly.to_attribute(), "recvonly");
        assert_eq!(MediaDirection::SendRecv.to_attribute(), "sendrecv");
        assert_eq!(MediaDirection::Inactive.to_attribute(), "inactive");
    }

    #[test]
    fn test_media_session_config_new() {
        let config = MediaSessionConfig::new("192.168.1.100", 5000);
        assert_eq!(config.media_address, "192.168.1.100");
        assert_eq!(config.rtp_port, 5000);
        assert_eq!(config.rtcp_port, 5001);
        assert_eq!(config.direction, MediaDirection::SendRecv);
        assert!(!config.audio_codecs.is_empty());
        assert!(!config.video_codecs.is_empty());
    }

    #[test]
    fn test_media_session_config_gb28181_recv() {
        let config = MediaSessionConfig::gb28181_recv("192.168.1.100", 5000);
        assert_eq!(config.direction, MediaDirection::RecvOnly);
        assert!(config.video_codecs.iter().any(|c| c.name_matches("PS")));
    }

    #[test]
    fn test_media_session_config_gb28181_send() {
        let config = MediaSessionConfig::gb28181_send("192.168.1.200", 6000);
        assert_eq!(config.direction, MediaDirection::SendOnly);
    }

    #[test]
    fn test_media_endpoint_new() {
        let endpoint = MediaEndpoint::new("192.168.1.100", 5000);
        assert_eq!(endpoint.address, "192.168.1.100");
        assert_eq!(endpoint.rtp_port, 5000);
        assert_eq!(endpoint.rtcp_port, 5001);
        assert!(endpoint.ssrc.is_none());
    }

    #[test]
    fn test_media_endpoint_with_ssrc() {
        let endpoint = MediaEndpoint::new("192.168.1.100", 5000).with_ssrc(0x12345678);
        assert_eq!(endpoint.ssrc, Some(0x12345678));
    }

    #[test]
    fn test_media_endpoint_from_sdp() {
        let media = MediaDescription::new(MediaType::Video, 5000, "RTP/AVP")
            .with_format("96")
            .with_rtpmap(96, "PS/90000");

        let sdp = SdpBuilder::new(test_origin(), "Test")
            .connection(Connection::ipv4("192.168.1.100"))
            .time(0, 0)
            .media(media)
            .ssrc("01234567890000000001")
            .build();

        let endpoint = MediaEndpoint::from_sdp(&sdp).unwrap();
        assert_eq!(endpoint.address, "192.168.1.100");
        assert_eq!(endpoint.rtp_port, 5000);
        assert_eq!(endpoint.rtcp_port, 5001);
        assert!(endpoint.ssrc.is_some());
    }

    #[test]
    fn test_media_session_new() {
        let session = MediaSession::new("test-session-1");
        assert_eq!(session.session_id, "test-session-1");
        assert_eq!(session.state, MediaSessionState::Initial);
        assert!(session.local_endpoint.is_none());
        assert!(session.remote_endpoint.is_none());
        assert!(session.negotiated_codecs.is_empty());
    }

    #[test]
    fn test_media_session_with_config() {
        let config = MediaSessionConfig::gb28181_recv("192.168.1.100", 5000);
        let session = MediaSession::with_config("test-session-1", &config);
        assert_eq!(session.session_id, "test-session-1");
        assert!(session.local_endpoint.is_some());
        assert_eq!(session.direction, MediaDirection::RecvOnly);
    }

    #[test]
    fn test_media_session_lifecycle() {
        let config = MediaSessionConfig::gb28181_recv("192.168.1.100", 5000);
        let mut session = MediaSession::with_config("test-session-1", &config);

        // 初始状态
        assert_eq!(session.state, MediaSessionState::Initial);
        assert!(!session.is_active());
        assert!(!session.is_terminated());

        // 设置远端 SDP
        let media = MediaDescription::new(MediaType::Video, 6000, "RTP/AVP")
            .with_format("96")
            .with_format("8")
            .with_rtpmap(96, "PS/90000")
            .with_rtpmap(8, "PCMA/8000")
            .with_attribute("sendonly", None);

        let remote_sdp = SdpBuilder::new(test_origin(), "Play")
            .connection(Connection::ipv4("192.168.1.200"))
            .time(0, 0)
            .media(media)
            .ssrc("01234567890000000001")
            .build();

        session.set_remote_sdp(remote_sdp).unwrap();
        assert_eq!(session.state, MediaSessionState::Negotiating);
        assert!(!session.negotiated_codecs.is_empty());
        assert_eq!(session.direction, MediaDirection::SendOnly);

        // 激活会话
        session.activate().unwrap();
        assert_eq!(session.state, MediaSessionState::Active);
        assert!(session.is_active());

        // 暂停会话
        session.pause().unwrap();
        assert_eq!(session.state, MediaSessionState::Paused);

        // 恢复会话
        session.resume().unwrap();
        assert_eq!(session.state, MediaSessionState::Active);

        // 终止会话
        session.terminate();
        assert_eq!(session.state, MediaSessionState::Terminated);
        assert!(session.is_terminated());
    }

    #[test]
    fn test_media_session_terminate_cannot_activate() {
        let mut session = MediaSession::new("test-session-1");
        session.terminate();
        let result = session.activate();
        assert!(result.is_err());
    }

    #[test]
    fn test_media_session_pause_not_active() {
        let mut session = MediaSession::new("test-session-1");
        let result = session.pause();
        assert!(result.is_err());
    }

    #[test]
    fn test_media_session_resume_not_paused() {
        let mut session = MediaSession::new("test-session-1");
        let result = session.resume();
        assert!(result.is_err());
    }

    #[test]
    fn test_media_session_codec_access() {
        let config = MediaSessionConfig::gb28181_recv("192.168.1.100", 5000);
        let mut session = MediaSession::with_config("test-session-1", &config);

        let media = MediaDescription::new(MediaType::Video, 6000, "RTP/AVP")
            .with_format("96")
            .with_format("8")
            .with_rtpmap(96, "PS/90000")
            .with_rtpmap(8, "PCMA/8000");

        let remote_sdp = SdpBuilder::new(test_origin(), "Play")
            .connection(Connection::ipv4("192.168.1.200"))
            .time(0, 0)
            .media(media)
            .build();

        session.set_remote_sdp(remote_sdp).unwrap();

        let audio = session.audio_codecs();
        let video = session.video_codecs();
        assert!(!audio.is_empty());
        assert!(!video.is_empty());
    }

    #[test]
    fn test_media_session_remote_info() {
        let config = MediaSessionConfig::gb28181_recv("192.168.1.100", 5000);
        let mut session = MediaSession::with_config("test-session-1", &config);

        let media = MediaDescription::new(MediaType::Video, 6000, "RTP/AVP")
            .with_format("96")
            .with_rtpmap(96, "PS/90000");

        let remote_sdp = SdpBuilder::new(test_origin(), "Play")
            .connection(Connection::ipv4("192.168.1.200"))
            .time(0, 0)
            .media(media)
            .ssrc("01234567890000000001")
            .build();

        session.set_remote_sdp(remote_sdp).unwrap();

        assert_eq!(session.remote_address(), Some("192.168.1.200"));
        assert_eq!(session.remote_rtp_port(), Some(6000));
        assert!(session.remote_ssrc().is_some());
    }

    #[test]
    fn test_media_session_no_common_codecs() {
        let config = MediaSessionConfig::new("192.168.1.100", 5000)
            .with_audio_codecs(vec![CodecInfo::opus()])
            .with_video_codecs(vec![CodecInfo::vp9()]);

        let mut session = MediaSession::with_config("test-session-1", &config);

        let media = MediaDescription::new(MediaType::Video, 6000, "RTP/AVP")
            .with_format("96")
            .with_rtpmap(96, "PS/90000");

        let remote_sdp = SdpBuilder::new(test_origin(), "Play")
            .connection(Connection::ipv4("192.168.1.200"))
            .time(0, 0)
            .media(media)
            .build();

        let result = session.set_remote_sdp(remote_sdp);
        assert!(result.is_err());
    }
}
