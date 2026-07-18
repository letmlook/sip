# siprs-media

媒体协商、RTP/RTCP 包处理和编解码协商库。

## 简介

`siprs-media` 提供 SIP 媒体层的核心功能，包括 RTP/RTCP 包的解析与构建、编解码协商以及媒体会话管理。在 GB28181 场景下，媒体流通常由流媒体服务器（如 ZLMediaKit、MediaMTX）处理，本 crate 的 RTP/RTCP 模块只做包的解析和构建，不做实际的网络传输。

## 主要功能

- **RTP 包解析/构建** — 基于 RFC 3550，支持 RFC 5285 头部扩展
- **RTCP 包解析/构建** — 支持 SR/RR/SDES/BYE/APP 五种包类型
- **编解码协商** — 从 SDP 提取编码列表，双向编码协商
- **媒体会话管理** — 会话创建、修改、终止，关联 SDP 和 RTP 端点

## 使用示例

### RTP 包解析

```rust
use siprs_media::rtp::RtpPacket;

let data = [0x80, 0x60, 0x00, 0x01, 0x00, 0x00, 0x00, 0xA0,
            0x12, 0x34, 0x56, 0x78, 0xAA, 0xBB, 0xCC, 0xDD];
let packet = RtpPacket::parse(&data).unwrap();
assert_eq!(packet.payload_type(), 96);
```

### RTP 包构建

```rust
use siprs_media::rtp::RtpPacket;

let mut packet = RtpPacket::new(96, 0x12345678, vec![1, 2, 3, 4]);
packet.set_marker(true);
packet.set_sequence_number(1);
packet.set_timestamp(90000);
let bytes = packet.to_bytes();
```

### RTCP 包解析

```rust
use siprs_media::rtcp::{RtcpPacket, SenderReport};

let sr = SenderReport::new(0x12345678);
let bytes = sr.to_bytes();
let parsed = RtcpPacket::parse(&bytes).unwrap();
```

### 编解码协商

```rust
use siprs_media::codec::{CodecNegotiator, CodecInfo};

let negotiator = CodecNegotiator::new();
let remote_codecs = vec![CodecInfo::pcma(), CodecInfo::ps()];
let result = negotiator.negotiate(&remote_codecs);
assert!(!result.is_empty());
```

### 媒体会话管理

```rust
use siprs_media::session::{MediaSession, MediaSessionConfig};

let config = MediaSessionConfig::gb28181_recv("192.168.1.100", 5000);
let mut session = MediaSession::with_config("session-1", &config);
// 设置远端 SDP 并协商编解码
session.set_remote_sdp(remote_sdp).unwrap();
session.activate().unwrap();
```

## 支持的编解码

### 音频编码

| 编码 | PT | 时钟率 | 说明 |
|------|-----|--------|------|
| PCMU | 0 | 8000 | G.711 μ-law |
| PCMA | 8 | 8000 | G.711 A-law |
| G722 | 9 | 8000 | G.722 |
| OPUS | 111 | 48000 | Opus |

### 视频编码

| 编码 | PT | 时钟率 | 说明 |
|------|-----|--------|------|
| H264 | 96 | 90000 | H.264/AVC |
| H265 | 97 | 90000 | H.265/HEVC |
| VP8 | 98 | 90000 | VP8 |
| VP9 | 99 | 90000 | VP9 |
| PS | 96 | 90000 | PS (GB28181 默认) |

## 与其他 crate 的关系

| 依赖 crate | 使用内容 |
|------------|---------|
| `siprs-core` | 错误处理 |
| `siprs-sdp` | SDP 解析/构建，提取编解码信息 |

## 许可证

MIT OR Apache-2.0