# siprs-registration

[![Crates.io](https://img.shields.io/crates/v/siprs-registration.svg)](https://crates.io/crates/siprs-registration)
[![Documentation](https://docs.rs/siprs-registration/badge.svg)](https://docs.rs/siprs-registration)

SIP 注册层实现，支持客户端注册与注册服务器。

## 安装

```bash
cargo add siprs-registration
```

## 简介

`siprs-registration` 提供 SIP 注册功能的完整实现，包括注册状态管理、RFC 2617 MD5 摘要认证处理、注册管理器和注册服务器（Registrar）。

## 主要功能

- **注册状态管理** — `RegistrationId`、`RegistrationState`、`RegistrationInfo` 等类型
- **摘要认证** — RFC 2617 MD5 Digest Authentication，自动处理 401/407 挑战
- **注册管理器** — `RegistrationManager` 管理注册生命周期，支持自动刷新
- **注册服务器** — `Registrar` 实现完整注册服务器，支持内存存储和凭据查找
- **注册存储** — `RegistrationStore` trait 支持自定义存储后端
- **绑定信息** — `BindingInfo` 记录 AOR 与 Contact 的绑定关系

## 注册流程

### 客户端注册流程

```
客户端                                    注册服务器
  |                                          |
  |  REGISTER (无 Authorization)             |
  |----------------------------------------->|
  |                                          |
  |  401 Unauthorized (WWW-Authenticate)     |
  |<-----------------------------------------|
  |                                          |
  |  REGISTER (含 Authorization)             |
  |----------------------------------------->|
  |                                          |
  |  200 OK                                  |
  |<-----------------------------------------|
```

### 自动认证处理

`RegistrationManager` 内部自动处理 401/407 挑战：

1. 发送 REGISTER（无 Authorization 头部）
2. 收到 401/407 响应，提取挑战参数
3. 使用凭据计算 MD5 摘要认证响应
4. 重新发送 REGISTER（含 Authorization 头部）

## 使用示例

### 客户端注册

```rust
use std::sync::Arc;
use siprs_registration::manager::RegistrationManager;
use siprs_core::config::{RegistrationConfig, Credentials};
use siprs_core::metrics::SipMetrics;

let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
let config = RegistrationConfig::default();
let credentials = Some(Credentials {
    username: "alice".to_string(),
    password: "password123".to_string(),
    realm: None,
});
let metrics = Arc::new(SipMetrics::new());

let manager = RegistrationManager::new(config, credentials, event_tx, metrics);

// 发起注册
let (reg_id, request) = manager.register(
    "sip:alice@example.com",
    "sip:alice@192.168.1.1:5060",
    None,
).await.unwrap();
```

### 注册服务器

```rust
use std::sync::Arc;
use siprs_registration::registrar::{Registrar, MemoryRegistrationStore};
use siprs_registration::auth::DigestAuthHandler;

let store = Arc::new(MemoryRegistrationStore::new());
let auth_handler = Arc::new(DigestAuthHandler::new());

let mut registrar = Registrar::new(
    store,
    auth_handler,
    "example.com".to_string(),
    true,  // 要求认证
);

// 设置凭据查找
registrar = registrar.with_credential_lookup(Arc::new(|username| {
    Some("password123".to_string())
}));

// 处理 REGISTER 请求
let response = registrar.handle_register(&request).await?;
```

## 与其他 crate 的关系

| 依赖 crate | 使用内容 |
|------------|---------|
| `siprs-core` | 配置、错误类型、指标、核心类型 |
| `siprs-message` | 消息类型（REGISTER 构建/解析） |
| `siprs-transaction` | 事务层（REGISTER 事务） |
| `siprs-ua` | 注册管理器（SipEngine 内部使用）、Registrar（GB28181 平台端） |

## 许可证

MIT OR Apache-2.0