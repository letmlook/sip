//! GB28181 平台端完整流程示例
//!
//! 演示 GB28181 平台端（WVP 信令服务器）的完整交互流程：
//! 1. 创建平台端
//! 2. 启动监听
//! 3. 等待设备注册
//! 4. 查询设备目录
//! 5. 发起视频点播
//! 6. 云台控制
//! 7. 停止
//!
//! # 运行方式
//!
//! ```bash
//! cargo run -p sip-ua --example gb28181_server
//! ```

use sip_ua::gb28181_server::{Gb28181Server, Gb28181ServerConfig, Gb28181ServerEvent};

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    // 1. 创建 GB28181 平台端配置
    let config = Gb28181ServerConfig {
        server_id: "34020000002000000001".to_string(),
        server_domain: "3402000000".to_string(),
        sip_ip: "192.168.1.1".to_string(),
        sip_port: 5060,
        realm: "3402000000".to_string(),
        auth_password: "12345678".to_string(),
        heartbeat_timeout: 180,
        register_expires: 3600,
    };

    println!("=== GB28181 平台端示例 ===");
    println!("平台编码: {}", config.server_id);
    println!("监听地址: {}:{}", config.sip_ip, config.sip_port);
    println!("认证域: {}", config.realm);
    println!();

    // 2. 创建平台端
    let mut server = Gb28181Server::new(config);

    // 获取事件接收器（必须在 start 之前）
    let mut event_rx = server
        .event_receiver()
        .expect("event receiver already taken");

    // 3. 启动平台端
    println!("[启动] 正在启动平台端...");
    match server.start().await {
        Ok(()) => println!("[启动] 平台端启动成功，等待设备注册..."),
        Err(e) => {
            eprintln!("[启动] 平台端启动失败: {}", e);
            return;
        }
    }

    // 4. 事件循环 - 处理来自设备的各种消息
    let event_task = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                Gb28181ServerEvent::DeviceRegistered { device_id } => {
                    println!("[事件] ✅ 设备注册成功: {}", device_id);
                }
                Gb28181ServerEvent::DeviceUnregistered { device_id } => {
                    println!("[事件] ❌ 设备注销: {}", device_id);
                }
                Gb28181ServerEvent::DeviceOnline { device_id } => {
                    println!("[事件] 🟢 设备上线: {}", device_id);
                }
                Gb28181ServerEvent::DeviceOffline { device_id } => {
                    println!("[事件] 🔴 设备离线: {}", device_id);
                }
                Gb28181ServerEvent::KeepaliveReceived { device_id } => {
                    println!("[事件] 💓 收到心跳: {}", device_id);
                }
                Gb28181ServerEvent::CatalogResponse { device_id, devices } => {
                    println!(
                        "[事件] 📋 收到目录响应: {} ({} 个子设备)",
                        device_id,
                        devices.len()
                    );
                    for item in &devices {
                        println!(
                            "  - {} ({})",
                            item.device_id,
                            item.name.as_deref().unwrap_or("未知")
                        );
                    }
                }
                Gb28181ServerEvent::DeviceInfoResponse {
                    device_id,
                    name,
                    manufacturer,
                    model,
                } => {
                    println!(
                        "[事件] ℹ️ 收到设备信息: {} ({} {} {})",
                        device_id, manufacturer, model, name
                    );
                }
                Gb28181ServerEvent::RecordQueryResponse { device_id, records } => {
                    println!(
                        "[事件] 🎬 收到录像查询响应: {} ({} 条录像)",
                        device_id,
                        records.len()
                    );
                }
                Gb28181ServerEvent::DeviceStatusResponse {
                    device_id,
                    online,
                    status,
                } => {
                    println!(
                        "[事件] 📊 收到设备状态: {} (online={}, status={})",
                        device_id, online, status
                    );
                }
                Gb28181ServerEvent::AlarmReceived { device_id, alarms } => {
                    println!(
                        "[事件] 🚨 收到报警: {} ({} 条报警)",
                        device_id,
                        alarms.len()
                    );
                }
                Gb28181ServerEvent::MobilePositionReceived {
                    device_id,
                    positions,
                } => {
                    println!(
                        "[事件] 📍 收到移动位置: {} ({} 条位置)",
                        device_id,
                        positions.len()
                    );
                }
                Gb28181ServerEvent::InviteSent { device_id, call_id } => {
                    println!("[事件] 📹 发送 INVITE: {} (call_id={})", device_id, call_id);
                }
                Gb28181ServerEvent::InviteOk {
                    device_id,
                    call_id,
                    sdp,
                } => {
                    println!(
                        "[事件] ✅ INVITE 200 OK: {} (call_id={}, sdp_len={})",
                        device_id,
                        call_id,
                        sdp.len()
                    );
                }
                Gb28181ServerEvent::ByeReceived { device_id, call_id } => {
                    println!("[事件] 📞 收到 BYE: {} (call_id={})", device_id, call_id);
                }
                Gb28181ServerEvent::Error(e) => {
                    eprintln!("[事件] ❌ 错误: {}", e);
                }
            }
        }
    });

    // 5. 模拟运行一段时间后执行主动操作
    println!("\n[模拟] 平台端运行中，等待设备注册...");
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // 查询录像
    println!("\n[操作] 查询录像...");
    if let Err(e) = server
        .query_record(
            "34020000001320000001",
            "2024-01-01T00:00:00",
            "2024-01-31T23:59:59",
        )
        .await
    {
        eprintln!("[操作] 查询录像失败: {}", e);
    }

    // 查询设备状态
    println!("[操作] 查询设备状态...");
    if let Err(e) = server.query_device_status("34020000001320000001").await {
        eprintln!("[操作] 查询设备状态失败: {}", e);
    }

    // 发起实时视频点播
    println!("[操作] 发起实时视频点播...");
    match server.invite_live("34020000001320000001").await {
        Ok(call_id) => {
            println!("[操作] INVITE 已发送 (call_id={})", call_id);

            // 模拟一段时间后挂断
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            println!("[操作] 挂断通话...");
            if let Err(e) = server.hang_up(&call_id).await {
                eprintln!("[操作] 挂断失败: {}", e);
            }
        }
        Err(e) => eprintln!("[操作] 发起 INVITE 失败: {}", e),
    }

    // 云台控制
    println!("[操作] 云台控制 - 向上...");
    if let Err(e) = server
        .ptz_control("34020000001320000001", gb28181_xml::PtzDirection::Up, 31)
        .await
    {
        eprintln!("[操作] 云台控制失败: {}", e);
    }

    // 远程启动
    println!("[操作] 远程启动...");
    if let Err(e) = server.remote_start("34020000001320000001").await {
        eprintln!("[操作] 远程启动失败: {}", e);
    }

    // 订阅目录变更
    println!("[操作] 订阅目录变更...");
    match server.subscribe_catalog("34020000001320000001").await {
        Ok(sub_id) => {
            println!("[操作] 订阅成功 (sub_id={})", sub_id);

            // 模拟一段时间后取消订阅
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            println!("[操作] 取消订阅...");
            if let Err(e) = server.unsubscribe(&sub_id).await {
                eprintln!("[操作] 取消订阅失败: {}", e);
            }
        }
        Err(e) => eprintln!("[操作] 订阅失败: {}", e),
    }

    // 列出所有设备
    let devices = server.list_devices().await;
    println!("\n[查询] 当前已注册设备: {} 台", devices.len());
    for device in &devices {
        println!("  - {} (status={:?})", device.device_id, device.status);
    }

    // 获取设备树
    let tree = server.get_device_tree().await;
    println!("[查询] 设备树根节点: {}", tree.root.device_id);

    // 6. 模拟运行一段时间后停止
    println!("\n[模拟] 继续运行 5 秒后停止...");
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // 停止平台端
    println!("\n[停止] 正在停止平台端...");
    match server.stop().await {
        Ok(()) => println!("[停止] 平台端已停止"),
        Err(e) => eprintln!("[停止] 停止失败: {}", e),
    }

    // 终止事件循环
    event_task.abort();

    println!("\n=== 示例结束 ===");
}
