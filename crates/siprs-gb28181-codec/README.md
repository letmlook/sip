# siprs-gb28181-codec

GB28181 20 位国标编码的解析、验证与生成库。

## 简介

`siprs-gb28181-codec` 实现 GB/T 28181 国标 20 位编码的解析、验证和生成功能。20 位编码是 GB28181 协议中设备标识的基础，用于 SIP URI 构建、设备注册、目录查询等所有交互场景。

## 编码格式

20 位编码格式：`AABBCCDDDDEEEFFF`

| 位置 | 长度 | 名称 | 说明 |
|------|------|------|------|
| 1-2 | 2 | 省份编码 | 11-65（行政区划前2位） |
| 3-4 | 2 | 部门编码 | 00-99 |
| 5-8 | 4 | 行业编码 | 02=社会治安，03=公安，04=交通，05=司法，06=消防，07=边防，08=其他 |
| 9-11 | 3 | 设备类型编码 | 111=球机，112=半球，113=固定枪机，114=遥控枪机，12x=编码设备，... |
| 12-14 | 3 | 行业扩展编码 | 000-999 |
| 15-20 | 6 | 序号 | 000001-999999 |

## 主要功能

- **编码解析** — `DeviceId::parse()` 将 20 位字符串解析为结构化数据
- **编码解码** — `DeviceId::decode()` 解码为省份、行业、设备类型等字段
- **编码生成** — `DeviceId::compose()` 从字段组合生成 20 位编码
- **SIP URI 构建** — `DeviceId::to_sip_uri()` 直接构建 SIP URI
- **省份枚举** — `Province` 覆盖全国 31 个省/直辖市/自治区
- **行业枚举** — `Industry` 覆盖 GB28181 定义的所有行业编码
- **设备类型** — `DeviceType` 覆盖摄像机、编码设备、报警器等所有设备类型

## 使用示例

### 解析国标编码

```rust
use siprs_gb28181_codec::{DeviceId, DeviceType, Industry, Province};

// 解析国标编码
let id = DeviceId::parse("34020000001320000001")?;

// 解码为结构化数据
let parsed = id.decode()?;
println!("省份: {} ({})", parsed.province.name(), parsed.province.code());
println!("行业: {}", parsed.industry.name());
println!("设备类型: {}", parsed.device_type.name());
```

### 生成国标编码

```rust
use siprs_gb28181_codec::{DeviceId, DeviceType, Industry, Province};

let id = DeviceId::compose(
    Province::Anhui,
    2,
    Industry::SocialSecurity,
    DeviceType::FixedCamera,
    0,
    1,
)?;

println!("生成的编码: {}", id);
```

### 构建 SIP URI

```rust
use siprs_gb28181_codec::DeviceId;

let id = DeviceId::parse("34020000001320000001")?;
let uri = id.to_sip_uri("192.168.1.1", 5060);
assert_eq!(uri, "sip:34020000001320000001@192.168.1.1:5060");
```

## 与其他 crate 的关系

| 依赖 crate | 使用内容 |
|------------|---------|
| `siprs-core` | 错误类型 |
| `siprs-gb28181-xml` | `DeviceId` 用于 XML 消息构建/解析 |
| `siprs-ua` | `DeviceId` 用于 GB28181 设备端/平台端 |

## 许可证

MIT OR Apache-2.0