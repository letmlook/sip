# siprs-transport

SIP 协议栈传输层实现，支持 UDP、TCP、TLS 三种传输协议。

## 简介

`siprs-transport` 提供 SIP 协议传输层的完整实现，包括 UDP、TCP、TLS (rustls) 三种传输协议，DNS 解析（RFC 3263），连接池管理和统一传输管理器。

## 主要功能

- **Transport trait** — 传输协议抽象接口，支持自定义实现
- **SipCodec** — TCP 流分帧编解码器（基于 Content-Length）
- **UdpTransport** — UDP 传输实现
- **TcpConnection/TcpListener** — TCP 传输实现
- **TlsConnection** — TLS 传输实现（feature-gated，需要 `tls-rustls` feature）
- **ConnectionPool** — 连接池与连接复用
- **DnsResolver** — DNS 解析（RFC 3263），支持 NAPTR/SRV/A 记录查询
- **TransportManager** — 统一传输管理器，管理所有传输协议

## 传输协议选择规则

| URI 格式 | 传输协议 |
|----------|---------|
| `sips:bob@example.com` | TLS |
| `sip:bob@example.com;transport=tcp` | TCP |
| `sip:bob@example.com`（无传输参数） | UDP |
| UDP 消息超过 MTU 限制 | 自动切换 TCP |

## 使用示例

### 启动传输层

```rust
use std::sync::Arc;
use siprs_transport::TransportManager;
use siprs_core::config::{TransportConfig, TlsConfig};
use siprs_core::metrics::SipMetrics;

let mut manager = TransportManager::new(
    TransportConfig::default(),
    TlsConfig::default(),
    Arc::new(SipMetrics::new()),
);

// 启动传输层
manager.start("0.0.0.0:5060".parse().unwrap()).await.unwrap();
```

### 发送与接收消息

```rust
// 发送消息
manager.send(&message, "sip:bob@example.com;transport=tcp").await?;

// 接收消息
let mut stream = manager.message_stream().unwrap();
while let Some(event) = stream.recv().await {
    // 处理传输事件
}
```

### 自定义传输实现

```rust
use async_trait::async_trait;
use siprs_transport::traits::Transport;
use siprs_core::{TransportError, TransportProtocol};

struct MyTransport;

#[async_trait]
impl Transport for MyTransport {
    fn protocol(&self) -> TransportProtocol {
        TransportProtocol::Udp
    }

    async fn send_message(&self, message: &[u8], addr: SocketAddr) -> Result<(), TransportError> {
        // 自定义发送逻辑
        Ok(())
    }

    async fn close(&self) -> Result<(), TransportError> {
        Ok(())
    }
}
```

## Feature Flags

| Feature | 默认 | 说明 |
|---------|------|------|
| `tls-rustls` | 启用 | 基于 rustls 的 TLS 传输 |
| `tls-native` | 禁用 | 预留：基于 native-tls 的 TLS 传输 |

## 与其他 crate 的关系

| 依赖 crate | 使用内容 |
|------------|---------|
| `siprs-core` | 配置、错误类型、指标、核心类型 |
| `siprs-message` | 消息类型（发送/接收） |
| `siprs-transaction` | 通过 TransportManager 发送/接收消息 |
| `siprs-ua` | 通过 SipEngine 使用传输层 |

## 许可证

MIT OR Apache-2.0