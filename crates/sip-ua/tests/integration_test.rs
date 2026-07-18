//! SIP 协议栈集成测试
//!
//! 覆盖消息解析往返一致性、URI 解析完整性、头部集合操作、
//! 错误处理和配置构建等核心功能。

use sip_core::config::SipConfig;
use sip_core::error::{BuildError, ConfigError, ParseError};
use sip_core::{Host, SipVersion, StatusCode, TransportProtocol};
use sip_message::builder::MessageBuilder;
use sip_message::headers::{CSeqHeader, HeaderCollection, HeaderName, HeaderValue, ViaHeader};
use sip_message::parser::MessageParser;
use sip_message::types::{
    Body, BranchId, CallId, Method, RequestLine, SipMessage, SipRequest, SipResponse, StatusLine,
    Tag,
};
use sip_message::uri::{SipUri, UriScheme};

// ============================================================================
// 辅助函数
// ============================================================================

/// 创建一个完整的 INVITE 请求消息
fn create_full_invite_request() -> SipMessage {
    let uri = SipUri::parse("sip:bob@example.com").unwrap();
    let mut headers = HeaderCollection::new();

    headers.insert(
        HeaderName::Via,
        HeaderValue::Via(ViaHeader::new(
            TransportProtocol::Udp,
            Host::Domain("192.168.1.1".to_string()),
            Some(5060),
        )),
    );
    headers.insert(
        HeaderName::From,
        HeaderValue::FromTo(sip_message::headers::FromToHeader {
            display_name: Some("Alice".to_string()),
            uri: SipUri::parse("sip:alice@example.com").unwrap(),
            tag: Some(Tag::new()),
        }),
    );
    headers.insert(
        HeaderName::To,
        HeaderValue::FromTo(sip_message::headers::FromToHeader {
            display_name: None,
            uri: SipUri::parse("sip:bob@example.com").unwrap(),
            tag: None,
        }),
    );
    headers.insert(HeaderName::CallId, HeaderValue::CallId(CallId::new()));
    headers.insert(
        HeaderName::CSeq,
        HeaderValue::CSeq(CSeqHeader::new(1, Method::Invite)),
    );
    headers.insert(
        HeaderName::Contact,
        HeaderValue::Contact(sip_message::headers::ContactHeader {
            display_name: None,
            uri: SipUri::parse("sip:alice@192.168.1.1:5060").unwrap(),
            expires: None,
        }),
    );
    headers.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

    let sdp_body = b"v=0\r\no=- 12345 1 IN IP4 192.168.1.1\r\ns=Session\r\nc=IN IP4 192.168.1.1\r\nt=0 0\r\nm=audio 5004 RTP/AVP 0\r\na=rtpmap:0 PCMU/8000\r\n".to_vec();

    SipMessage::Request(SipRequest {
        request_line: RequestLine {
            method: Method::Invite,
            request_uri: uri,
            version: SipVersion,
        },
        headers,
        body: Some(Body::new("application/sdp", sdp_body)),
    })
}

/// 创建一个完整的 200 OK 响应消息
fn create_full_200_ok_response() -> SipMessage {
    let mut headers = HeaderCollection::new();

    headers.insert(
        HeaderName::Via,
        HeaderValue::Via(ViaHeader::new(
            TransportProtocol::Udp,
            Host::Domain("192.168.1.1".to_string()),
            Some(5060),
        )),
    );
    headers.insert(
        HeaderName::From,
        HeaderValue::FromTo(sip_message::headers::FromToHeader {
            display_name: Some("Alice".to_string()),
            uri: SipUri::parse("sip:alice@example.com").unwrap(),
            tag: Some(Tag("alice-tag".to_string())),
        }),
    );
    headers.insert(
        HeaderName::To,
        HeaderValue::FromTo(sip_message::headers::FromToHeader {
            display_name: None,
            uri: SipUri::parse("sip:bob@example.com").unwrap(),
            tag: Some(Tag("bob-tag".to_string())),
        }),
    );
    headers.insert(
        HeaderName::CallId,
        HeaderValue::CallId(CallId("call-123@example.com".to_string())),
    );
    headers.insert(
        HeaderName::CSeq,
        HeaderValue::CSeq(CSeqHeader::new(1, Method::Invite)),
    );
    headers.insert(
        HeaderName::Contact,
        HeaderValue::Contact(sip_message::headers::ContactHeader {
            display_name: None,
            uri: SipUri::parse("sip:bob@192.168.1.2:5060").unwrap(),
            expires: None,
        }),
    );

    SipMessage::Response(SipResponse {
        status_line: StatusLine {
            version: SipVersion,
            status_code: StatusCode::OK,
            reason_phrase: "OK".to_string(),
        },
        headers,
        body: None,
    })
}

