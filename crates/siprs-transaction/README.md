# siprs-transaction

SIP 协议栈事务层实现，遵循 RFC 3261 Section 17。

## 简介

`siprs-transaction` 按照 RFC 3261 Section 17 实现完整的事务层，包括四种事务状态机（ICT/NICT/IST/NIST）、定时器管理（Timer A~K）、事务匹配（基于 Branch ID + Method）和 ACK 处理。

## 主要功能

- **INVITE 客户端事务 (ICT)** — RFC 3261 Section 17.1.1，Timer A/B/D
- **非 INVITE 客户端事务 (NICT)** — RFC 3261 Section 17.1.2，Timer E/F
- **INVITE 服务端事务 (IST)** — RFC 3261 Section 17.2.1，Timer G/H/I
- **非 INVITE 服务端事务 (NIST)** — RFC 3261 Section 17.2.2，Timer J
- **定时器管理** — `TimerManager` 管理 Timer A~K，默认值遵循 RFC 3261
- **事务匹配** — 基于 Branch ID + Method 匹配请求与响应
- **ACK 处理** — 事务层内 ACK（2xx）和 TU 层 ACK（非 2xx）

## 事务状态机

### INVITE 客户端事务 (ICT)

```
                 |INVITE from TU
                 |send INVITE
      +----------+---------------+
      |          |               |
      |          V               |
      |      Calling             |
      |          |               |
      |          | Timer A/B     |
      |          V               |
      |      Proceeding          |
      |          |               |
      | 1xx      | 2xx           | 300-699
      |          |               | send ACK
      |          V               V
      |      Completed       Completed
      |          |               |
      |          | Timer D       | Timer D
      |          V               V
      |      Confirmed       Confirmed
      |          |               |
      |          V               V
      |      Terminated      Terminated
      +----------+---------------+
```

### 非 INVITE 客户端事务 (NICT)

```
                 |Request from TU
                 |send request
      +----------+---------------+
      |          |               |
      |          V               |
      |      Trying              |
      |          |               |
      |          | Timer E/F     |
      |          V               |
      |      Proceeding          |
      |          |               |
      | 1xx      | 200-699       |
      |          |               |
      |          V               V
      |      Completed       Completed
      |          |               |
      |          | Timer K       |
      |          V               |
      |      Terminated      Terminated
      +----------+---------------+
```

## 使用示例

### 创建事务管理器

```rust
use siprs_transaction::TransactionManager;
use siprs_core::config::TransactionConfig;
use siprs_core::metrics::SipMetrics;
use std::sync::Arc;

let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
let config = TransactionConfig::default();
let metrics = Arc::new(SipMetrics::new());

let mut manager = TransactionManager::new(config, event_tx, metrics);
manager.start();
```

### 发送请求与接收响应

```rust
use siprs_transaction::TransactionManager;
use siprs_message::SipRequest;

// 发送 INVITE 请求
let transaction_id = manager.send_request(invite_request).await?;

// 接收事务事件
while let Some(event) = event_rx.recv().await {
    match event {
        TransactionEvent::ResponseReceived { transaction_id, response } => {
            println!("收到响应: {} {}", response.status_code, response.reason_phrase);
        }
        TransactionEvent::Timeout { transaction_id } => {
            println!("事务超时: {}", transaction_id);
        }
        _ => {}
    }
}
```

## 定时器默认值

| 定时器 | 默认值 | 说明 |
|--------|--------|------|
| T1 | 500ms | RTT 估计值 |
| T2 | 4000ms | 最大重传间隔 |
| T4 | 5000ms | 消息存活时间 |
| Timer A | 2×T1 | INVITE 重传间隔 |
| Timer B | 64×T1 | INVITE 事务超时 |
| Timer D | 32s | INVITE 完成等待 |
| Timer E | 2×T1 | 非 INVITE 重传间隔 |
| Timer F | 64×T1 | 非 INVITE 事务超时 |
| Timer G | 2×T1 | INVITE 响应重传间隔 |
| Timer H | 64×T1 | INVITE 响应超时 |
| Timer I | T4 | INVITE 确认等待 |
| Timer J | 64×T1 | 非 INVITE 确认等待 |
| Timer K | T4 | 非 INVITE 确认等待 |

## 与其他 crate 的关系

| 依赖 crate | 使用内容 |
|------------|---------|
| `siprs-core` | 配置、错误类型、指标、核心类型 |
| `siprs-message` | 消息类型（请求/响应） |
| `siprs-transport` | 传输层（发送/接收消息） |
| `siprs-dialog` | 事务事件（对话创建触发） |
| `siprs-registration` | 事务层（REGISTER 事务） |
| `siprs-ua` | 事务管理器（SipEngine 内部使用） |

## 许可证

MIT OR Apache-2.0