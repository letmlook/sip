//! 最简单的 SIP 呼叫示例
//!
//! 演示使用 SipEngine 完成最基本的 SIP 呼叫流程：
//! 1. 创建配置
//! 2. 启动引擎
//! 3. 发起呼叫
//! 4. 处理呼叫事件
//!
//! # 运行方式
//!
//! ```bash
//! cargo run -p siprs-ua --example simple_call
//! ```

use siprs_core::config::SipConfig;
use siprs_ua::engine::SipEngine;
use siprs_ua::event::SipEvent;

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::fmt::init();

    println!("=== 最简单的 SIP 呼叫示例 ===\n");

    // 1. 创建配置
    let config = SipConfig::builder()
        .aor("sip:alice@example.com")
        .contact("sip:alice@192.168.1.100:5060")
        .outbound_proxy("sip:proxy.example.com:5060")
        .build()
        .expect("配置构建失败");

    // 2. 创建并启动引擎
    let mut engine = SipEngine::new(config);

    match engine.start().await {
        Ok(()) => println!("[✓] 引擎启动成功"),
        Err(e) => {
            println!("[✗] 引擎启动失败: {}", e);
            println!("    提示：请确保网络可用且端口 5060 未被占用");
            return;
        }
    }

    // 3. 获取事件接收器
    let mut event_rx = engine.event_receiver().expect("事件接收器只能获取一次");

    // 4. 发起呼叫
    let target = "sip:bob@example.com";

    match engine.make_call(target, None, None).await {
        Ok(call_id) => println!("[✓] 呼叫已发起 (call_id={})", call_id),
        Err(e) => {
            println!("[✗] 呼叫发起失败: {}", e);
            engine.stop().await;
            return;
        }
    }

    // 5. 处理呼叫事件
    println!("[...] 等待呼叫事件...\n");

    loop {
        tokio::select! {
            Some(event) = event_rx.recv() => {
                match event {
                    SipEvent::CallProgress { call_id, status_code, reason_phrase } => {
                        println!("[进展] {} {} (call_id={})", status_code, reason_phrase, call_id);
                    }
                    SipEvent::CallEstablished { call_id, dialog_id, .. } => {
                        println!("[✓] 呼叫建立！call_id={}, dialog_id={}", call_id, dialog_id);

                        // 通话 5 秒后挂断
                        println!("[...] 通话中，5 秒后挂断...");
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

                        match engine.hang_up(&call_id).await {
                            Ok(()) => println!("[✓] 已挂断"),
                            Err(e) => println!("[✗] 挂断失败: {}", e),
                        }
                    }
                    SipEvent::IncomingCall { call_id, from, .. } => {
                        println!("[来电] call_id={}, from={}", call_id, from);

                        // 自动接听
                        match engine.answer_call(&call_id, None, None).await {
                            Ok(()) => println!("[✓] 已接听来电"),
                            Err(e) => println!("[✗] 接听失败: {}", e),
                        }
                    }
                    SipEvent::CallTerminated { call_id, reason } => {
                        println!("[结束] 呼叫终止: call_id={}, 原因={}", call_id, reason);
                        break;
                    }
                    SipEvent::Error(e) => {
                        println!("[✗] 错误: {}", e);
                    }
                    _ => {}
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(30)) => {
                println!("[!] 等待事件超时（30秒）");
                break;
            }
        }
    }

    // 6. 停止引擎
    engine.stop().await;
    println!("\n[✓] 引擎已停止");
}