// ============================================================================
// 消息解析往返一致性测试
// ============================================================================

#[test]
fn test_invite_request_roundtrip() {
    // 构建 INVITE 请求
    let original = create_full_invite_request();

    // 序列化
    let builder = MessageBuilder::new();
    let bytes = builder.build(&original).expect("构建 INVITE 失败");

    // 重新解析
    let parser = MessageParser::default_parser();
    let parsed = parser.parse(&bytes).expect("解析 INVITE 失败");

    // 验证是请求消息
    assert!(parsed.is_request());
    assert!(!parsed.is_response());

    // 验证请求行
    if let SipMessage::Request(req) = parsed {
        assert_eq!(req.request_line.method, Method::Invite);
        assert_eq!(req.request_line.version, SipVersion);
        assert_eq!(
            req.request_line.request_uri.to_string(),
            "sip:bob@example.com"
        );

        // 验证头部存在
        assert!(req.headers.contains(&HeaderName::Via));
        assert!(req.headers.contains(&HeaderName::From));
        assert!(req.headers.contains(&HeaderName::To));
        assert!(req.headers.contains(&HeaderName::CallId));
        assert!(req.headers.contains(&HeaderName::CSeq));
        assert!(req.headers.contains(&HeaderName::Contact));
        assert!(req.headers.contains(&HeaderName::MaxForwards));

        // 验证消息体
        assert!(req.body.is_some());
        let body = req.body.unwrap();
        assert_eq!(body.content_type, "application/sdp");
        assert!(!body.content.is_empty());
    } else {
        panic!("期望是请求消息");
    }
}

#[test]
fn test_200_ok_response_roundtrip() {
    // 构建 200 OK 响应
    let original = create_full_200_ok_response();

    // 序列化
    let builder = MessageBuilder::new();
    let bytes = builder.build(&original).expect("构建 200 OK 失败");

    // 重新解析
    let parser = MessageParser::default_parser();
    let parsed = parser.parse(&bytes).expect("解析 200 OK 失败");

    // 验证是响应消息
    assert!(parsed.is_response());
    assert!(!parsed.is_request());

    // 验证状态行
    if let SipMessage::Response(resp) = parsed {
        assert_eq!(resp.status_line.version, SipVersion);
        assert_eq!(resp.status_line.status_code, StatusCode::OK);
        assert_eq!(resp.status_line.reason_phrase, "OK");

        // 验证头部存在
        assert!(resp.headers.contains(&HeaderName::Via));
        assert!(resp.headers.contains(&HeaderName::From));
        assert!(resp.headers.contains(&HeaderName::To));
        assert!(resp.headers.contains(&HeaderName::CallId));
        assert!(resp.headers.contains(&HeaderName::CSeq));

        // 验证无消息体
        assert!(resp.body.is_none());
    } else {
        panic!("期望是响应消息");
    }
}

#[test]
fn test_invite_with_sdp_roundtrip() {
    // 构建 INVITE 请求（含 SDP）
    let original = create_full_invite_request();

    // 序列化
    let builder = MessageBuilder::new();
    let bytes = builder.build(&original).expect("构建 INVITE 失败");

    // 重新解析
    let parser = MessageParser::default_parser();
    let parsed = parser.parse(&bytes).expect("解析 INVITE 失败");

    if let SipMessage::Request(req) = parsed {
        // 验证消息体内容一致性
        assert!(req.body.is_some());
        let body = req.body.as_ref().unwrap();
        assert_eq!(body.content_type, "application/sdp");
        assert!(body.content.len() > 0);

        // 验证 Content-Length 与实际消息体长度一致
        let content_length = req
            .headers()
            .get(&HeaderName::ContentLength)
            .and_then(|v| v.as_content_length());
        assert_eq!(content_length, Some(body.content.len()));
    } else {
        panic!("期望是请求消息");
    }
}

