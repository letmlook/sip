//! SIP Message - Message parsing and building for the SIP protocol stack
//!
//! 本 crate 提供 SIP 消息的核心类型定义、URI 解析/构建、头部类型定义与解析、
//! 完整消息解析器和构建器功能。
//!
//! # 模块结构
//!
//! - [`types`] - SIP 方法、BranchId、Tag、CallId、请求/响应消息等核心类型
//! - [`uri`] - SIP URI（sip:/sips:）解析与构建
//! - [`headers`] - 消息头类型定义、集合与解析
//! - [`parser`] - SIP 消息解析器
//! - [`builder`] - SIP 消息构建器

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
