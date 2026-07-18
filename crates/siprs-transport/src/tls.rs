//! SIP TLS 传输实现（feature-gated）
//!
//! 在 TCP 传输基础上增加 TLS 加密层，使用 `rustls` 建立 TLS 1.2+ 连接。
//! 仅在 `tls-rustls` feature 启用时可用。

#[cfg(feature = "tls-rustls")]
mod rustls_transport;

#[cfg(feature = "tls-rustls")]
pub use rustls_transport::TlsConnection;