// ============================================================================
// URI 解析完整性测试
// ============================================================================

#[test]
fn test_sip_uri_basic() {
    let uri = SipUri::parse("sip:alice@example.com").unwrap();
    assert_eq!(uri.scheme, UriScheme::Sip);
    assert!(uri.user_info.is_some());
    assert_eq!(uri.user_info.as_ref().unwrap().user, "alice");
    assert!(uri.user_info.as_ref().unwrap().password.is_none());
    assert_eq!(uri.host, Host::Domain("example.com".to_string()));
    assert!(uri.port.is_none());
}

#[test]
fn test_sips_uri() {
    let uri = SipUri::parse("sips:bob@example.com").unwrap();
    assert_eq!(uri.scheme, UriScheme::Sips);
    assert!(uri.user_info.is_some());
    assert_eq!(uri.user_info.as_ref().unwrap().user, "bob");
}

#[test]
fn test_sip_uri_with_password() {
    let uri = SipUri::parse("sip:alice:secret@example.com").unwrap();
    assert!(uri.user_info.is_some());
    let user_info = uri.user_info.as_ref().unwrap();
    assert_eq!(user_info.user, "alice");
    assert_eq!(user_info.password.as_deref(), Some("secret"));
}

#[test]
fn test_sip_uri_with_port() {
    let uri = SipUri::parse("sip:alice@example.com:5060").unwrap();
    assert_eq!(uri.port, Some(5060));
}

#[test]
fn test_sip_uri_with_transport_param() {
    let uri = SipUri::parse("sip:alice@example.com;transport=tcp").unwrap();
    assert_eq!(uri.transport(), Some("tcp"));
}

#[test]
fn test_sip_uri_with_lr_param() {
    let uri = SipUri::parse("sip:proxy.example.com;lr").unwrap();
    assert!(uri.lr());
}

#[test]
fn test_sip_uri_ipv6() {
    let uri = SipUri::parse("sip:user@[::1]:5060").unwrap();
    assert!(uri.user_info.is_some());
    assert_eq!(uri.user_info.as_ref().unwrap().user, "user");
    assert_eq!(uri.host, Host::IPv6("::1".parse().unwrap()));
    assert_eq!(uri.port, Some(5060));
}

#[test]
fn test_sip_uri_ipv6_full() {
    let uri = SipUri::parse("sip:user@[2001:db8::1]:5060;transport=tcp").unwrap();
    assert_eq!(uri.host, Host::IPv6("2001:db8::1".parse().unwrap()));
    assert_eq!(uri.port, Some(5060));
    assert_eq!(uri.transport(), Some("tcp"));
}

#[test]
fn test_sip_uri_display_roundtrip() {
    let uri_str = "sip:alice@example.com:5060;transport=tcp";
    let uri = SipUri::parse(uri_str).unwrap();
    let display = uri.to_string();
    // 重新解析显示字符串
    let uri2 = SipUri::parse(&display).unwrap();
    assert_eq!(uri.scheme, uri2.scheme);
    assert_eq!(uri.host, uri2.host);
    assert_eq!(uri.port, uri2.port);
}

#[test]
fn test_sip_uri_invalid_scheme() {
    let result = SipUri::parse("http:alice@example.com");
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(matches!(e, ParseError::InvalidUri { .. }));
    }
}

#[test]
fn test_sip_uri_missing_host() {
    let result = SipUri::parse("sip:");
    assert!(result.is_err());
}

// ============================================================================
// 头部集合操作测试
// ============================================================================

