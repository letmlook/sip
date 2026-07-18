# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-07-19

### 修复
- 修正 crates.io 上显示的仓库地址（github.com/letmlook/sip）

## [0.1.0] - 2026-07-19

### 首次发布

- 完整的 SIP 协议栈实现（消息→传输→事务→对话→UA 五层架构）
- GB28181 国标信令服务器（设备注册、目录查询、视频点播、云台控制、录像回放）
- WebSocket 传输支持（RFC 7118）
- 媒体层（RTP/RTCP 解析、编解码协商、媒体会话管理）
- GB28181 高级功能（目录订阅通知、移动位置上报、多级联）
- 826+ 测试全部通过
- 零 clippy 警告
- CI 全绿

### Added
- Complete SIP protocol stack implementation (RFC 3261)
  - Message parsing and building (siprs-message)
  - Transport layer: UDP/TCP/TLS (siprs-transport)
  - Transaction layer: 4 state machines, Timer A-K (siprs-transaction)
  - Dialog management (siprs-dialog)
  - Registration with MD5 digest auth (siprs-registration)
  - User Agent core with SipEngine (siprs-ua)
- GB28181 adaptation layer
  - SDP parsing/building with GB28181 extensions (siprs-sdp)
  - 20-digit national standard encoding (siprs-gb28181-codec)
  - XML message processing (siprs-gb28181-xml)
  - Device-side implementation (Gb28181Device)
  - Platform-side implementation (Gb28181Server)
  - Device registry with heartbeat monitoring (DeviceRegistry)
  - SUBSCRIBE/NOTIFY subscription framework
  - Registrar server
- Media layer (siprs-media)
  - RTP/RTCP packet parsing and building
  - Codec negotiation
  - Media session management
- WVP signaling server capabilities
  - Device registration and authentication
  - Catalog query/subscription
  - Video on demand (live/playback/download)
  - PTZ control
  - Record query
  - Alarm notification
  - Mobile position tracking
  - Device tree management