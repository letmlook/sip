//! sip-rs - A complete SIP protocol stack implementation in Rust
//!
//! This crate provides a full SIP (Session Initiation Protocol) stack
//! implementation following RFC 3261, including message parsing/building,
//! transport, transaction, dialog, registration, and user agent layers,
//! with GB28181 support for video surveillance.
//!
//! # Crate Organization
//!
//! - [`sip_core`] - Core types, error handling, configuration, and metrics
//! - [`sip_message`] - SIP message parsing and building with URI, headers, and body support
//! - [`sip_transport`] - SIP transport layer with UDP, TCP, and TLS support
//! - [`sip_transaction`] - SIP transaction layer with RFC 3261 state machines and timers
//! - [`sip_dialog`] - SIP dialog management with route set and in-dialog request support
//! - [`sip_registration`] - SIP registration with MD5 digest authentication and registrar server
//! - [`sip_ua`] - SIP User Agent core with SipEngine, GB28181 device and platform support
//! - [`sip_sdp`] - SDP parsing and building with GB28181 extensions for video surveillance
//! - [`gb28181_codec`] - GB28181 20-digit national standard encoding parser and generator
//! - [`gb28181_xml`] - GB28181 XML message processing for device catalog, control, and queries

// Re-export all sub-crates for convenient access
pub use gb28181_codec;
pub use gb28181_xml;
pub use sip_core;
pub use sip_dialog;
pub use sip_message;
pub use sip_registration;
pub use sip_sdp;
pub use sip_transaction;
pub use sip_transport;
pub use sip_ua;