#[test]
fn test_header_collection_insert_and_get() {
    let mut headers = HeaderCollection::new();

    headers.insert(
        HeaderName::CallId,
        HeaderValue::CallId(CallId("test@example.com".to_string())),
    );
    headers.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

    assert!(headers.contains(&HeaderName::CallId));
    assert!(headers.contains(&HeaderName::MaxForwards));
    assert!(!headers.contains(&HeaderName::Via));

    let call_id = headers.get(&HeaderName::CallId);
    assert!(call_id.is_some());
    if let Some(HeaderValue::CallId(cid)) = call_id {
        assert_eq!(cid.0, "test@example.com");
    } else {
        panic!("期望 CallId 头部值");
    }

    let max_forwards = headers.get(&HeaderName::MaxForwards);
    assert!(max_forwards.is_some());
    if let Some(HeaderValue::MaxForwards(mf)) = max_forwards {
        assert_eq!(*mf, 70);
    } else {
        panic!("期望 MaxForwards 头部值");
    }
}

#[test]
fn test_header_collection_remove() {
    let mut headers = HeaderCollection::new();

    headers.insert(
        HeaderName::CallId,
        HeaderValue::CallId(CallId("test@example.com".to_string())),
    );
    headers.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

    assert!(headers.contains(&HeaderName::CallId));
    assert_eq!(headers.len(), 2);

    headers.remove(&HeaderName::CallId);

    assert!(!headers.contains(&HeaderName::CallId));
    assert_eq!(headers.len(), 1);
}

#[test]
fn test_header_collection_iter() {
    let mut headers = HeaderCollection::new();

    headers.insert(
        HeaderName::Via,
        HeaderValue::Via(ViaHeader::new(
            TransportProtocol::Udp,
            Host::Domain("192.168.1.1".to_string()),
            Some(5060),
        )),
    );
    headers.insert(
        HeaderName::CallId,
        HeaderValue::CallId(CallId("test@example.com".to_string())),
    );
    headers.insert(
        HeaderName::CSeq,
        HeaderValue::CSeq(CSeqHeader::new(1, Method::Invite)),
    );

    let count = headers.iter().count();
    assert_eq!(count, 3);

    // 验证遍历顺序
    let names: Vec<String> = headers.iter().map(|(n, _)| n.to_string()).collect();
    assert_eq!(names[0], "Via");
    assert_eq!(names[1], "Call-ID");
    assert_eq!(names[2], "CSeq");
}

#[test]
fn test_header_collection_multiple_same_name() {
    let mut headers = HeaderCollection::new();

    // 添加多个 Via 头部
    headers.insert(
        HeaderName::Via,
        HeaderValue::Via(ViaHeader::new(
            TransportProtocol::Udp,
            Host::Domain("proxy1.example.com".to_string()),
            Some(5060),
        )),
    );
    headers.insert(
        HeaderName::Via,
        HeaderValue::Via(ViaHeader::new(
            TransportProtocol::Tcp,
            Host::Domain("proxy2.example.com".to_string()),
            None,
        )),
    );

    // get 返回第一个
    let first = headers.get(&HeaderName::Via);
    assert!(first.is_some());

    // get_all 返回所有
    let all = headers.get_all(&HeaderName::Via);
    assert_eq!(all.len(), 2);
}

#[test]
fn test_header_collection_empty() {
    let headers = HeaderCollection::new();
    assert!(headers.is_empty());
    assert_eq!(headers.len(), 0);
}

// ============================================================================
// 错误处理测试
// ============================================================================

#[test]
fn test_parse_error_invalid_start_line() {
    // 使用格式错误的起始行（缺少必要部分）
    let raw = b"INVITE sip:bob@example.com\r\n\r\n";
    let parser = MessageParser::default_parser();
    let result = parser.parse(raw);
    assert!(result.is_err());
}

#[test]
fn test_parse_error_invalid_version() {
    let raw = b"INVITE sip:bob@example.com SIP/1.0\r\n\r\n";
    let parser = MessageParser::default_parser();
    let result = parser.parse(raw);
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(matches!(e, ParseError::InvalidVersion { .. }));
    }
}

#[test]
fn test_parse_error_invalid_status_code() {
    let raw = b"SIP/2.0 99 Bad\r\n\r\n";
    let parser = MessageParser::default_parser();
    let result = parser.parse(raw);
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(matches!(e, ParseError::InvalidStatusCode { .. }));
    }
}

