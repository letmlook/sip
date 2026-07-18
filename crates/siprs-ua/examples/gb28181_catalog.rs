//! GB28181 目录查询示例
//!
//! 演示 GB28181 平台端查询设备目录的完整流程：
//! 1. 创建平台端配置
//! 2. 启动平台端
//! 3. 查询设备目录
//! 4. 处理目录响应
//!
//! 同时演示设备端如何响应目录查询：
//! - 构建设备目录 XML
//! - 发送 MESSAGE 响应
//!
//! # 运行方式
//!
//! ```bash
//! cargo run -p siprs-ua --example gb28181_catalog
//! ```

use siprs_ua::gb28181_server::{Gb28181Server, Gb28181ServerConfig, Gb28181ServerEvent};

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    println!("=== GB28181 目录查询示例 ===\n");

    // ========================================================================
    // 1. 创建平台端配置
    // ========================================================================

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

    println!("平台编码: {}", config.server_id);
    println!("监听地址: {}:{}", config.sip_ip, config.sip_port);
    println!();

    // ========================================================================
    // 2. 创建并启动平台端
    // ========================================================================

    let mut server = Gb28181Server::new(config);

    // 获取事件接收器（必须在 start 之前）
    let mut event_rx = server
        .event_receiver()
        .expect("event receiver already taken");

    match server.start().await {
        Ok(()) => println!("[✓] 平台端启动成功"),
        Err(e) => {
            println!("[✗] 平台端启动失败: {}", e);
            return;
        }
    }

    // ========================================================================
    // 3. 启动事件处理任务
    // ========================================================================

    let event_task = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                Gb28181ServerEvent::DeviceRegistered { device_id } => {
                    println!("[事件] ✅ 设备注册: {}", device_id);
                }
                Gb28181ServerEvent::DeviceOnline { device_id } => {
                    println!("[事件] 🟢 设备上线: {}", device_id);
                }
                Gb28181ServerEvent::DeviceOffline { device_id } => {
                    println!("[事件] 🔴 设备离线: {}", device_id);
                }
                Gb28181ServerEvent::KeepaliveReceived { device_id } => {
                    println!("[事件] 💓 心跳: {}", device_id);
                }
                Gb28181ServerEvent::CatalogResponse { device_id, devices } => {
                    println!(
                        "[事件] 📋 目录响应: {} ({} 个子设备)",
                        device_id,
                        devices.len()
                    );
                    for item in &devices {
                        println!(
                            "  ├─ {} ({}) 状态={}",
                            item.device_id,
                            item.name.as_deref().unwrap_or("未知"),
                            item.status.as_deref().unwrap_or("未知")
                        );
                        if let Some(manufacturer) = &item.manufacturer {
                            println!("  │  厂商: {}", manufacturer);
                        }
                        if let Some(model) = &item.model {
                            println!("  │  型号: {}", model);
                        }
                    }
                }
                Gb28181ServerEvent::Error(e) => {
                    println!("[事件] ❌ 错误: {}", e);
                }
                _ => {}
            }
        }
    });

    // ========================================================================
    // 4. 查询设备目录
    // ========================================================================

    // 等待设备注册
    println!("[...] 等待设备注册（5秒）...\n");
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    let device_id = "34020000001320000001";

    println!("[操作] 查询设备目录: {}", device_id);
    match server.query_catalog(device_id).await {
        Ok(()) => println!("[✓] 目录查询已发送"),
        Err(e) => println!("[✗] 目录查询失败: {}", e),
    }

    // 等待目录响应
    println!("[...] 等待目录响应（5秒）...\n");
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // ========================================================================
    // 5. 演示：直接构建和解析目录 XML
    // ========================================================================

    println!("=== 目录 XML 构建与解析演示 ===\n");

    // 构建目录查询 XML
    let query_device_id = siprs_gb28181_codec::DeviceId::parse("34020000002000000001").unwrap();
    let query = siprs_gb28181_xml::Query::catalog(1, query_device_id.clone());
    let query_xml = query.to_xml();
    println!("[构建] 目录查询 XML:");
    println!("{}\n", query_xml);

    // 构建目录响应 XML
    let item_id = siprs_gb28181_codec::DeviceId::parse("34020000001320000001").unwrap();
    let mut item1 = siprs_gb28181_xml::DeviceItem::new(item_id);
    item1.name = Some("Camera1".to_string());
    item1.manufacturer = Some("Hikvision".to_string());
    item1.model = Some("DS-2CD2143".to_string());
    item1.status = Some("ON".to_string());

    let item_id2 = siprs_gb28181_codec::DeviceId::parse("34020000001320000002").unwrap();
    let mut item2 = siprs_gb28181_xml::DeviceItem::new(item_id2);
    item2.name = Some("Camera2".to_string());
    item2.manufacturer = Some("Dahua".to_string());
    item2.model = Some("DH-IPC-HFW2431".to_string());
    item2.status = Some("ON".to_string());

    let response = siprs_gb28181_xml::Response::catalog(1, query_device_id, 2, vec![item1, item2]);
    let response_xml = response.to_xml();
    println!("[构建] 目录响应 XML:");
    println!("{}\n", response_xml);

    // 解析目录响应 XML
    let parsed = siprs_gb28181_xml::Response::from_xml(&response_xml).unwrap();
    println!("[解析] 目录响应:");
    println!("  命令类型: {:?}", parsed.cmd_type);
    println!("  命令序号: {}", parsed.sn);
    println!(
        "  设备数量: {} (实际 {})",
        parsed.sum_num.unwrap_or(0),
        parsed.device_list.len()
    );
    for item in &parsed.device_list {
        println!(
            "  ├─ {} ({})",
            item.device_id,
            item.name.as_deref().unwrap_or("未知")
        );
    }

    // ========================================================================
    // 6. 停止平台端
    // ========================================================================

    println!("\n[停止] 正在停止平台端...");
    match server.stop().await {
        Ok(()) => println!("[✓] 平台端已停止"),
        Err(e) => println!("[✗] 停止失败: {}", e),
    }

    event_task.abort();
    println!("\n=== 示例结束 ===");
}
