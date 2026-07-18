//! 基本 SIP 呼叫流程示例
//!
//! 演示如何使用 SipEngine 完成一次完整的 SIP 呼叫流程：
//! 1. 创建 SipEngine
//! 2. 注册
//! 3. 发起呼叫
//! 4. 接听
//! 5. 挂断
//!
//! # 运行说明
//!
//! 此示例需要网络连接到 SIP 服务器。在没有 SIP 服务器的环境下，
//! 代码可以编译但无法完成实际呼叫。示例中使用注释说明了预期行为。

use siprs_core::config::SipConfig;
use siprs_ua::engine::SipEngine;
use siprs_ua::event::SipEvent;

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::fmt::init();

    println!("=== SIP 基本呼叫流程示例 ===\n");

    // ========================================================================
    // 1. 创建 SipEngine
    // ========================================================================

    let config = SipConfig::builder()
        .aor("sip:alice@example.com")
        .contact("sip:alice@192.168.1.100:5060")
        .outbound_proxy("sip:proxy.example.com:5060")
        .registrar_server("sip:reg.example.com:5060")
        .credentials("alice", "password123")
        .build()
        .expect("配置构建失败");

    let mut engine = SipEngine::new(config);
    println!("[1] SipEngine 已创建");

    // ========================================================================
    // 2. 启动引擎
    // ========================================================================

    // 注意：start() 需要绑定网络端口，在没有网络环境时会失败
    match engine.start().await {
        Ok(()) => println!("[2] SipEngine 已启动"),
        Err(e) => {
            println!("[2] SipEngine 启动失败（需要网络环境）: {}", e);
            println!("    在实际部署中，请确保网络可用且端口未被占用");
            return;
        }
    }

    // 获取事件接收器
    let mut event_rx = engine.event_receiver().expect("事件接收器只能获取一次");

    // ========================================================================
    // 3. 注册
    // ========================================================================

    // 注意：register() 需要连接到注册服务器
    match engine.register().await {
        Ok(reg_id) => println!("[3] 注册成功，reg_id={}", reg_id),
        Err(e) => {
            println!("[3] 注册失败（需要 SIP 注册服务器）: {}", e);
            // 在实际部署中，注册失败后应重试或通知用户
        }
    }

    // ========================================================================
    // 4. 发起呼叫
    // ========================================================================

    // 注意：make_call() 需要连接到目标用户或代理服务器
    let target = "sip:bob@example.com";

    // 可选的 SDP 会话描述
    let sdp = b"v=0\r\no=- 12345 1 IN IP4 192.168.1.100\r\ns=Call\r\nc=IN IP4 192.168.1.100\r\nt=0 0\r\nm=audio 5004 RTP/AVP 0\r\na=rtpmap:0 PCMU/8000\r\n".to_vec();

    match engine
        .make_call(target, Some(sdp), Some("application/sdp"))
        .await
    {
        Ok(call_id) => println!("[4] 呼叫已发起，call_id={}", call_id),
        Err(e) => {
            println!("[4] 呼叫发起失败（需要 SIP 服务器）: {}", e);
        }
    }

    // ========================================================================
    // 5. 处理事件
    // ========================================================================

    println!("[5] 等待 SIP 事件...");

    // 在实际应用中，这里应该是一个事件循环
    // 以下展示了如何处理各种 SipEvent
    loop {
        // 注意：event_rx.recv().await 需要实际的网络事件
        // 在没有网络连接时，此调用会一直阻塞
        // 实际部署时请使用 tokio::select! 处理多个异步事件源
        tokio::select! {
            Some(event) = event_rx.recv() => {
                match event {
                    SipEvent::IncomingCall { call_id, from, to, body, content_type } => {
                        println!("[事件] 来电: call_id={}, from={}, to={}", call_id, from, to);
                        if body.is_some() {
                            println!("       包含会话描述，类型: {:?}", content_type);
                        }

                        // 接听来电
                        let answer_sdp = b"v=0\r\no=- 67890 1 IN IP4 192.168.1.100\r\ns=Answer\r\nc=IN IP4 192.168.1.100\r\nt=0 0\r\nm=audio 5006 RTP/AVP 0\r\na=rtpmap:0 PCMU/8000\r\n".to_vec();
                        match engine.answer_call(&call_id, Some(answer_sdp), Some("application/sdp")).await {
                            Ok(()) => println!("[事件] 已接听来电"),
                            Err(e) => println!("[事件] 接听失败: {}", e),
                        }
                    }
                    SipEvent::CallProgress { call_id, status_code, reason_phrase } => {
                        println!("[事件] 呼叫进展: call_id={}, {} {}", call_id, status_code, reason_phrase);
                    }
                    SipEvent::CallEstablished { call_id, dialog_id, .. } => {
                        println!("[事件] 呼叫建立: call_id={}, dialog_id={}", call_id, dialog_id);

                        // 模拟通话一段时间后挂断
                        println!("[事件] 通话中... 模拟 5 秒后挂断");
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

                        match engine.hang_up(&call_id).await {
                            Ok(()) => println!("[事件] 已挂断"),
                            Err(e) => println!("[事件] 挂断失败: {}", e),
                        }
                    }
                    SipEvent::CallTerminated { call_id, reason } => {
                        println!("[事件] 呼叫终止: call_id={}, 原因={}", call_id, reason);
                        break;
                    }
                    SipEvent::RegistrationResult { registration_id, result } => {
                        match result {
                            Ok(()) => println!("[事件] 注册成功: reg_id={}", registration_id),
                            Err(e) => println!("[事件] 注册失败: reg_id={}, 原因={}", registration_id, e),
                        }
                    }
                    SipEvent::Error(e) => {
                        println!("[事件] 错误: {}", e);
                    }
                    SipEvent::DialogEvent(e) => {
                        println!("[事件] 对话事件: {:?}", e);
                    }
                    SipEvent::RegistrationEvent(e) => {
                        println!("[事件] 注册事件: {:?}", e);
                    }
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(30)) => {
                println!("[超时] 30 秒内未收到事件，退出示例");
                break;
            }
        }
    }

    // ========================================================================
    // 6. 停止引擎
    // ========================================================================

    engine.stop().await;
    println!("[6] SipEngine 已停止");
}
