# siprs-gb28181-xml

[![Crates.io](https://img.shields.io/crates/v/siprs-gb28181-xml.svg)](https://crates.io/crates/siprs-gb28181-xml)
[![Documentation](https://docs.rs/siprs-gb28181-xml/badge.svg)](https://docs.rs/siprs-gb28181-xml)

GB28181 XML 正文处理库，实现 MANSCDP XML 消息的解析和构建。

## 安装

```bash
cargo add siprs-gb28181-xml
```

## 简介

`siprs-gb28181-xml` 实现 GB28181 MANSCDP XML 消息的解析和构建，支持查询（Query）、响应（Response）、控制（Control）三大类消息。GB28181 中 SIP MESSAGE 请求的消息体为 XML 格式，本 crate 提供类型安全的 XML 构建和解析接口。

## 主要功能

- **Query 查询消息** — 设备目录查询、设备信息查询、录像查询、设备状态查询
- **Response 响应消息** — 设备目录响应、设备信息响应、录像查询响应、报警通知
- **Control 控制消息** — 云台控制（PTZ）、远程启动、录像拖动
- **统一解析** — `parse_xml()` 自动检测根元素并解析为对应类型
- **类型安全** — `CmdType` 枚举覆盖所有命令类型
- **设备目录** — `DeviceItem` 设备目录项，包含编码、名称、状态、厂商等

## 支持的消息类型

### Query（查询）

| 命令类型 | 说明 |
|---------|------|
| `Catalog` | 设备目录查询 |
| `DeviceInfo` | 设备信息查询 |
| `RecordInfo` | 录像查询 |
| `DeviceStatus` | 设备状态查询 |

### Response（响应）

| 命令类型 | 说明 |
|---------|------|
| `Catalog` | 设备目录响应 |
| `DeviceInfo` | 设备信息响应 |
| `RecordInfo` | 录像查询响应 |
| `Alarm` | 报警通知 |

### Control（控制）

| 命令类型 | 说明 |
|---------|------|
| `DeviceControl` | 设备控制（PTZ/远程启动/录像拖动） |

## 使用示例

### 构建设备目录查询

```rust
use siprs_gb28181_codec::DeviceId;
use siprs_gb28181_xml::Query;

let device_id = DeviceId::parse("34020000002000000001").unwrap();
let query = Query::catalog(1, device_id);
let xml = query.to_xml();
```

### 构建录像查询

```rust
use siprs_gb28181_codec::DeviceId;
use siprs_gb28181_xml::Query;

let device_id = DeviceId::parse("34020000001320000001").unwrap();
let query = Query::record_query(10, device_id, "2024-01-01T00:00:00", "2024-01-31T23:59:59");
let xml = query.to_xml();
```

### 解析设备目录响应

```rust
use siprs_gb28181_xml::Response;

let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Response>
  <CmdType>Catalog</CmdType>
  <SN>1</SN>
  <DeviceID>34020000002000000001</DeviceID>
  <SumNum>1</SumNum>
  <DeviceList Num="1">
    <Item>
      <DeviceID>34020000001320000001</DeviceID>
      <Name>Camera1</Name>
      <Status>ON</Status>
    </Item>
  </DeviceList>
</Response>"#;

let response = Response::from_xml(xml).unwrap();
assert_eq!(response.device_list.len(), 1);
```

### 构建云台控制命令

```rust
use siprs_gb28181_codec::DeviceId;
use siprs_gb28181_xml::Control;

let device_id = DeviceId::parse("34020000001320000001").unwrap();
let control = Control::ptz(3, device_id, "A50F01001F0000");
let xml = control.to_xml();
```

### 统一解析

```rust
use siprs_gb28181_xml::{parse_xml, Message};

let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Query>
  <CmdType>Catalog</CmdType>
  <SN>1</SN>
  <DeviceID>34020000002000000001</DeviceID>
</Query>"#;

let msg = parse_xml(xml)?;
match msg {
    Message::Query(q) => println!("查询: {:?}", q.cmd_type),
    Message::Response(r) => println!("响应: {:?}", r.cmd_type),
    Message::Control(c) => println!("控制: {:?}", c.cmd_type),
}
```

## 与其他 crate 的关系

| 依赖 crate | 使用内容 |
|------------|---------|
| `siprs-gb28181-codec` | `DeviceId` 用于 XML 消息中的设备编码 |
| `siprs-ua` | XML 消息构建/解析（GB28181 设备端/平台端） |

## 许可证

MIT OR Apache-2.0