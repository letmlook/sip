//! # siprs-message
//!
//! SIP 消息解析与构建库，遵循 RFC 3261 规范。
//!
//! 本 crate 提供 SIP 消息的核心类型定义、URI 解析/构建、头部类型定义与解析、
//! 完整消息解析器和构建器功能。
//!
//! # 模块结构
//!
//! - [`types`] — SIP 方法、BranchId、Tag、CallId、请求/响应消息等核心类型
//! - [`uri`] — SIP URI（sip:/sips:）解析与构建
//! - [`headers`] — 消息头类型定义、集合与解析
//! - [`parser`] — SIP 消息解析器
//! - [`builder`] — SIP 消息构建器
//!
//! # 示例
//!
//! ## 解析 SIP 消息
//!
//! ```ignore
//! use siprs_message::parser::MessageParser;
//!
//! let message = MessageParser::parse(raw_bytes).expect("解析失败");
//! ```
//!
//! ## 构建 SIP 请求
//!
//! ```ignore
//! use siprs_message::builder::MessageBuilder;
//! use siprs_message::Method;
//!
//! let request = MessageBuilder::request(Method::Invite, "sip:bob@example.com")
//!     .from("sip:alice@example.com", Some("a73hj"))
//!     .to("sip:bob@example.com", None)
//!     .call_id("a84b4c76e66710@pc33.example.com")
//!     .cseq(1, Method::Invite)
//!     .build()
//!     .expect("构建失败");
//! ```

pub mod builder;
pub mod headers;
pub mod parser;
pub mod types;
pub mod uri;

// 重导出常用类型
pub use types::{
    Body, BranchId, CallId, Method, RequestLine, SipMessage, SipRequest, SipResponse, StatusLine,
    Tag,
};

pub use uri::{SipUri, UriHeaders, UriParams, UriScheme, UserInfo};

pub use headers::{
    AuthHeader, CSeqHeader, ContactHeader, FromToHeader, HeaderCollection, HeaderName, HeaderValue,
    ParseHeadersResult, ParseWarning, RouteHeader, SentBy, ViaHeader,
};

pub use parser::MessageParser;

pub use builder::MessageBuilder;
