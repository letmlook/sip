//! SIP 消息构建器性能基准测试
//!
//! 使用 criterion 基准测试框架，测量 MessageBuilder 在不同场景下的构建性能。

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use sip_core::{Host, SipVersion, StatusCode, TransportProtocol};
use sip_message::builder::MessageBuilder;
use sip_message::headers::{CSeqHeader, HeaderCollection, HeaderName, HeaderValue, ViaHeader};
use sip_message::types::{
    Body, CallId, Method, RequestLine, SipMessage, SipRequest, SipResponse, StatusLine, Tag,
};
use sip_message::uri::SipUri;

/// 创建 INVITE 请求消息
fn create_invite_request() -> SipMessage {
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
            tag: Some(Tag("1928301774".to_string())),
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
    headers.insert(
        HeaderName::CallId,
        HeaderValue::CallId(CallId("a84b4c76e66710@pc33.example.com".to_string())),
    );
    headers.insert(
        HeaderName::CSeq,
        HeaderValue::CSeq(CSeqHeader::new(314159, Method::Invite)),
    );
    headers.insert(
        HeaderName::Contact,
        HeaderValue::Contact(sip_message::headers::ContactHeader {
            display_name: None,
            uri: SipUri::parse("sip:alice@pc33.example.com").unwrap(),
            expires: None,
        }),
    );
    headers.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

    SipMessage::Request(SipRequest {
        request_line: RequestLine {
            method: Method::Invite,
            request_uri: uri,
            version: SipVersion,
        },
        headers,
        body: None,
    })
}

/// 创建带 SDP 的 INVITE 请求消息
fn create_invite_request_with_sdp() -> SipMessage {
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
            tag: Some(Tag("1928301774".to_string())),
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
    headers.insert(
        HeaderName::CallId,
        HeaderValue::CallId(CallId("a84b4c76e66710@pc33.example.com".to_string())),
    );
    headers.insert(
        HeaderName::CSeq,
        HeaderValue::CSeq(CSeqHeader::new(314159, Method::Invite)),
    );
    headers.insert(
        HeaderName::Contact,
        HeaderValue::Contact(sip_message::headers::ContactHeader {
            display_name: None,
            uri: SipUri::parse("sip:alice@pc33.example.com").unwrap(),
            expires: None,
        }),
    );
    headers.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

    let sdp_body = b"v=0\r\n\
                     o=- 12345 1 IN IP4 192.168.1.1\r\n\
                     s=Session\r\n\
                     c=IN IP4 192.168.1.1\r\n\
                     t=0 0\r\n\
                     m=audio 5004 RTP/AVP 0 8 101\r\n\
                     a=rtpmap:0 PCMU/8000\r\n\
                     a=rtpmap:8 PCMA/8000\r\n\
                     a=rtpmap:101 telephone-event/8000\r\n\
                     a=fmtp:101 0-16\r\n\
                     a=sendrecv\r\n"
        .to_vec();

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

/// 创建 200 OK 响应消息
fn create_200_ok_response() -> SipMessage {
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
            tag: Some(Tag("1928301774".to_string())),
        }),
    );
    headers.insert(
        HeaderName::To,
        HeaderValue::FromTo(sip_message::headers::FromToHeader {
            display_name: None,
            uri: SipUri::parse("sip:bob@example.com").unwrap(),
            tag: Some(Tag("9876".to_string())),
        }),
    );
    headers.insert(
        HeaderName::CallId,
        HeaderValue::CallId(CallId("a84b4c76e66710@pc33.example.com".to_string())),
    );
    headers.insert(
        HeaderName::CSeq,
        HeaderValue::CSeq(CSeqHeader::new(314159, Method::Invite)),
    );
    headers.insert(
        HeaderName::Contact,
        HeaderValue::Contact(sip_message::headers::ContactHeader {
            display_name: None,
            uri: SipUri::parse("sip:bob@pc34.example.com").unwrap(),
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

/// 基准测试：构建 INVITE 请求
fn bench_build_invite(c: &mut Criterion) {
    let message = create_invite_request();
    let builder = MessageBuilder::new();

    c.bench_function("build_invite", |b| {
        b.iter(|| {
            let _ = builder.build(black_box(&message));
        })
    });
}

/// 基准测试：构建带 SDP 的 INVITE 请求
fn bench_build_invite_with_sdp(c: &mut Criterion) {
    let message = create_invite_request_with_sdp();
    let builder = MessageBuilder::new();

    c.bench_function("build_invite_with_sdp", |b| {
        b.iter(|| {
            let _ = builder.build(black_box(&message));
        })
    });
}

/// 基准测试：构建 200 OK 响应
fn bench_build_200_ok(c: &mut Criterion) {
    let message = create_200_ok_response();
    let builder = MessageBuilder::new();

    c.bench_function("build_200_ok", |b| {
        b.iter(|| {
            let _ = builder.build(black_box(&message));
        })
    });
}

/// 基准测试：带校验的构建
fn bench_build_with_validation(c: &mut Criterion) {
    let message = create_invite_request();
    let builder = MessageBuilder::with_validation(true);

    c.bench_function("build_invite_with_validation", |b| {
        b.iter(|| {
            let _ = builder.build(black_box(&message));
        })
    });
}

criterion_group!(
    benches,
    bench_build_invite,
    bench_build_invite_with_sdp,
    bench_build_200_ok,
    bench_build_with_validation,
);
criterion_main!(benches);
