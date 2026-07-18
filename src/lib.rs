//! sip-rs - A complete SIP protocol stack implementation in Rust
//!
//! This crate provides a full SIP (Session Initiation Protocol) stack
//! implementation following RFC 3261, including message parsing/building,
//! transport, transaction, dialog, registration, and user agent layers,
//! with GB28181 support for video surveillance.
//!
//! # Crate Organization
//!
//! - [`siprs_core`] - Core types, error handling, configuration, and metrics
//! - [`siprs_message`] - SIP message parsing and building with URI, headers, and body support
//! - [`siprs_transport`] - SIP transport layer with UDP, TCP, and TLS support
//! - [`siprs_transaction`] - SIP transaction layer with RFC 3261 state machines and timers
//! - [`siprs_dialog`] - SIP dialog management with route set and in-dialog request support
//! - [`siprs_registration`] - SIP registration with MD5 digest authentication and registrar server
//! - [`siprs_ua`] - SIP User Agent core with SipEngine, GB28181 device and platform support
//! - [`siprs_sdp`] - SDP parsing and building with GB28181 extensions for video surveillance
//! - [`siprs_gb28181_codec`] - GB28181 20-digit national standard encoding parser and generator
//! - [`siprs_gb28181_xml`] - GB28181 XML message processing for device catalog, control, and queries

// Re-export all sub-crates for convenient access
pub use siprs_gb28181_codec;
pub use siprs_gb28181_xml;
pub use siprs_core;
pub use siprs_dialog;
pub use siprs_message;
pub use siprs_registration;
pub use siprs_sdp;
pub use siprs_transaction;
pub use siprs_transport;
pub use siprs_ua;
