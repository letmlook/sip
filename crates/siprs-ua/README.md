# siprs-ua

SIP 用户代理核心实现，提供 SipEngine、GB28181 设备端与平台端支持。

## 简介

`siprs-ua` 是 SIP 协议栈的最高层，提供用户代理的核心功能，包括呼叫控制（UAC/UAS）、注册管理、事件通知、GB28181 设备端和平台端完整实现。`SipEngine` 是整个协议栈的主入口，协调传输层、事务层、对话层和注册层。

## 主要功能

- **SipEngine** — SIP 协议栈主入口，协调所有下层组件
- **UAC/UAS** — 呼出控制（UAC）和呼入控制（UAS）
- **呼叫控制** — 发起呼叫、接听、拒绝、挂断、取消
- **注册管理** — 注册/注销，自动处理认证挑战
- **事件通知** — `SipEvent` 向上层应用通知来电、呼叫进展、注册结果等
- **GB28181 设备端** — `Gb28181Device` 设备注册、心跳、目录查询、视频点播
- **GB28181 平台端** — `Gb28181Server` 设备管理、目录查询、视频点播、云台控制
- **设备注册表** — `DeviceRegistry` 设备在线状态管理、设备树
- **订阅管理** — `SubscriptionManager` SUBSCRIBE/NOTIFY 订阅框架

## 架构

```
Application Layer
      ↕ (SipEvent / Gb28181Event / Gb28181ServerEvent)
   SipEngine / Gb28181Device / Gb28181Server
      ↕
┌────┼────┬────────┐
UAC  UAS  Dialog   Registration
      ↕
┌────┼────┬────────┐
Transport Transaction Dialog Registration
```

## 使用示例

### 基本 SIP 呼叫

```rust
use siprs_ua::{SipEngine, SipEvent};
use siprs_core::config::SipConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = SipConfig::builder()
        .aor("sip:alice@example.com")
        .contact("sip:alice@192.168.1.100:5060")
        .build()?;

    let mut engine = SipEngine::new(config);
    engine.start().await?;

    // 发起呼叫
    let call_id = engine.make_call("sip:bob@example.com", None, None).await?;

    // 接收事件
    let mut events = engine.event_receiver().unwrap();
    while let Some(event) = events.recv().await {
        match event {
            SipEvent::IncomingCall { call_id, from, .. } => {
                println!("来电: {} from {}", call_id, from);
            }
            SipEvent::CallEstablished { call_id, .. } => {
                println!("呼叫建立: {}", call_id);
            }
            _ => {}
        }
    }

    Ok(())
}
```

### GB28181 设备端

```rust
use siprs_ua::gb28181::{Gb28181Device, Gb28181Config, Gb28181Event};

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

// 处理事件
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
```

### GB28181 平台端

```rust
use siprs_ua::gb28181_server::{Gb28181Server, Gb28181ServerConfig, Gb28181ServerEvent};

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
```

## 与其他 crate 的关系

`siprs-ua` 是协议栈的最高层，依赖所有其他 crate：

| 依赖 crate | 使用内容 |
|------------|---------|
| `siprs-core` | 配置、错误类型、指标、核心类型 |
| `siprs-message` | 消息类型（请求/响应构建） |
| `siprs-transport` | 传输管理器 |
| `siprs-transaction` | 事务管理器 |
| `siprs-dialog` | 对话管理器 |
| `siprs-registration` | 注册管理器、Registrar |
| `siprs-sdp` | SDP 构建（GB28181 INVITE） |
| `siprs-gb28181-codec` | 设备编码解析 |
| `siprs-gb28181-xml` | XML 消息构建/解析 |

## 许可证

MIT OR Apache-2.0