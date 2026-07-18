//! SIP 消息解析器性能基准测试
//!
//! 使用 criterion 基准测试框架，测量 MessageParser 在不同场景下的解析性能。

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

use siprs_message::parser::MessageParser;

/// 创建简单 INVITE 请求的原始字节
fn create_simple_invite_raw() -> Vec<u8> {
    let raw = b"INVITE sip:bob@example.com SIP/2.0\r\n\
                Via: SIP/2.0/UDP 192.168.1.1:5060;branch=z9hG4bK-abc123\r\n\
                From: <sip:alice@example.com>;tag=1928301774\r\n\
                To: <sip:bob@example.com>\r\n\
                Call-ID: a84b4c76e66710@pc33.example.com\r\n\
                CSeq: 314159 INVITE\r\n\
                Contact: <sip:alice@pc33.example.com>\r\n\
                Content-Length: 0\r\n\
                \r\n";
    raw.to_vec()
}

/// 创建带 SDP 的 INVITE 请求的原始字节
fn create_invite_with_sdp_raw() -> Vec<u8> {
    let sdp = "v=0\r\n\
               o=- 12345 1 IN IP4 192.168.1.1\r\n\
               s=Session\r\n\
               c=IN IP4 192.168.1.1\r\n\
               t=0 0\r\n\
               m=audio 5004 RTP/AVP 0 8 101\r\n\
               a=rtpmap:0 PCMU/8000\r\n\
               a=rtpmap:8 PCMA/8000\r\n\
               a=rtpmap:101 telephone-event/8000\r\n\
               a=fmtp:101 0-16\r\n\
               a=sendrecv\r\n";
    let raw = format!(
        "INVITE sip:bob@example.com SIP/2.0\r\n\
         Via: SIP/2.0/UDP 192.168.1.1:5060;branch=z9hG4bK-abc123\r\n\
         From: <sip:alice@example.com>;tag=1928301774\r\n\
         To: <sip:bob@example.com>\r\n\
         Call-ID: a84b4c76e66710@pc33.example.com\r\n\
         CSeq: 314159 INVITE\r\n\
         Contact: <sip:alice@pc33.example.com>\r\n\
         Content-Type: application/sdp\r\n\
         Content-Length: {}\r\n\
         \r\n{}",
        sdp.len(),
        sdp
    );
    raw.into_bytes()
}

/// 创建 200 OK 响应的原始字节
fn create_200_ok_raw() -> Vec<u8> {
    let raw = b"SIP/2.0 200 OK\r\n\
                Via: SIP/2.0/UDP 192.168.1.1:5060;branch=z9hG4bK-abc123\r\n\
                From: <sip:alice@example.com>;tag=1928301774\r\n\
                To: <sip:bob@example.com>;tag=9876\r\n\
                Call-ID: a84b4c76e66710@pc33.example.com\r\n\
                CSeq: 314159 INVITE\r\n\
                Contact: <sip:bob@pc34.example.com>\r\n\
                Content-Length: 0\r\n\
                \r\n";
    raw.to_vec()
}

/// 基准测试：解析简单 INVITE 请求
fn bench_parse_simple_invite(c: &mut Criterion) {
    let raw = create_simple_invite_raw();
    let parser = MessageParser::default_parser();

    c.bench_function("parse_simple_invite", |b| {
        b.iter(|| {
            let _ = parser.parse(black_box(&raw));
        })
    });
}

/// 基准测试：解析带 SDP 的 INVITE 请求
fn bench_parse_invite_with_sdp(c: &mut Criterion) {
    let raw = create_invite_with_sdp_raw();
    let parser = MessageParser::default_parser();

    c.bench_function("parse_invite_with_sdp", |b| {
        b.iter(|| {
            let _ = parser.parse(black_box(&raw));
        })
    });
}

/// 基准测试：解析 200 OK 响应
fn bench_parse_200_ok(c: &mut Criterion) {
    let raw = create_200_ok_raw();
    let parser = MessageParser::default_parser();

    c.bench_function("parse_200_ok", |b| {
        b.iter(|| {
            let _ = parser.parse(black_box(&raw));
        })
    });
}

/// 基准测试：不同消息大小的解析性能
fn bench_parse_varying_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_varying_sizes");
    let parser = MessageParser::default_parser();

    // 简单消息（无 body）
    let simple = create_simple_invite_raw();
    group.bench_with_input(BenchmarkId::new("size", simple.len()), &simple, |b, raw| {
        b.iter(|| {
            let _ = parser.parse(black_box(raw));
        })
    });

    // 带 SDP 的消息
    let with_sdp = create_invite_with_sdp_raw();
    group.bench_with_input(
        BenchmarkId::new("size", with_sdp.len()),
        &with_sdp,
        |b, raw| {
            b.iter(|| {
                let _ = parser.parse(black_box(raw));
            })
        },
    );

    group.finish();
}

criterion_group!(
    benches,
    bench_parse_simple_invite,
    bench_parse_invite_with_sdp,
    bench_parse_200_ok,
    bench_parse_varying_sizes,
);
criterion_main!(benches);
