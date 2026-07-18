# siprs-message

SIP 消息解析与构建库，遵循 RFC 3261 规范。

## 简介

`siprs-message` 提供 SIP 消息的核心类型定义、URI 解析/构建、头部类型定义与解析、完整消息解析器和构建器功能。支持 RFC 3261 定义的所有 SIP 方法和头部。

## 主要功能

- **SIP URI** — `sip:` / `sips:` URI 解析与构建，支持用户信息、参数、头部
- **消息头部** — Via、From/To、Call-ID、CSeq、Contact、Route、Authorization 等头部类型
- **消息解析** — `MessageParser` 零拷贝解析 SIP 请求和响应
- **消息构建** — `MessageBuilder` Builder 模式构建 SIP 消息
- **完整类型** — `SipRequest`、`SipResponse`、`SipMessage` 等核心消息类型

## 使用示例

### 解析 SIP 消息

```rust
use siprs_message::parser::MessageParser;

let raw = b"INVITE sip:bob@example.com SIP/2.0\r\n\
            Via: SIP/2.0/UDP 192.168.1.1:5060;branch=z9hG4bK776asdhds\r\n\
            From: <sip:alice@example.com>;tag=1928301774\r\n\
            To: <sip:bob@example.com>\r\n\
            Call-ID: a84b4c76e66710@pc33.example.com\r\n\
            CSeq: 314159 INVITE\r\n\
            Contact: <sip:alice@pc33.example.com>\r\n\
            Content-Length: 0\r\n\
            \r\n";

let message = MessageParser::parse(raw).expect("解析失败");
```

### 构建 SIP 请求

```rust
use siprs_message::builder::MessageBuilder;
use siprs_message::Method;

let request = MessageBuilder::request(Method::Invite, "sip:bob@example.com")
    .from("sip:alice@example.com", Some("a73hj"))
    .to("sip:bob@example.com", None)
    .call_id("a84b4c76e66710@pc33.example.com")
    .cseq(1, Method::Invite)
    .via("SIP/2.0/UDP", "192.168.1.1:5060", Some("z9hG4bK776asdhds"))
    .contact("sip:alice@pc33.example.com")
    .build()
    .expect("构建失败");
```

### 解析 SIP URI

```rust
use siprs_message::SipUri;

let uri: SipUri = "sip:alice@example.com;transport=tcp".parse().unwrap();
assert_eq!(uri.user_info().unwrap().user(), "alice");
assert_eq!(uri.host().to_string(), "example.com");
```

## 与其他 crate 的关系

| 依赖 crate | 使用内容 |
|------------|---------|
| `siprs-core` | 错误类型、核心类型 |
| `siprs-transport` | 消息类型（发送/接收） |
| `siprs-transaction` | 消息类型（事务匹配） |
| `siprs-dialog` | 消息类型（对话内请求构建） |
| `siprs-registration` | 消息类型（REGISTER 构建） |
| `siprs-ua` | 消息类型（UAC/UAS 消息构建） |

## 许可证

MIT OR Apache-2.0