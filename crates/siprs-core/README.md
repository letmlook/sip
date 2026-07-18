# siprs-core

[![Crates.io](https://img.shields.io/crates/v/siprs-core.svg)](https://crates.io/crates/siprs-core)
[![Documentation](https://docs.rs/siprs-core/badge.svg)](https://docs.rs/siprs-core)

SIP 协议栈核心类型、错误处理、配置与运行指标。

## 安装

```bash
cargo add siprs-core
```

## 简介

`siprs-core` 是 SIP 协议栈的基础设施层，为所有上层 crate 提供统一的错误类型、配置管理、运行指标监控和核心数据类型。该 crate 不依赖任何 SIP 业务逻辑，仅定义协议栈各层共享的基础抽象。

## 主要功能

- **统一错误体系** — `SipError` 顶层错误类型，覆盖解析、构建、传输、事务、对话、注册、配置等所有错误场景，支持 `?` 操作符自动转换
- **Builder 模式配置** — `SipConfig` 支持 Builder 模式构建，包含传输层、事务层、TLS、注册等子配置
- **无锁运行指标** — `SipMetrics` 基于 `AtomicU64` 实现无锁并发安全的指标收集，支持消息、事务、对话、注册、传输五大类指标
- **核心类型定义** — `TransportProtocol`、`Host`、`StatusCode`、`SipVersion` 等协议栈全局共享类型

## 使用示例

### 构建 SIP 配置

```rust
use siprs_core::config::SipConfig;

let config = SipConfig::builder()
    .aor("sip:alice@example.com")
    .contact("sip:alice@192.168.1.100:5060")
    .registrar_server("sip:reg.example.com:5060")
    .credentials("alice", "password123")
    .sip_port(5060)
    .build()
    .expect("配置构建失败");
```

### 错误处理

```rust
use siprs_core::{SipError, ParseError};

fn parse_message() -> Result<(), SipError> {
    // 子模块错误自动转换为 SipError
    let err = ParseError::InvalidStartLine {
        detail: "bad line".into(),
    };
    Err(err)?; // ParseError -> SipError via ?
    Ok(())
}
```

### 运行指标监控

```rust
use std::sync::Arc;
use siprs_core::metrics::SipMetrics;

let metrics = Arc::new(SipMetrics::new());

// 递增计数器
metrics.inc_messages_received();
metrics.inc_active_dialogs();

// 获取快照
let snapshot = metrics.snapshot();
println!("接收消息: {}", snapshot.messages_received);
println!("活跃对话: {}", snapshot.active_dialogs);
```

## 与其他 crate 的关系

`siprs-core` 是协议栈的基石，所有其他 crate 均依赖它：

| 依赖 crate | 使用内容 |
|------------|---------|
| `siprs-message` | 错误类型、核心类型 |
| `siprs-transport` | 配置、错误类型、指标、核心类型 |
| `siprs-transaction` | 配置、错误类型、指标、核心类型 |
| `siprs-dialog` | 错误类型、指标、核心类型 |
| `siprs-registration` | 配置、错误类型、指标、核心类型 |
| `siprs-ua` | 配置、错误类型、指标、核心类型 |
| `siprs-sdp` | 错误类型 |
| `siprs-gb28181-codec` | 错误类型 |

## 许可证

MIT OR Apache-2.0