# sip-rs

**基于 Rust 的完整 SIP 协议栈 + GB28181 国标信令服务器**

[![CI](https://github.com/letmlook/sip/actions/workflows/ci.yml/badge.svg)](https://github.com/letmlook/sip/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/siprs.svg)](https://crates.io/crates/siprs)
[![Documentation](https://docs.rs/siprs/badge.svg)](https://docs.rs/siprs)
[![Downloads](https://img.shields.io/crates/d/siprs.svg)](https://crates.io/crates/siprs)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)

`sip-rs` 是一个用 Rust 编写的完整 SIP (Session Initiation Protocol) 协议栈实现，遵循 RFC 3261 规范，并内置 GB28181 国标适配层，可广泛应用于 VoIP、视频监控、即时通讯等场景。

## ✨ 功能特性

- **完整 SIP 协议栈** — 遵循 RFC 3261，从消息解析到用户代理的完整实现
- **Async/Await 原生** — 基于 Tokio 构建高性能异步 I/O
- **五层分层架构** — 消息层 → 传输层 → 事务层 → 对话层 → UA 层，职责清晰
- **GB28181 国标适配** — 内置 GB/T 28181 国标视频监控协议完整支持
- **TLS 安全传输** — 基于 rustls 的 TLS 传输（feature-gated）
- **MD5 摘要认证** — RFC 2617 摘要认证，自动处理 401/407 挑战
- **零拷贝解析** — 基于 `bytes` crate 的高效消息解析
- **WVP 兼容** — 信令服务器功能兼容 WVP-PRO 平台
- **无锁指标** — 基于 `AtomicU64` 的并发安全运行指标监控
- **类型安全** — 强类型 API，编译期保证正确性

## 🏗️ 架构

```
┌─────────────────────────────────────────────────────────────┐
│                      应用层 (Application)                     │
│                  (GB28181 设备端 / 平台端)                     │
└──────────────────────────┬──────────────────────────────────┘
                           │ SipEvent / Gb28181Event
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                       siprs-ua (用户代理层)                    │
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
│                   siprs-dialog (对话层)                        │
│            对话 ID 管理、Early/Confirmed 状态、路由集           │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                siprs-transaction (事务层)                      │
│  ┌────────────┐ ┌────────────┐ ┌──────────┐ ┌───────────┐ │
│  │ ICT (Inv.) │ │ NICT       │ │ IST      │ │ NIST      │ │
│  │ Client     │ │ Client     │ │ Server   │ │ Server    │ │
│  │ Timer A-G  │ │ Timer E/F  │ │ Timer G/H│ │ Timer J   │ │
│  └────────────┘ └────────────┘ └──────────┘ └───────────┘ │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                 siprs-transport (传输层)                       │
│         UDP / TCP / TLS (rustls)   DNS Resolution            │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                 siprs-message (消息层)                         │
│       Request / Response 解析与构建 (RFC 3261)                │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                   siprs-core (核心类型)                        │
│       错误类型、配置、运行指标、共享工具                         │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                    GB28181 扩展 Crate                          │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────────────┐│
│  │ siprs-sdp    │ │siprs-gb28181-│ │ siprs-gb28181-xml    ││
│  │ SDP + GB     │ │codec 20-digit│ │ MANSCDP XML          ││
│  │ extensions   │ │ encoding     │ │ parse & build        ││
│  └──────────────┘ └──────────────┘ └──────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

## 🚀 快速开始

### 安装

使用 `cargo add` 快速安装：

```bash
# 安装统一 crate（包含所有子 crate）
cargo add siprs

# 或按需安装独立 crate：
# cargo add siprs-ua            # 用户代理（最常用）
# cargo add siprs-core          # 核心类型
# cargo add siprs-message       # 消息解析/构建
# cargo add siprs-transport     # 传输层
# cargo add siprs-transaction   # 事务层
# cargo add siprs-dialog        # 对话层
# cargo add siprs-registration  # 注册层
# cargo add siprs-sdp           # SDP 解析/构建
# cargo add siprs-media         # 媒体协商/RTP/RTCP
# cargo add siprs-gb28181-codec # 国标编码
# cargo add siprs-gb28181-xml   # 国标 XML
```

或在 `Cargo.toml` 中手动添加：

```toml
[dependencies]
siprs = "0.1"

# 或按需引入独立 crate：
# siprs-ua = "0.1"            # 用户代理（最常用）
# siprs-core = "0.1"          # 核心类型
# siprs-message = "0.1"       # 消息解析/构建
# siprs-transport = "0.1"     # 传输层
# siprs-transaction = "0.1"   # 事务层
# siprs-dialog = "0.1"        # 对话层
# siprs-registration = "0.1"  # 注册层
# siprs-sdp = "0.1"           # SDP 解析/构建
# siprs-media = "0.1"         # 媒体协商/RTP/RTCP
# siprs-gb28181-codec = "0.1" # 国标编码
# siprs-gb28181-xml = "0.1"   # 国标 XML
```

### 基本使用 — SIP 注册

```rust
use siprs_ua::{SipEngine, SipEvent};
use siprs_core::config::SipConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = SipConfig::builder()
        .aor("sip:alice@example.com")
        .contact("sip:alice@192.168.1.100:5060")
        .registrar_server("sip:registrar.example.com")
        .credentials("alice", "password123")
        .build()?;

    let mut engine = SipEngine::new(config);
    engine.start().await?;

    // 注册到 SIP 服务器
    engine.register().await?;

    // 接收事件
    let mut events = engine.event_receiver().unwrap();
    while let Some(event) = events.recv().await {
        match event {
            SipEvent::IncomingCall { call_id, from, .. } => {
                println!("来电: {} from {}", call_id, from);
            }
            SipEvent::RegistrationResult { result, .. } => {
                println!("注册结果: {:?}", result);
            }
            _ => {}
        }
    }

    Ok(())
}
```

### GB28181 设备端示例

```rust
use siprs_ua::gb28181::{Gb28181Device, Gb28181Config, Gb28181Event};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Gb28181Config {
        device_id: "34020000001320000001".to_string(),
        server_addr: "192.168.1.1".to_string(),
        server_port: 5060,
        server_domain: "3402000000".to_string(),
        username: "34020000001320000001".to_string(),
        password: "12345678".to_string(),
        expires: 3600,
        heartbeat_interval: 60,
        local_ip: "192.168.1.100".to_string(),
        local_port: 5060,
    };

    let mut device = Gb28181Device::new(config);
    device.start().await?;

    // 设备将自动完成：
    // - 向平台注册（REGISTER + 摘要认证）
    // - 心跳保活（MESSAGE Keepalive）
    // - 响应目录查询
    // - 处理视频点播请求

    let mut event_rx = device.event_receiver().unwrap();
    while let Some(event) = event_rx.recv().await {
        match event {
            Gb28181Event::Registered => println!("注册成功"),
            Gb28181Event::CatalogQuery { sn, device_id } => {
                device.handle_catalog_query(sn, &device_id, vec![]).await;
            }
            Gb28181Event::InviteReceived { call_id, .. } => {
                device.accept_invite(&call_id, "192.168.1.100", 6000).await;
            }
            _ => {}
        }
    }

    Ok(())
}
```

### GB28181 平台端示例

```rust
use siprs_ua::gb28181_server::{Gb28181Server, Gb28181ServerConfig, Gb28181ServerEvent};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Gb28181ServerConfig {
        server_id: "34020000002000000001".to_string(),
        server_domain: "3402000000".to_string(),
        sip_ip: "192.168.1.1".to_string(),
        sip_port: 5060,
        realm: "3402000000".to_string(),
        auth_password: "12345678".to_string(),
        heartbeat_timeout: 180,
        register_expires: 3600,
    };

    let mut server = Gb28181Server::new(config);
    server.start().await?;

    // 查询设备目录
    server.query_catalog("34020000001320000001").await?;

    // 发起实时视频点播
    let call_id = server.invite_live("34020000001320000001").await?;

    // 云台控制
    server.ptz_control("34020000001320000001", siprs_gb28181_xml::PtzDirection::Up, 31).await?;

    Ok(())
}
```

## 📦 Crate 列表

| Crate | 说明 | crates.io |
|-------|------|-----------|
| [`siprs`](.) | 统一 re-export 所有子 crate | [![Crates.io](https://img.shields.io/crates/v/siprs.svg)](https://crates.io/crates/siprs) |
| [`siprs-core`](./crates/siprs-core) | 核心类型、错误处理、配置、运行指标 | [![Crates.io](https://img.shields.io/crates/v/siprs-core.svg)](https://crates.io/crates/siprs-core) |
| [`siprs-message`](./crates/siprs-message) | SIP 消息解析与构建 (RFC 3261) | [![Crates.io](https://img.shields.io/crates/v/siprs-message.svg)](https://crates.io/crates/siprs-message) |
| [`siprs-transport`](./crates/siprs-transport) | 传输层 — UDP/TCP/TLS (rustls) | [![Crates.io](https://img.shields.io/crates/v/siprs-transport.svg)](https://crates.io/crates/siprs-transport) |
| [`siprs-transaction`](./crates/siprs-transaction) | 事务层 — 4 种状态机、Timer A~K | [![Crates.io](https://img.shields.io/crates/v/siprs-transaction.svg)](https://crates.io/crates/siprs-transaction) |
| [`siprs-dialog`](./crates/siprs-dialog) | 对话层 — 对话 ID 管理、状态跟踪 | [![Crates.io](https://img.shields.io/crates/v/siprs-dialog.svg)](https://crates.io/crates/siprs-dialog) |
| [`siprs-registration`](./crates/siprs-registration) | 注册层 — MD5 摘要认证、Registrar | [![Crates.io](https://img.shields.io/crates/v/siprs-registration.svg)](https://crates.io/crates/siprs-registration) |
| [`siprs-ua`](./crates/siprs-ua) | 用户代理 — SipEngine、GB28181 设备/平台 | [![Crates.io](https://img.shields.io/crates/v/siprs-ua.svg)](https://crates.io/crates/siprs-ua) |
| [`siprs-sdp`](./crates/siprs-sdp) | SDP 解析/构建 + GB28181 媒体扩展 | [![Crates.io](https://img.shields.io/crates/v/siprs-sdp.svg)](https://crates.io/crates/siprs-sdp) |
| [`siprs-media`](./crates/siprs-media) | 媒体协商、RTP/RTCP 包处理、编解码协商 | [![Crates.io](https://img.shields.io/crates/v/siprs-media.svg)](https://crates.io/crates/siprs-media) |
| [`siprs-gb28181-codec`](./crates/siprs-gb28181-codec) | GB28181 20 位国标编码解析/生成 | [![Crates.io](https://img.shields.io/crates/v/siprs-gb28181-codec.svg)](https://crates.io/crates/siprs-gb28181-codec) |
| [`siprs-gb28181-xml`](./crates/siprs-gb28181-xml) | GB28181 XML (MANSCDP) 消息处理 | [![Crates.io](https://img.shields.io/crates/v/siprs-gb28181-xml.svg)](https://crates.io/crates/siprs-gb28181-xml) |

## 🇨🇳 GB28181 支持

[GB28181](https://en.wikipedia.org/wiki/GB/T_28181) 是中国国家标准《安全防范视频监控联网系统信息传输、交换、控制技术要求》。`sip-rs` 提供全面的 GB28181 支持：

### 设备端 (`Gb28181Device`)

- 设备注册与摘要认证
- 心跳保活（MESSAGE Keepalive）
- 目录查询响应（MESSAGE Catalog Response）
- 设备信息查询响应
- 视频点播（INVITE → 200 OK 含 GB28181 SDP）
- 云台控制响应
- 挂断通话（BYE）

### 平台端 (`Gb28181Server`)

- 设备注册与认证（Registrar）
- 设备注册表与心跳监控（`DeviceRegistry`）
- 目录查询/订阅
- 视频点播（实时/回放/下载）
- 云台控制
- 录像查询
- 报警订阅与通知
- 移动位置追踪与订阅
- 设备树管理
- SUBSCRIBE/NOTIFY 订阅框架

### WVP 信令服务器兼容

`sip-rs` 实现了与 [WVP-PRO](https://github.com/648540858/wvp-PRO28181) 平台兼容的信令服务器功能，包括设备注册认证、目录查询订阅、视频点播、云台控制、录像查询、报警通知、移动位置追踪和设备树管理。

## 🛠️ 开发指南

### 环境要求

- Rust 1.75+（2021 edition）
- Tokio 异步运行时

### 构建

```bash
cargo build --all
```

### 测试

```bash
cargo test --all
```

### 代码检查

```bash
cargo clippy --all -- -D warnings
cargo fmt --all -- --check
```

### 文档

```bash
cargo doc --all --no-deps --open
```

### 运行示例

```bash
# SIP 注册
cargo run -p siprs-ua --example registration

# SIP 呼叫
cargo run -p siprs-ua --example basic_call

# GB28181 设备端
cargo run -p siprs-ua --example gb28181_device

# GB28181 平台端
cargo run -p siprs-ua --example gb28181_server

# 自定义传输层
cargo run -p siprs-ua --example custom_transport

# 简单注册
cargo run -p siprs-ua --example simple_register

# 简单呼叫
cargo run -p siprs-ua --example simple_call

# GB28181 目录查询
cargo run -p siprs-ua --example gb28181_catalog
```

### 贡献

1. Fork 本仓库
2. 创建功能分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'Add amazing feature'`)
4. 推送分支 (`git push origin feature/amazing-feature`)
5. 创建 Pull Request

请确保所有提交通过 CI 检查（`cargo test`、`cargo clippy`、`cargo fmt`）。

## 📄 许可证

本项目采用双重许可：

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) 或 http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) 或 http://opensource.org/licenses/MIT)

任选其一。除非您明确声明，否则您提交的任何贡献将按照上述双重许可进行授权，不附加任何额外条款或条件。
