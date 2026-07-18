//! # siprs-media
//!
//! 媒体协商、RTP/RTCP 包处理和编解码协商库。
//!
//! 本 crate 提供 SIP 媒体层的核心功能：
//!
//! - **RTP 包解析/构建** — 基于 RFC 3550，支持 RFC 5285 头部扩展
//! - **RTCP 包解析/构建** — 支持 SR/RR/SDES/BYE/APP 五种包类型
//! - **编解码协商** — 从 SDP 提取编码列表，双向编码协商
//! - **媒体会话管理** — 会话创建、修改、终止，关联 SDP 和 RTP 端点
//!
//! # GB28181 场景
//!
//! 在 GB28181 国标场景下，媒体流通常由流媒体服务器（如 ZLMediaKit、MediaMTX）处理，
//! SIP 信令服务器只需完成 SDP 协商并告知双方媒体地址。本 crate 的 RTP/RTCP 模块
//! 只做包的解析和构建，不做实际的网络传输。
//!
//! # 模块结构
//!
//! - [`rtp`] — RTP 包解析与构建
//! - [`rtcp`] — RTCP 包解析与构建
//! - [`codec`] — 编解码协商
//! - [`session`] — 媒体会话管理
//!
//! # 快速开始
//!
//! ## RTP 包解析
//!
//! ```
//! use siprs_media::rtp::RtpPacket;
//!
//! let data = [0x80, 0x60, 0x00, 0x01, 0x00, 0x00, 0x00, 0xA0,
//!             0x12, 0x34, 0x56, 0x78, 0xAA, 0xBB, 0xCC, 0xDD];
//! let packet = RtpPacket::parse(&data).unwrap();
//! assert_eq!(packet.payload_type(), 96);
//! ```
//!
//! ## 编解码协商
//!
//! ```
//! use siprs_media::codec::CodecNegotiator;
//! use siprs_media::codec::CodecInfo;
//!
//! let negotiator = CodecNegotiator::new();
//! let remote_codecs = vec![CodecInfo::pcma(), CodecInfo::ps()];
//! let result = negotiator.negotiate(&remote_codecs);
//! assert!(!result.is_empty());
//! ```
//!
//! ## 媒体会话
//!
//! ```
//! use siprs_media::session::{MediaSession, MediaSessionConfig};
//!
//! let config = MediaSessionConfig::gb28181_recv("192.168.1.100", 5000);
//! let session = MediaSession::with_config("session-1", &config);
//! assert_eq!(session.session_id, "session-1");
//! ```

pub mod codec;
pub mod rtcp;
pub mod rtp;
pub mod session;

// ============================================================================
// 错误类型
// ============================================================================

/// 媒体层错误类型
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    /// RTP 解析错误
    #[error("RTP parse error: {0}")]
    RtpParseError(String),

    /// RTCP 解析错误
    #[error("RTCP parse error: {0}")]
    RtcpParseError(String),

    /// 编解码协商失败
    #[error("codec negotiation failed: {0}")]
    CodecNegotiationFailed(String),

    /// 会话错误
    #[error("session error: {0}")]
    SessionError(String),

    /// SDP 集成错误
    #[error("SDP integration error: {0}")]
    SdpError(String),
}

/// 媒体操作结果类型
pub type MediaResult<T> = Result<T, MediaError>;

// 重导出常用类型
pub use codec::CodecInfo;
pub use codec::CodecNegotiator;
pub use rtcp::RtcpPacket;
pub use rtp::RtpPacket;
pub use session::MediaSession;
pub use session::MediaSessionConfig;
