//! 最简单的 SIP 注册示例
//!
//! 演示使用 SipEngine 完成最基本的 SIP 注册流程：
//! 1. 创建配置
//! 2. 启动引擎
//! 3. 发起注册
//! 4. 等待结果
//!
//! # 运行方式
//!
//! ```bash
//! cargo run -p siprs-ua --example simple_register
//! ```

use siprs_core::config::SipConfig;
use siprs_ua::engine::SipEngine;
use siprs_ua::event::SipEvent;

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::fmt::init();

    println!("=== 最简单的 SIP 注册示例 ===\n");

    // 1. 创建配置（必填项：AOR 和 Contact）
    let config = SipConfig::builder()
        .aor("sip:alice@example.com")
        .contact("sip:alice@192.168.1.100:5060")
        .registrar_server("sip:reg.example.com:5060")
        .credentials("alice", "password123")
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

    // 4. 发起注册
    match engine.register().await {
        Ok(reg_id) => println!("[✓] 注册请求已发送 (reg_id={})", reg_id),
        Err(e) => {
            println!("[✗] 注册请求发送失败: {}", e);
            engine.stop().await;
            return;
        }
    }

    // 5. 等待注册结果
    println!("[...] 等待注册结果...\n");

    tokio::select! {
        Some(event) = event_rx.recv() => {
            match event {
                SipEvent::RegistrationResult { registration_id, result } => {
                    match result {
                        Ok(()) => println!("[✓] 注册成功！reg_id={}", registration_id),
                        Err(e) => println!("[✗] 注册失败: {}", e),
                    }
                }
                SipEvent::Error(e) => println!("[✗] 错误: {}", e),
                other => println!("[?] 其他事件: {:?}", other),
            }
        }
        _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
            println!("[!] 等待注册结果超时（10秒）");
        }
    }

    // 6. 停止引擎
    engine.stop().await;
    println!("\n[✓] 引擎已停止");
}
