# sip-rs

**A complete SIP protocol stack implementation in Rust** | **基于 Rust 的完整 SIP 协议栈实现**

[![CI](https://github.com/sip-rs/sip/actions/workflows/ci.yml/badge.svg)](https://github.com/sip-rs/sip/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/sip-rs.svg)](https://crates.io/crates/sip-rs)
[![Documentation](https://docs.rs/sip-rs/badge.svg)](https://docs.rs/sip-rs)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)

`sip-rs` 是一个用 Rust 编写的完整 SIP (Session Initiation Protocol) 协议栈实现，遵循 RFC 3261 规范，并内置 GB28181 国标适配层，可广泛应用于 VoIP、视频监控、即时通讯等场景。

A full-featured SIP protocol stack written in Rust, compliant with RFC 3261, with built-in GB28181 (Chinese national standard for video surveillance) adaptation layer.

## ✨ Features

- **Complete SIP Stack** — Full RFC 3261 implementation from message parsing to user agent
- **Async/Await Native** — Built on Tokio for high-performance asynchronous I/O
- **Layered Architecture** — Clean separation: Message → Transport → Transaction → Dialog → UA
- **GB28181 Ready** — Built-in support for Chinese national standard video surveillance protocol
- **TLS Support** — Secure transport via rustls (feature-gated)
- **MD5 Digest Auth** — RFC 2617 digest authentication for registration
- **Zero-copy Parsing** — Efficient message parsing using `bytes` crate
- **WVP Compatible** — Signaling server capabilities compatible with WVP platform

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Application Layer                       │
│                  (GB28181 Device / Server)                   │
└──────────────────────────┬──────────────────────────────────┘
                           │ SipEvent
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                       sip-ua (User Agent)                    │
│  ┌──────┐ ┌──────┐ ┌──────────┐ ┌───────────┐ ┌─────────┐ │
│  │ UAC  │ │ UAS  │ │ Dialog   │ │ Register  │ │ Subscr. │ │
│  │      │ │      │ │ Manager  │ │ Manager   │ │ Manager │ │
│  └──────┘ └──────┘ └──────────┘ └───────────┘ └─────────┘ │
│  ┌─────────────────┐ ┌─────────────────────────────────────┐│
│  │ DeviceRegistry  │ │ Gb28181Device / Gb28181Server      ││
│  └─────────────────┘ └─────────────────────────────────────┘│
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                   sip-dialog (Dialog Layer)                  │
│            Dialog ID management, early/confirmed state       │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                sip-transaction (Transaction Layer)           │
│  ┌────────────┐ ┌────────────┐ ┌──────────┐ ┌───────────┐ │
│  │ ICT (Inv.) │ │ NICT       │ │ IST      │ │ NIST      │ │
│  │ Client     │ │ Client     │ │ Server   │ │ Server    │ │
│  │ Timer A-G  │ │ Timer E/F  │ │ Timer G/H│ │ Timer J   │ │
│  └────────────┘ └────────────┘ └──────────┘ └───────────┘ │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                 sip-transport (Transport Layer)               │
│         UDP / TCP / TLS (rustls)   DNS Resolution            │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                 sip-message (Message Layer)                   │
│       Request / Response parsing & building (RFC 3261)       │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                   sip-core (Core Types)                      │
│       Error types, config, metrics, shared utilities         │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                    GB28181 Extension Crates                   │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────────────┐│
│  │ sip-sdp      │ │gb28181-codec │ │ gb28181-xml          ││
│  │ SDP + GB     │ │ 20-digit     │ │ MANSCDP XML          ││
│  │ extensions   │ │ encoding     │ │ parse & build        ││
│  └──────────────┘ └──────────────┘ └──────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

## 🚀 Quick Start

### Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
sip-ua = "0.1"
# Or use individual crates:
# sip-message = "0.1"
# sip-transport = "0.1"
# sip-transaction = "0.1"
# sip-dialog = "0.1"
# sip-registration = "0.1"
```

### Basic Usage — SIP Registration

```rust
use sip_ua::{SipEngine, SipEvent};
use sip_core::config::SipConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = SipConfig::builder()
        .aor("sip:alice@example.com")
        .contact("sip:alice@192.168.1.100:5060")
        .registrar("sip:registrar.example.com")
        .build()?;

    let mut engine = SipEngine::new(config);
    engine.start().await?;

    // Register with the SIP server
    engine.register().await?;

    // Receive events
    let mut events = engine.event_receiver().unwrap();
    while let Some(event) = events.recv().await {
        match event {
            SipEvent::IncomingCall(invite) => {
                println!("Incoming call from: {}", invite.from);
            }
            SipEvent::RegistrationOk => {
                println!("Registration successful!");
            }
            _ => {}
        }
    }

    Ok(())
}
```

### GB28181 Device Example

```rust
use sip_ua::gb28181::{Gb28181Device, Gb28181Config};
use sip_core::config::SipConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sip_config = SipConfig::builder()
        .aor("sip:34020000001320000001@3402000000")
        .contact("sip:34020000001320000001@192.168.1.100:5060")
        .build()?;

    let gb_config = Gb28181Config::new("34020000001320000001")
        .server_id("34020000002000000001")
        .server_domain("3402000000");

    let mut device = Gb28181Device::new(sip_config, gb_config);
    device.start().await?;

    // Device will automatically:
    // - Register with the platform
    // - Send heartbeat keep-alives
    // - Respond to catalog queries
    // - Handle video-on-demand requests

    Ok(())
}
```

### GB28181 Server Example

```rust
use sip_ua::gb28181_server::{Gb28181Server, Gb28181ServerConfig};
use sip_ua::device_registry::DeviceRegistry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server_config = Gb28181ServerConfig::new()
        .sip_domain("3402000000")
        .server_id("34020000002000000001");

    let mut server = Gb28181Server::new(server_config);
    server.start().await?;

    // Query device catalog
    let catalog = server.query_catalog("34020000001320000001").await?;

    // Request live video
    let sdp = server.invite_live("34020000001320000001", "34020000001320000002").await?;

    Ok(())
}
```

## 📦 Crates

| Crate | Description |
|-------|-------------|
| [`sip-core`](./crates/sip-core) | Core types, error definitions, configuration, metrics, and shared utilities |
| [`sip-message`](./crates/sip-message) | SIP message parsing and building (RFC 3261) — Request/Response, headers, URI |
| [`sip-transport`](./crates/sip-transport) | Transport layer — UDP/TCP/TLS (rustls) with DNS resolution |
| [`sip-transaction`](./crates/sip-transaction) | Transaction layer — 4 state machines (ICT/NICT/IST/NIST), Timer A-K |
| [`sip-dialog`](./crates/sip-dialog) | Dialog layer — dialog ID management, early/confirmed state tracking |
| [`sip-registration`](./crates/sip-registration) | Registration layer — client/server registration with MD5 digest auth |
| [`sip-ua`](./crates/sip-ua) | User Agent — SipEngine, UAC/UAS, call control, event dispatch |
| [`sip-sdp`](./crates/sip-sdp) | SDP parser/builder with GB28181 media extensions |
| [`gb28181-codec`](./crates/gb28181-codec) | GB28181 20-digit national standard encoding — parse/validate/generate |
| [`gb28181-xml`](./crates/gb28181-xml) | GB28181 XML (MANSCDP) message parsing and building |

## 🇨🇳 GB28181 Support

[GB28181](https://en.wikipedia.org/wiki/GB/T_28181) is the Chinese national standard for security and protection video surveillance networking systems. `sip-rs` provides comprehensive GB28181 support:

### Device Side (`Gb28181Device`)
- Device registration and authentication
- Heartbeat keep-alive
- Catalog query response
- Video on demand (live / playback / download)
- PTZ control
- Alarm notification
- Mobile position reporting

### Platform Side (`Gb28181Server`)
- Device registration and authentication (Registrar)
- Device registry with heartbeat monitoring (`DeviceRegistry`)
- Catalog query / subscription
- Video on demand (live / playback / download)
- PTZ control
- Record query
- Alarm subscription & notification
- Mobile position tracking & subscription
- Device tree management
- SUBSCRIBE/NOTIFY subscription framework

### WVP Signaling Server Compatibility
`sip-rs` implements signaling server capabilities compatible with the [WVP-PRO](https://github.com/648540858/wvp-PRO28181) platform, including:
- Device registration and authentication
- Catalog query and subscription
- Video on demand (live/playback/download)
- PTZ control
- Record query
- Alarm notification
- Mobile position tracking
- Device tree management

## 🛠️ Development

### Prerequisites

- Rust 1.70+ (2021 edition)
- Tokio async runtime

### Build

```bash
cargo build --all
```

### Test

```bash
cargo test --all
```

### Lint

```bash
cargo clippy --all -- -D warnings
cargo fmt --all -- --check
```

### Documentation

```bash
cargo doc --all --no-deps --open
```

## 📄 License

This project is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.