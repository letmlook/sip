# siprs-dialog

SIP 对话层实现，遵循 RFC 3261 Section 12。

## 简介

`siprs-dialog` 按照 RFC 3261 Section 12 实现完整的对话层，包括对话标识管理（Call-ID + LocalTag + RemoteTag 三元组）、对话状态流转（Early / Confirmed / Terminated）、路由集管理、对话内请求构建和 BYE 处理。

## 主要功能

- **对话标识** — Call-ID + LocalTag + RemoteTag 三元组唯一标识对话
- **对话状态** — Early（早期）/ Confirmed（确认）/ Terminated（终止）
- **对话创建** — UAC 侧从 1xx/2xx 响应创建，UAS 侧从请求创建
- **对话维护** — 状态流转、序列号管理、远端目标更新
- **路由集** — UAC 侧逆序、UAS 侧正序，符合 RFC 3261 规范
- **对话内请求** — CSeq 递增、Route 头部、Request-URI 构建
- **2xx 重传** — INVITE 2xx 响应重传时重新发送 ACK
- **对话终止** — BYE 请求处理与对话清理

## 对话状态机

```
                INVITE 请求
                    |
                    V
              +-----------+
              |   Early   |  (1xx 响应)
              +-----------+
                    |
                    | 2xx 响应
                    V
              +-----------+
              | Confirmed |  (对话建立)
              +-----------+
                    |
                    | BYE / 超时 / 错误
                    V
              +-----------+
              | Terminated|  (对话终止)
              +-----------+
```

## 使用示例

### 创建对话管理器

```rust
use std::sync::Arc;
use siprs_dialog::DialogManager;
use siprs_core::metrics::SipMetrics;

let (dialog_manager, dialog_event_rx) = DialogManager::with_event_channel(
    Arc::new(SipMetrics::new())
);
```

### UAC 侧创建对话

```rust
use siprs_dialog::create_uac_dialog_from_response;

// 从 2xx 响应创建对话
let dialog_id = create_uac_dialog_from_response(&response, &config)?;
println!("对话已创建: {}", dialog_id);
```

### 构建对话内请求

```rust
use siprs_dialog::build_in_dialog_request;
use siprs_message::Method;

// 在对话内构建 BYE 请求
let bye_request = dialog_manager
    .build_in_dialog_request(&dialog_id, Method::Bye)
    .await?;
```

### 终止对话

```rust
// 终止对话
dialog_manager
    .terminate_dialog(&dialog_id, "BYE sent".to_string())
    .await?;
```

## 与其他 crate 的关系

| 依赖 crate | 使用内容 |
|------------|---------|
| `siprs-core` | 错误类型、指标、核心类型 |
| `siprs-message` | 消息类型（请求/响应解析） |
| `siprs-transaction` | 事务层（对话创建触发） |
| `siprs-registration` | 无直接依赖 |
| `siprs-ua` | 对话管理器（SipEngine 内部使用） |

## 许可证

MIT OR Apache-2.0