# siprs-sdp

[![Crates.io](https://img.shields.io/crates/v/siprs-sdp.svg)](https://crates.io/crates/siprs-sdp)
[![Documentation](https://docs.rs/siprs-sdp/badge.svg)](https://docs.rs/siprs-sdp)

SDP (Session Description Protocol) 解析/构建库，支持 GB28181 国标扩展。

## 安装

```bash
cargo add siprs-sdp
```

## 简介

`siprs-sdp` 基于 RFC 4566 实现的 SDP 会话描述协议解析和构建工具，并支持 GB/T 28181 国标扩展属性（`y=`、`f=` 行）。SDP 是 SIP INVITE/200 OK 消息体中描述媒体会话的标准格式。

## 主要功能

- **SDP 解析** — `SdpParser` 解析 SDP 文本为结构化 `SessionDescription`
- **SDP 构建** — `SdpBuilder` Builder 模式构建 SDP
- **完整类型** — Origin、Connection、MediaDescription、Attribute 等所有 SDP 类型
- **GB28181 扩展** — `y=` 行（SSRC）、`f=` 行（媒体参数），支持 PS/H264/H265 视频编码和 G711A/G711U/AAC 音频编码
- **INVITE SDP** — `build_invite_sdp` 构建实时/回放/下载 INVITE SDP
- **200 OK SDP** — `build_ok_sdp` 构建 200 OK 响应 SDP

## 使用示例

### 解析 SDP

```rust
use siprs_sdp::parser::SdpParser;

let sdp_text = "v=0\r\no=- 1234 1234 IN IP4 192.168.1.1\r\ns=Session\r\nc=IN IP4 192.168.1.1\r\nt=0 0\r\nm=video 5000 RTP/AVP 96\r\na=rtpmap:96 PS/90000\r\na=recvonly\r\n";
let sdp = SdpParser::parse(sdp_text).unwrap();
assert_eq!(sdp.version, 0);
assert_eq!(sdp.media_descriptions.len(), 1);
```

### 构建 SDP

```rust
use siprs_sdp::builder::SdpBuilder;
use siprs_sdp::types::*;

let origin = Origin {
    username: "-".to_string(),
    session_id: 1234,
    session_version: 1234,
    network_type: "IN".to_string(),
    address_type: "IP4".to_string(),
    unicast_address: "192.168.1.1".to_string(),
};

let sdp = SdpBuilder::new(origin, "Test Session")
    .connection(Connection::ipv4("192.168.1.1"))
    .time(0, 0)
    .build();

let sdp_str = sdp.to_sdp_string();
assert!(sdp_str.starts_with("v=0"));
```

### GB28181 扩展

```rust
use siprs_sdp::gb28181::*;

// 构建实时视频 INVITE SDP
let sdp = build_invite_sdp(
    "01234567890000000001",  // SSRC
    "192.168.1.100",         // 媒体 IP
    5000,                    // 媒体端口
    &MediaParam {
        video_encoding: VideoEncoding::PS,
        audio_encoding: AudioEncoding::G711A,
        stream_type: StreamType::Live,
    },
);

// 提取 SSRC
assert_eq!(extract_ssrc(&sdp), Some("01234567890000000001".to_string()));
```

## GB28181 媒体参数

### 视频编码

| 编码 | 说明 |
|------|------|
| `VideoEncoding::PS` | PS 封装（默认） |
| `VideoEncoding::H264` | H.264 |
| `VideoEncoding::H265` | H.265 |
| `VideoEncoding::SVAC` | SVAC |

### 音频编码

| 编码 | 说明 |
|------|------|
| `AudioEncoding::G711A` | G.711 A-law（默认） |
| `AudioEncoding::G711U` | G.711 μ-law |
| `AudioEncoding::G7221` | G.722.1 |
| `AudioEncoding::G729` | G.729 |
| `AudioEncoding::AAC` | AAC |

### 流类型

| 类型 | 说明 |
|------|------|
| `StreamType::Live` | 实时视频 |
| `StreamType::Playback` | 录像回放 |
| `StreamType::Download` | 录像下载 |

## 与其他 crate 的关系

| 依赖 crate | 使用内容 |
|------------|---------|
| `siprs-core` | 错误类型 |
| `siprs-ua` | GB28181 INVITE/200 OK SDP 构建 |

## 许可证

MIT OR Apache-2.0