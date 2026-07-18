//! SIP 注册流程示例
//!
//! 演示如何使用 SipEngine 完成 SIP 注册/注销流程：
//! 1. 创建 SipEngine
//! 2. 发起注册
//! 3. 处理认证挑战
//! 4. 注销
//!
//! # 运行说明
//!
//! 此示例需要网络连接到 SIP 注册服务器。在没有 SIP 服务器的环境下，
//! 代码可以编译但无法完成实际注册。示例中使用注释说明了预期行为。

use siprs_core::config::{RegistrationConfig, SipConfig};
use siprs_ua::engine::SipEngine;
use siprs_ua::event::SipEvent;

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::fmt::init();

    println!("=== SIP 注册流程示例 ===\n");

    // ========================================================================
    // 1. 创建 SipEngine
    // ========================================================================

    let config = SipConfig::builder()
        .aor("sip:alice@example.com")
        .contact("sip:alice@192.168.1.100:5060")
        .registrar_server("sip:reg.example.com:5060")
        .credentials("alice", "password123")
        .registration_config(RegistrationConfig {
            default_expires: 3600,  // 注册有效期 1 小时
            refresh_threshold: 0.5, // 剩余 50% 时自动刷新
            retry_interval: 30,     // 失败重试间隔 30 秒
            max_retries: 3,         // 最大重试 3 次
            ..RegistrationConfig::default()
        })
        .build()
        .expect("配置构建失败");

    let mut engine = SipEngine::new(config);
    println!("[1] SipEngine 已创建");

    // ========================================================================
    // 2. 启动引擎
    // ========================================================================

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
    // 3. 发起注册
    // ========================================================================

    // 注意：register() 需要连接到注册服务器
    // 如果服务器要求认证，SipEngine 会自动处理 401/407 挑战
    // 前提是配置中提供了 credentials
    match engine.register().await {
        Ok(reg_id) => {
            println!("[3] 注册请求已发送，reg_id={}", reg_id);

            // 等待注册结果
            println!("[3] 等待注册结果...");

            // 处理认证挑战的预期流程：
            // 1. 客户端发送 REGISTER（无 Authorization 头部）
            // 2. 服务器返回 401 Unauthorized（含 WWW-Authenticate 头部）
            // 3. 客户端使用凭据计算摘要认证响应
            // 4. 客户端重新发送 REGISTER（含 Authorization 头部）
            // 5. 服务器返回 200 OK
            //
            // SipEngine 内部会自动处理步骤 2-4，
            // 最终通过 RegistrationResult 事件通知结果
        }
        Err(e) => {
            println!("[3] 注册请求发送失败（需要 SIP 注册服务器）: {}", e);
        }
    }

    // ========================================================================
    // 4. 处理注册结果事件
    // ========================================================================

    println!("[4] 等待 SIP 事件...");

    let mut registration_id: Option<String> = None;

    loop {
        tokio::select! {
            Some(event) = event_rx.recv() => {
                match event {
                    SipEvent::RegistrationResult { registration_id: reg_id, result } => {
                        match result {
                            Ok(()) => {
                                println!("[4] 注册成功！reg_id={}", reg_id);
                                registration_id = Some(reg_id);
                            }
                            Err(e) => {
                                println!("[4] 注册失败: {}", e);
                                println!("    可能的原因：");
                                println!("    - 用户名或密码错误");
                                println!("    - 注册服务器不可达");
                                println!("    - 认证挑战处理失败");
                            }
                        }
                        break;
                    }
                    SipEvent::Error(e) => {
                        println!("[4] 错误: {}", e);
                    }
                    _ => {
                        // 忽略其他事件
                    }
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
                println!("[4] 等待注册结果超时（10秒）");
                break;
            }
        }
    }

    // ========================================================================
    // 5. 注销
    // ========================================================================

    if let Some(reg_id) = registration_id {
        println!("[5] 发起注销...");

        match engine.unregister(&reg_id).await {
            Ok(()) => println!("[5] 注销请求已发送"),
            Err(e) => println!("[5] 注销失败: {}", e),
        }

        // 等待注销结果
        tokio::select! {
            Some(event) = event_rx.recv() => {
                if let SipEvent::RegistrationResult { result, .. } = event {
                    match result {
                        Ok(()) => println!("[5] 注销成功"),
                        Err(e) => println!("[5] 注销失败: {}", e),
                    }
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {
                println!("[5] 等待注销结果超时");
            }
        }
    } else {
        println!("[5] 跳过注销（注册未成功）");
    }

    // ========================================================================
    // 6. 停止引擎
    // ========================================================================

    engine.stop().await;
    println!("[6] SipEngine 已停止");
}
