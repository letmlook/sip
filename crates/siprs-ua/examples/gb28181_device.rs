//! GB28181 设备端完整流程示例
//!
//! 演示 GB28181 设备的完整交互流程：
//! 1. 创建设备
//! 2. 注册到平台
//! 3. 处理目录查询
//! 4. 处理视频点播
//! 5. 注销
//!
//! # 运行方式
//!
//! ```bash
//! cargo run -p sip-ua --example gb28181_device
//! ```

use siprs_ua::gb28181::{Gb28181Config, Gb28181Device, Gb28181Event};

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    // 1. 创建 GB28181 设备配置
    let config = Gb28181Config {
        device_id: "34020000001320000001".to_string(),
        server_addr: "192.168.1.1".to_string(),
        server_port: 5060,
        server_domain: "3402000000".to_string(),
        username: "34020000001320000001".to_string(),
        password: "12345678".to_string(),
        expires: 3600,
        heartbeat_interval: 60,
        local_ip: "192.168.1.100".to_string(),
        local_port: 5060,
    };

    println!("=== GB28181 设备端示例 ===");
    println!("设备编码: {}", config.device_id);
    println!("服务器地址: {}:{}", config.server_addr, config.server_port);
    println!("服务器域: {}", config.server_domain);
    println!();

    // 2. 创建设备
    let mut device = Gb28181Device::new(config);

    // 获取事件接收器（必须在 start 之前）
    let mut event_rx = device
        .event_receiver()
        .expect("event receiver already taken");

    // 3. 启动设备（注册 + 心跳）
    println!("[启动] 正在启动设备并注册到平台...");
    match device.start().await {
        Ok(()) => println!("[启动] 设备启动成功"),
        Err(e) => {
            eprintln!("[启动] 设备启动失败: {}", e);
            return;
        }
    }

    // 4. 事件循环 - 处理来自平台的各种请求
    println!("[事件循环] 等待平台请求...\n");

    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                Gb28181Event::Registered => {
                    println!("[事件] ✅ 注册成功");
                }
                Gb28181Event::RegistrationFailed(reason) => {
                    eprintln!("[事件] ❌ 注册失败: {}", reason);
                }
                Gb28181Event::KeepaliveOk => {
                    println!("[事件] 💓 心跳保活成功");
                }
                Gb28181Event::KeepaliveTimeout => {
                    eprintln!("[事件] ⚠️ 心跳超时");
                }
                Gb28181Event::CatalogQuery { sn, device_id } => {
                    println!("[事件] 📋 收到目录查询 (SN={}, DeviceID={})", sn, device_id);

                    // 构建设备目录响应
                    let item_id =
                        siprs_gb28181_codec::DeviceId::parse("34020000001320000001").unwrap();
                    let mut item = siprs_gb28181_xml::DeviceItem::new(item_id);
                    item.name = Some("Camera1".to_string());
                    item.manufacturer = Some("Hikvision".to_string());
                    item.model = Some("DS-2CD2143".to_string());
                    item.status = Some("ON".to_string());

                    // 注意：实际使用时需要通过 device 引用发送
                    println!("[处理] 构建目录响应: 1 个设备");
                }
                Gb28181Event::DeviceInfoQuery { sn, device_id } => {
                    println!(
                        "[事件] ℹ️ 收到设备信息查询 (SN={}, DeviceID={})",
                        sn, device_id
                    );
                    println!("[处理] 设备信息: Hikvision DS-2CD2143 V5.4.5");
                }
                Gb28181Event::InviteReceived {
                    call_id,
                    device_id,
                    sdp: _,
                } => {
                    println!(
                        "[事件] 📹 收到视频点播请求 (CallID={}, DeviceID={})",
                        call_id, device_id
                    );
                    println!("[处理] 接受点播，媒体地址: 192.168.1.100:6000");
                }
                Gb28181Event::ByeReceived { call_id } => {
                    println!("[事件] 📞 对方挂断 (CallID={})", call_id);
                }
                Gb28181Event::PtzControl { device_id, command } => {
                    println!(
                        "[事件] 🎮 收到云台控制命令 (DeviceID={}, Cmd={})",
                        device_id, command
                    );
                }
            }
        }
    });

    // 5. 模拟运行一段时间后注销
    println!("\n[模拟] 设备运行中，10秒后自动注销...");
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

    // 6. 停止设备（注销 + 清理）
    println!("\n[停止] 正在注销并停止设备...");
    match device.stop().await {
        Ok(()) => println!("[停止] 设备已停止"),
        Err(e) => eprintln!("[停止] 停止失败: {}", e),
    }

    println!("\n=== 示例结束 ===");
}