#[test]
fn test_parse_error_message_too_large() {
    let raw = b"INVITE sip:bob@example.com SIP/2.0\r\n\r\n";
    let parser = MessageParser::new(10); // 极小的最大消息大小
    let result = parser.parse(raw);
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(matches!(e, ParseError::MessageTooLarge { .. }));
    }
}

#[test]
fn test_parse_error_invalid_uri() {
    let result = SipUri::parse("not-a-valid-uri");
    assert!(result.is_err());
}

#[test]
fn test_build_error_missing_call_id() {
    let uri = SipUri::parse("sip:bob@example.com").unwrap();
    let mut headers = HeaderCollection::new();
    headers.insert(
        HeaderName::Via,
        HeaderValue::Via(ViaHeader::new(
            TransportProtocol::Udp,
            Host::Domain("192.168.1.1".to_string()),
            Some(5060),
        )),
    );
    headers.insert(
        HeaderName::CSeq,
        HeaderValue::CSeq(CSeqHeader::new(1, Method::Invite)),
    );

    let message = SipMessage::Request(SipRequest {
        request_line: RequestLine {
            method: Method::Invite,
            request_uri: uri,
            version: SipVersion,
        },
        headers,
        body: None,
    });

    let builder = MessageBuilder::with_validation(true);
    let result = builder.build(&message);
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(matches!(e, BuildError::MissingHeader { ref header } if header == "Call-ID"));
    }
}

#[test]
fn test_build_error_missing_via() {
    let uri = SipUri::parse("sip:bob@example.com").unwrap();
    let mut headers = HeaderCollection::new();
    headers.insert(
        HeaderName::CallId,
        HeaderValue::CallId(CallId("test@example.com".to_string())),
    );
    headers.insert(
        HeaderName::CSeq,
        HeaderValue::CSeq(CSeqHeader::new(1, Method::Invite)),
    );

    let message = SipMessage::Request(SipRequest {
        request_line: RequestLine {
            method: Method::Invite,
            request_uri: uri,
            version: SipVersion,
        },
        headers,
        body: None,
    });

    let builder = MessageBuilder::with_validation(true);
    let result = builder.build(&message);
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(matches!(e, BuildError::MissingHeader { ref header } if header == "Via"));
    }
}

// ============================================================================
// 配置构建测试
// ============================================================================

#[test]
fn test_config_builder_missing_aor() {
    let result = SipConfig::builder()
        .contact("sip:alice@192.168.1.1:5060")
        .build();
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(matches!(e, ConfigError::MissingField { ref field } if field == "aor"));
    }
}

#[test]
fn test_config_builder_missing_contact() {
    let result = SipConfig::builder().aor("sip:alice@example.com").build();
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(matches!(e, ConfigError::MissingField { ref field } if field == "contact"));
    }
}

#[test]
fn test_config_builder_success() {
    let config = SipConfig::builder()
        .aor("sip:alice@example.com")
        .contact("sip:alice@192.168.1.1:5060")
        .build()
        .unwrap();

    assert_eq!(config.aor, "sip:alice@example.com");
    assert_eq!(config.contact, "sip:alice@192.168.1.1:5060");
    assert!(config.outbound_proxy.is_none());
    assert!(config.registrar_server.is_none());
    assert_eq!(config.transport, TransportProtocol::Udp);
    assert!(config.credentials.is_none());
    assert_eq!(config.sip_port, 5060);
}

#[test]
fn test_config_builder_full() {
    let config = SipConfig::builder()
        .aor("sip:bob@example.com")
        .contact("sip:bob@10.0.0.1:5060")
        .outbound_proxy("sip:proxy.example.com:5060")
        .registrar_server("sip:reg.example.com:5060")
        .transport(TransportProtocol::Tcp)
        .credentials("bob", "secret123")
        .sip_port(5080)
        .build()
        .unwrap();

    assert_eq!(config.aor, "sip:bob@example.com");
    assert_eq!(config.contact, "sip:bob@10.0.0.1:5060");
    assert_eq!(
        config.outbound_proxy.as_deref(),
        Some("sip:proxy.example.com:5060")
    );
    assert_eq!(
        config.registrar_server.as_deref(),
        Some("sip:reg.example.com:5060")
    );
    assert_eq!(config.transport, TransportProtocol::Tcp);
    assert!(config.credentials.is_some());
    let creds = config.credentials.unwrap();
    assert_eq!(creds.username, "bob");
    assert_eq!(creds.password, "secret123");
    assert_eq!(config.sip_port, 5080);
}

