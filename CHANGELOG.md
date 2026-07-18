# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-07-18

### Added
- Complete SIP protocol stack implementation (RFC 3261)
  - Message parsing and building (sip-message)
  - Transport layer: UDP/TCP/TLS (sip-transport)
  - Transaction layer: 4 state machines, Timer A-K (sip-transaction)
  - Dialog management (sip-dialog)
  - Registration with MD5 digest auth (sip-registration)
  - User Agent core with SipEngine (sip-ua)
- GB28181 adaptation layer
  - SDP parsing/building with GB28181 extensions (sip-sdp)
  - 20-digit national standard encoding (gb28181-codec)
  - XML message processing (gb28181-xml)
  - Device-side implementation (Gb28181Device)
  - Platform-side implementation (Gb28181Server)
  - Device registry with heartbeat monitoring (DeviceRegistry)
  - SUBSCRIBE/NOTIFY subscription framework
  - Registrar server
- WVP signaling server capabilities
  - Device registration and authentication
  - Catalog query/subscription
  - Video on demand (live/playback/download)
  - PTZ control
  - Record query
  - Alarm notification
  - Mobile position tracking
  - Device tree management