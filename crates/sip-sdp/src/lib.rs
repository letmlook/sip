//! SIP SDP - SDP (Session Description Protocol) 解析/构建库
//!
//! 基于 RFC 4566 实现的 SDP 会话描述协议解析和构建工具，
//! 并支持 GB/T 28181 国标扩展属性（y=、f= 行）。
//!
//! # 模块结构
//!
//! - [`types`] - SDP 核心数据类型定义
//! - [`parser`] - SDP 文本解析器
//! - [`builder`] - SDP 构建器（Builder 模式）
//! - [`gb28181`] - GB28181 国标扩展
//!
//! # 快速开始
//!
//! ## 解析 SDP
//!
//! ```
//! use sip_sdp::parser::SdpParser;
//!
//! let sdp_text = "v=0\r\no=- 1234 1234 IN IP4 192.168.1.1\r\ns=Session\r\nc=IN IP4 192.168.1.1\r\nt=0 0\r\nm=video 5000 RTP/AVP 96\r\na=rtpmap:96 PS/90000\r\na=recvonly\r\n";
//! let sdp = SdpParser::parse(sdp_text).unwrap();
//! assert_eq!(sdp.version, 0);
//! assert_eq!(sdp.media_descriptions.len(), 1);
//! ```
//!
//! ## 构建 SDP
//!
//! ```
//! use sip_sdp::builder::SdpBuilder;
//! use sip_sdp::types::*;
//!
//! let origin = Origin {
//!     username: "-".to_string(),
//!     session_id: 1234,
//!     session_version: 1234,
//!     network_type: "IN".to_string(),
//!     address_type: "IP4".to_string(),
//!     unicast_address: "192.168.1.1".to_string(),
//! };
//!
//! let sdp = SdpBuilder::new(origin, "Test Session")
//!     .connection(Connection::ipv4("192.168.1.1"))
//!     .time(0, 0)
//!     .build();
//!
//! let sdp_str = sdp.to_sdp_string();
//! assert!(sdp_str.starts_with("v=0"));
//! ```
//!
//! ## GB28181 扩展
//!
//! ```
//! use sip_sdp::gb28181::*;
//!
//! let sdp = build_invite_sdp(
//!     "01234567890000000001",
//!     "192.168.1.100",
//!     5000,
//!     &MediaParam {
//!         video_encoding: VideoEncoding::PS,
//!         audio_encoding: AudioEncoding::G711A,
//!         stream_type: StreamType::Live,
//!     },
//! );
//!
//! assert_eq!(extract_ssrc(&sdp), Some("01234567890000000001".to_string()));
//! ```

pub mod builder;
pub mod gb28181;
pub mod parser;
pub mod types;

// 重导出常用类型，方便使用
pub use types::{
    Attribute, Bandwidth, Connection, Encryption, MediaDescription, MediaType, Origin, RepeatTime,
    SdpError, SdpResult, SessionDescription, TimeDescription,
};