#[test]
fn test_config_builder_default_sub_configs() {
    let config = SipConfig::builder()
        .aor("sip:alice@example.com")
        .contact("sip:alice@192.168.1.1:5060")
        .build()
        .unwrap();

    // 验证子配置使用默认值
    assert!(config.transport_config.udp_enabled);
    assert!(config.transport_config.tcp_enabled);
    assert_eq!(config.transaction_config.t1, 500);
    assert_eq!(config.transaction_config.t2, 4000);
    assert_eq!(config.registration_config.default_expires, 3600);
}

// ============================================================================
// 流式解析测试
// ============================================================================

#[test]
fn test_streaming_parse_complete() {
    let raw = b"INVITE sip:bob@example.com SIP/2.0\r\n\
                Via: SIP/2.0/UDP 192.168.1.1:5060;branch=z9hG4bK-abc123\r\n\
                Call-ID: test@example.com\r\n\
                CSeq: 1 INVITE\r\n\
                Content-Length: 0\r\n\
                \r\n";

    let parser = MessageParser::default_parser();
    let result = parser.parse_streaming(raw);
    assert!(result.is_ok());

    if let Ok(Some((message, consumed))) = result {
        assert!(message.is_request());
        assert_eq!(consumed, raw.len());
    } else {
        panic!("期望成功解析完整消息");
    }
}

#[test]
fn test_streaming_parse_incomplete() {
    let raw = b"INVITE sip:bob@example.com SIP/2.0\r\n\
                Via: SIP/2.0/UDP 192.168.1.1:5060;branch=z9hG4bK-abc123\r\n";

    let parser = MessageParser::default_parser();
    let result = parser.parse_streaming(raw);
    assert!(result.is_ok());

    // 数据不完整，应返回 None
    if let Ok(opt) = result {
        assert!(opt.is_none());
    }
}

// ============================================================================
// 方法解析测试
// ============================================================================

#[test]
fn test_method_display_and_parse() {
    // 标准方法
    assert_eq!(Method::Invite.to_string(), "INVITE");
    assert_eq!(Method::Register.to_string(), "REGISTER");
    assert_eq!(Method::Bye.to_string(), "BYE");
    assert_eq!(Method::Ack.to_string(), "ACK");
    assert_eq!(Method::Cancel.to_string(), "CANCEL");
    assert_eq!(Method::Options.to_string(), "OPTIONS");

    // 从字符串解析
    assert!(matches!("INVITE".parse::<Method>(), Ok(Method::Invite)));
    assert!(matches!("invite".parse::<Method>(), Ok(Method::Invite)));
    assert!(matches!("REGISTER".parse::<Method>(), Ok(Method::Register)));

    // 扩展方法
    let ext = "CUSTOM1".parse::<Method>().unwrap();
    assert!(ext.is_extension());
}

// ============================================================================
// BranchId 和 Tag 测试
// ============================================================================

#[test]
fn test_branch_id_validity() {
    let branch = BranchId::new();
    assert!(branch.is_valid());
    assert!(branch.0.starts_with("z9hG4bK-"));

    let invalid = BranchId("invalid-branch".to_string());
    assert!(!invalid.is_valid());
}

#[test]
fn test_tag_generation() {
    let tag1 = Tag::new();
    let tag2 = Tag::new();
    // 两个 Tag 应该不同
    assert_ne!(tag1.0, tag2.0);
}

#[test]
fn test_call_id_generation() {
    let call_id = CallId::new();
    assert!(call_id.0.contains('@'));
    assert!(call_id.0.ends_with("sip-rs"));

    let call_id_with_host = CallId::with_host("example.com");
    assert!(call_id_with_host.0.ends_with("example.com"));
}
