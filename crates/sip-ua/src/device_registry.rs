//! GB28181 设备注册表
//!
//! 实现多设备并发管理、在线状态跟踪、心跳超时检测和设备树。
//!
//! # 核心功能
//!
//! - 设备注册/注销（从 REGISTER 请求信息）
//! - 在线状态管理（Online/Offline/Unknown）
//! - 心跳超时检测（自动标记离线）
//! - 设备信息更新（从 DeviceInfo 响应）
//! - 设备目录更新（从 Catalog 响应）
//! - 设备树构建与查询
//! - 事件通知（上线/离线/注册/注销/目录更新）
//!
//! # 架构
//!
//! ```text
//! DeviceRegistry
//!   ├── devices: Arc<Mutex<HashMap<String, RegisteredDevice>>>
//!   ├── heartbeat_timeout: u64
//!   └── event_tx/event_rx: mpsc::UnboundedSender/Receiver<DeviceRegistryEvent>
//! ```
//!
//! # 示例
//!
//! ```ignore
//! use sip_ua::device_registry::{DeviceRegistry, DeviceOnlineStatus};
//!
//! let registry = DeviceRegistry::new(180); // 心跳超时 180 秒
//!
//! // 注册设备
//! registry.register_device(
//!     "34020000001320000001",
//!     "sip:34020000001320000001@192.168.1.100:5060",
//!     "192.168.1.1:5060",
//!     3600,
//!     "call-id-123",
//! );
//!
//! // 更新心跳
//! registry.update_keepalive("34020000001320000001");
//!
//! // 检查心跳超时
//! registry.check_heartbeat();
//!
//! // 获取在线设备
//! let online = registry.list_online_devices();
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use gb28181_xml::DeviceItem;
use tokio::sync::{mpsc, Mutex};

// ============================================================================
// DeviceOnlineStatus - 设备在线状态
// ============================================================================

/// 设备在线状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceOnlineStatus {
    /// 在线
    Online,
    /// 离线
    Offline,
    /// 未知
    Unknown,
}

impl std::fmt::Display for DeviceOnlineStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceOnlineStatus::Online => write!(f, "ONLINE"),
            DeviceOnlineStatus::Offline => write!(f, "OFFLINE"),
            DeviceOnlineStatus::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

// ============================================================================
// RegisteredDevice - 注册设备信息
// ============================================================================

/// 注册设备信息
///
/// 保存一次 GB28181 设备注册的完整上下文信息，包括设备编码、
/// SIP 联系地址、在线状态、心跳时间、设备描述和子设备列表。
#[derive(Debug, Clone)]
pub struct RegisteredDevice {
    /// 设备国标编码（20位）
    pub device_id: String,
    /// SIP 联系地址
    pub contact: String,
    /// SIP 服务器地址
    pub server_addr: String,
    /// 在线状态
    pub status: DeviceOnlineStatus,
    /// 注册时间
    pub registered_at: Instant,
    /// 最后心跳时间
    pub last_keepalive: Instant,
    /// 注册有效期（秒）
    pub expires: u64,
    /// Call-ID
    pub call_id: String,
    /// 设备名称
    pub name: Option<String>,
    /// 设备厂商
    pub manufacturer: Option<String>,
    /// 设备型号
    pub model: Option<String>,
    /// IP 地址
    pub ip_address: Option<String>,
    /// SIP 端口
    pub port: Option<u16>,
    /// 子设备列表（设备树）
    pub sub_devices: Vec<DeviceItem>,
    /// 经度
    pub longitude: Option<f64>,
    /// 纬度
    pub latitude: Option<f64>,
}

impl RegisteredDevice {
    /// 计算注册剩余有效时间（秒）
    ///
    /// 如果已过期则返回 0。
    pub fn remaining_expires(&self) -> u64 {
        let elapsed = self.registered_at.elapsed().as_secs();
        self.expires.saturating_sub(elapsed)
    }

    /// 判断注册是否已过期
    pub fn is_expired(&self) -> bool {
        self.remaining_expires() == 0
    }

    /// 判断心跳是否超时
    ///
    /// # 参数
    ///
    /// - `timeout_secs` - 心跳超时阈值（秒）
    pub fn is_keepalive_timeout(&self, timeout_secs: u64) -> bool {
        self.last_keepalive.elapsed().as_secs() > timeout_secs
    }
}

// ============================================================================
// DeviceRegistryEvent - 设备注册表事件
// ============================================================================

/// 设备注册表事件
///
/// 当设备状态发生变化时，通过事件通道通知上层应用。
#[derive(Debug)]
pub enum DeviceRegistryEvent {
    /// 设备上线
    DeviceOnline { device_id: String },
    /// 设备离线
    DeviceOffline { device_id: String },
    /// 设备注册
    DeviceRegistered { device_id: String },
    /// 设备注销
    DeviceUnregistered { device_id: String },
    /// 设备目录更新
    DeviceCatalogUpdated { device_id: String, count: usize },
}

// ============================================================================
// DeviceTreeNode - 设备树节点
// ============================================================================

/// 设备树节点
///
/// 表示设备树中的一个节点，包含设备编码、名称、在线状态和子节点列表。
#[derive(Debug, Clone)]
pub struct DeviceTreeNode {
    /// 设备编码
    pub device_id: String,
    /// 设备名称
    pub name: Option<String>,
    /// 在线状态
    pub status: DeviceOnlineStatus,
    /// 子节点列表
    pub children: Vec<DeviceTreeNode>,
}

impl DeviceTreeNode {
    /// 创建新的设备树节点
    pub fn new(device_id: impl Into<String>) -> Self {
        Self {
            device_id: device_id.into(),
            name: None,
            status: DeviceOnlineStatus::Unknown,
            children: Vec::new(),
        }
    }

    /// 递归统计节点总数（包含自身）
    pub fn total_count(&self) -> usize {
        1 + self.children.iter().map(|c| c.total_count()).sum::<usize>()
    }
}

// ============================================================================
// DeviceTree - 设备树
// ============================================================================

/// 设备树
///
/// GB28181 设备树结构，根节点为平台编码，子节点为设备编码。
/// 子设备可以有更深的层级，形成树状结构。
#[derive(Debug, Clone)]
pub struct DeviceTree {
    /// 根节点（平台编码）
    pub root: DeviceTreeNode,
}

impl DeviceTree {
    /// 创建新的设备树
    ///
    /// # 参数
    ///
    /// - `root_id` - 根节点设备编码（通常为平台编码）
    pub fn new(root_id: impl Into<String>) -> Self {
        Self {
            root: DeviceTreeNode::new(root_id),
        }
    }

    /// 递归统计节点总数（包含根节点）
    pub fn total_count(&self) -> usize {
        self.root.total_count()
    }
}

// ============================================================================
// DeviceRegistry - 设备注册表
// ============================================================================

/// 设备注册表
///
/// 管理 GB28181 设备的注册、在线状态、心跳检测和设备树。
/// 使用 `Arc<Mutex<...>>` 保护内部状态，支持多设备并发管理。
///
/// # 线程安全
///
/// 所有公共方法通过 `&self` 引用和内部 `Mutex` 实现线程安全，
/// 可在多个异步任务间共享。
///
/// # 生命周期
///
/// 1. `new(heartbeat_timeout)` - 创建注册表
/// 2. `register_device()` - 注册设备
/// 3. `update_keepalive()` - 更新心跳
/// 4. `check_heartbeat()` - 检查心跳超时
/// 5. `event_stream()` - 获取事件流
pub struct DeviceRegistry {
    /// 设备映射表（device_id → RegisteredDevice）
    devices: Arc<Mutex<HashMap<String, RegisteredDevice>>>,
    /// 心跳超时（秒）
    heartbeat_timeout: u64,
    /// 事件发送端
    event_tx: mpsc::UnboundedSender<DeviceRegistryEvent>,
    /// 事件接收端
    event_rx: Option<mpsc::UnboundedReceiver<DeviceRegistryEvent>>,
}

impl DeviceRegistry {
    /// 创建新的设备注册表
    ///
    /// # 参数
    ///
    /// - `heartbeat_timeout` - 心跳超时阈值（秒），超过该时间未收到心跳则标记为离线
    pub fn new(heartbeat_timeout: u64) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel::<DeviceRegistryEvent>();

        Self {
            devices: Arc::new(Mutex::new(HashMap::new())),
            heartbeat_timeout,
            event_tx,
            event_rx: Some(event_rx),
        }
    }

    /// 注册设备
    ///
    /// 从 REGISTER 请求信息注册设备。如果设备已存在，则更新注册信息。
    /// 新注册的设备状态为 Online。
    ///
    /// # 参数
    ///
    /// - `device_id` - 设备国标编码
    /// - `contact` - SIP 联系地址
    /// - `server_addr` - SIP 服务器地址
    /// - `expires` - 注册有效期（秒）
    /// - `call_id` - Call-ID
    pub async fn register_device(
        &self,
        device_id: &str,
        contact: &str,
        server_addr: &str,
        expires: u64,
        call_id: &str,
    ) {
        let now = Instant::now();
        let mut devices = self.devices.lock().await;

        let is_new = !devices.contains_key(device_id);

        if let Some(existing) = devices.get_mut(device_id) {
            // 更新已有设备的注册信息
            existing.contact = contact.to_string();
            existing.server_addr = server_addr.to_string();
            existing.expires = expires;
            existing.call_id = call_id.to_string();
            existing.registered_at = now;
            existing.last_keepalive = now;
            // 如果设备之前是离线状态，重新注册后变为在线
            if existing.status == DeviceOnlineStatus::Offline {
                existing.status = DeviceOnlineStatus::Online;
                let _ = self.event_tx.send(DeviceRegistryEvent::DeviceOnline {
                    device_id: device_id.to_string(),
                });
            } else {
                existing.status = DeviceOnlineStatus::Online;
            }
        } else {
            // 新建设备
            let device = RegisteredDevice {
                device_id: device_id.to_string(),
                contact: contact.to_string(),
                server_addr: server_addr.to_string(),
                status: DeviceOnlineStatus::Online,
                registered_at: now,
                last_keepalive: now,
                expires,
                call_id: call_id.to_string(),
                name: None,
                manufacturer: None,
                model: None,
                ip_address: None,
                port: None,
                sub_devices: Vec::new(),
                longitude: None,
                latitude: None,
            };
            devices.insert(device_id.to_string(), device);
        }

        // 发送事件
        if is_new {
            let _ = self.event_tx.send(DeviceRegistryEvent::DeviceRegistered {
                device_id: device_id.to_string(),
            });
            let _ = self.event_tx.send(DeviceRegistryEvent::DeviceOnline {
                device_id: device_id.to_string(),
            });
        }

        tracing::info!(
            "DeviceRegistry: registered device {} (expires={}s)",
            device_id,
            expires
        );
    }

    /// 注销设备
    ///
    /// 从注册表中移除设备，并发送注销和离线事件。
    ///
    /// # 参数
    ///
    /// - `device_id` - 设备国标编码
    ///
    /// # 返回
    ///
    /// 如果设备存在并被注销则返回 `true`，否则返回 `false`。
    pub async fn unregister_device(&self, device_id: &str) -> bool {
        let mut devices = self.devices.lock().await;

        if let Some(_removed) = devices.remove(device_id) {
            let _ = self.event_tx.send(DeviceRegistryEvent::DeviceOffline {
                device_id: device_id.to_string(),
            });
            let _ = self.event_tx.send(DeviceRegistryEvent::DeviceUnregistered {
                device_id: device_id.to_string(),
            });

            tracing::info!("DeviceRegistry: unregistered device {}", device_id);
            true
        } else {
            tracing::warn!(
                "DeviceRegistry: attempted to unregister unknown device {}",
                device_id
            );
            false
        }
    }

    /// 更新心跳时间
    ///
    /// 当收到设备的 Keepalive 消息时调用此方法更新心跳时间。
    /// 如果设备之前是离线状态，会触发 DeviceOnline 事件。
    ///
    /// # 参数
    ///
    /// - `device_id` - 设备国标编码
    ///
    /// # 返回
    ///
    /// 如果设备存在则返回 `true`，否则返回 `false`。
    pub async fn update_keepalive(&self, device_id: &str) -> bool {
        let mut devices = self.devices.lock().await;

        if let Some(device) = devices.get_mut(device_id) {
            let was_offline = device.status == DeviceOnlineStatus::Offline;
            device.last_keepalive = Instant::now();
            device.status = DeviceOnlineStatus::Online;

            if was_offline {
                let _ = self.event_tx.send(DeviceRegistryEvent::DeviceOnline {
                    device_id: device_id.to_string(),
                });
                tracing::info!("DeviceRegistry: device {} back online", device_id);
            }

            tracing::debug!("DeviceRegistry: keepalive updated for {}", device_id);
            true
        } else {
            tracing::warn!("DeviceRegistry: keepalive for unknown device {}", device_id);
            false
        }
    }

    /// 更新设备信息
    ///
    /// 从 DeviceInfo 响应中更新设备的厂商、型号、名称等信息。
    ///
    /// # 参数
    ///
    /// - `device_id` - 设备国标编码
    /// - `name` - 设备名称
    /// - `manufacturer` - 设备厂商
    /// - `model` - 设备型号
    /// - `ip_address` - IP 地址
    /// - `port` - SIP 端口
    ///
    /// # 返回
    ///
    /// 如果设备存在则返回 `true`，否则返回 `false`。
    pub async fn update_device_info(
        &self,
        device_id: &str,
        name: Option<String>,
        manufacturer: Option<String>,
        model: Option<String>,
        ip_address: Option<String>,
        port: Option<u16>,
    ) -> bool {
        let mut devices = self.devices.lock().await;

        if let Some(device) = devices.get_mut(device_id) {
            if let Some(v) = name {
                device.name = Some(v);
            }
            if let Some(v) = manufacturer {
                device.manufacturer = Some(v);
            }
            if let Some(v) = model {
                device.model = Some(v);
            }
            if let Some(v) = ip_address {
                device.ip_address = Some(v);
            }
            if let Some(v) = port {
                device.port = Some(v);
            }

            tracing::info!("DeviceRegistry: updated device info for {}", device_id);
            true
        } else {
            tracing::warn!(
                "DeviceRegistry: device info update for unknown device {}",
                device_id
            );
            false
        }
    }

    /// 更新设备目录
    ///
    /// 从 Catalog 响应中更新设备的子设备列表。
    ///
    /// # 参数
    ///
    /// - `device_id` - 设备国标编码
    /// - `sub_devices` - 子设备列表
    ///
    /// # 返回
    ///
    /// 如果设备存在则返回 `true`，否则返回 `false`。
    pub async fn update_catalog(&self, device_id: &str, sub_devices: Vec<DeviceItem>) -> bool {
        let count = sub_devices.len();
        let mut devices = self.devices.lock().await;

        if let Some(device) = devices.get_mut(device_id) {
            device.sub_devices = sub_devices;

            let _ = self
                .event_tx
                .send(DeviceRegistryEvent::DeviceCatalogUpdated {
                    device_id: device_id.to_string(),
                    count,
                });

            tracing::info!(
                "DeviceRegistry: updated catalog for {} (count={})",
                device_id,
                count
            );
            true
        } else {
            tracing::warn!(
                "DeviceRegistry: catalog update for unknown device {}",
                device_id
            );
            false
        }
    }

    /// 获取设备信息
    ///
    /// # 参数
    ///
    /// - `device_id` - 设备国标编码
    ///
    /// # 返回
    ///
    /// 返回设备的克隆信息，如果设备不存在则返回 `None`。
    pub async fn get_device(&self, device_id: &str) -> Option<RegisteredDevice> {
        let devices = self.devices.lock().await;
        devices.get(device_id).cloned()
    }

    /// 列出所有设备
    ///
    /// 返回注册表中所有设备的克隆列表。
    pub async fn list_devices(&self) -> Vec<RegisteredDevice> {
        let devices = self.devices.lock().await;
        devices.values().cloned().collect()
    }

    /// 列出在线设备
    ///
    /// 仅返回状态为 Online 的设备列表。
    pub async fn list_online_devices(&self) -> Vec<RegisteredDevice> {
        let devices = self.devices.lock().await;
        devices
            .values()
            .filter(|d| d.status == DeviceOnlineStatus::Online)
            .cloned()
            .collect()
    }

    /// 检查心跳超时
    ///
    /// 遍历所有设备，检查心跳是否超时。超时的设备状态被标记为 Offline，
    /// 并触发 DeviceOffline 事件。
    ///
    /// # 返回
    ///
    /// 返回被标记为离线的设备数量。
    pub async fn check_heartbeat(&self) -> usize {
        let mut devices = self.devices.lock().await;
        let timeout = self.heartbeat_timeout;
        let mut offline_count = 0;

        for (device_id, device) in devices.iter_mut() {
            if device.status == DeviceOnlineStatus::Online && device.is_keepalive_timeout(timeout) {
                device.status = DeviceOnlineStatus::Offline;
                offline_count += 1;

                let _ = self.event_tx.send(DeviceRegistryEvent::DeviceOffline {
                    device_id: device_id.clone(),
                });

                tracing::warn!(
                    "DeviceRegistry: device {} heartbeat timeout (last_keepalive={}s ago, timeout={}s)",
                    device_id,
                    device.last_keepalive.elapsed().as_secs(),
                    timeout
                );
            }
        }

        if offline_count > 0 {
            tracing::info!(
                "DeviceRegistry: heartbeat check completed, {} device(s) offline",
                offline_count
            );
        }

        offline_count
    }

    /// 获取事件流
    ///
    /// 返回设备注册表事件接收端。此方法只能调用一次。
    pub fn event_stream(&mut self) -> Option<mpsc::UnboundedReceiver<DeviceRegistryEvent>> {
        self.event_rx.take()
    }

    /// 获取设备树
    ///
    /// 构建以 `root_id` 为根节点的设备树。所有已注册设备作为根节点的子节点，
    /// 每个设备的子设备作为更深层的子节点。
    ///
    /// # 参数
    ///
    /// - `root_id` - 根节点设备编码（通常为平台编码）
    pub async fn get_device_tree(&self, root_id: &str) -> DeviceTree {
        let devices = self.devices.lock().await;
        let mut tree = DeviceTree::new(root_id);

        for device in devices.values() {
            let mut node = DeviceTreeNode {
                device_id: device.device_id.clone(),
                name: device.name.clone(),
                status: device.status,
                children: Vec::new(),
            };

            // 添加子设备
            for sub in &device.sub_devices {
                let sub_node = DeviceTreeNode {
                    device_id: sub.device_id.to_string(),
                    name: sub.name.clone(),
                    status: if let Some(ref s) = sub.status {
                        match s.as_str() {
                            "ON" => DeviceOnlineStatus::Online,
                            "OFF" => DeviceOnlineStatus::Offline,
                            _ => DeviceOnlineStatus::Unknown,
                        }
                    } else {
                        DeviceOnlineStatus::Unknown
                    },
                    children: Vec::new(),
                };
                node.children.push(sub_node);
            }

            tree.root.children.push(node);
        }

        tree
    }

    /// 按设备ID查询子设备
    ///
    /// 返回指定设备的子设备列表。如果设备不存在，返回空列表。
    ///
    /// # 参数
    ///
    /// - `device_id` - 设备国标编码
    pub async fn get_sub_devices(&self, device_id: &str) -> Vec<DeviceItem> {
        let devices = self.devices.lock().await;
        devices
            .get(device_id)
            .map(|d| d.sub_devices.clone())
            .unwrap_or_default()
    }

    /// 获取设备数量
    pub async fn device_count(&self) -> usize {
        let devices = self.devices.lock().await;
        devices.len()
    }

    /// 获取在线设备数量
    pub async fn online_device_count(&self) -> usize {
        let devices = self.devices.lock().await;
        devices
            .values()
            .filter(|d| d.status == DeviceOnlineStatus::Online)
            .count()
    }

    /// 获取心跳超时阈值
    pub fn heartbeat_timeout(&self) -> u64 {
        self.heartbeat_timeout
    }

    /// 设置心跳超时阈值
    pub fn set_heartbeat_timeout(&mut self, timeout: u64) {
        self.heartbeat_timeout = timeout;
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 创建测试用注册表
    fn make_registry() -> DeviceRegistry {
        DeviceRegistry::new(180)
    }

    // ── DeviceOnlineStatus 测试 ──────────────────────────────────────

    #[test]
    fn test_online_status_display() {
        assert_eq!(format!("{}", DeviceOnlineStatus::Online), "ONLINE");
        assert_eq!(format!("{}", DeviceOnlineStatus::Offline), "OFFLINE");
        assert_eq!(format!("{}", DeviceOnlineStatus::Unknown), "UNKNOWN");
    }

    #[test]
    fn test_online_status_equality() {
        assert_eq!(DeviceOnlineStatus::Online, DeviceOnlineStatus::Online);
        assert_ne!(DeviceOnlineStatus::Online, DeviceOnlineStatus::Offline);
        assert_ne!(DeviceOnlineStatus::Offline, DeviceOnlineStatus::Unknown);
    }

    // ── RegisteredDevice 测试 ────────────────────────────────────────

    #[test]
    fn test_registered_device_remaining_expires() {
        let device = RegisteredDevice {
            device_id: "34020000001320000001".to_string(),
            contact: "sip:34020000001320000001@192.168.1.100:5060".to_string(),
            server_addr: "192.168.1.1:5060".to_string(),
            status: DeviceOnlineStatus::Online,
            registered_at: Instant::now(),
            last_keepalive: Instant::now(),
            expires: 3600,
            call_id: "call-123".to_string(),
            name: None,
            manufacturer: None,
            model: None,
            ip_address: None,
            port: None,
            sub_devices: Vec::new(),
            longitude: None,
            latitude: None,
        };

        // 刚注册，剩余时间应接近 3600
        assert!(device.remaining_expires() <= 3600);
        assert!(device.remaining_expires() >= 3599);
        assert!(!device.is_expired());
    }

    #[test]
    fn test_registered_device_expired() {
        let device = RegisteredDevice {
            device_id: "34020000001320000001".to_string(),
            contact: "sip:34020000001320000001@192.168.1.100:5060".to_string(),
            server_addr: "192.168.1.1:5060".to_string(),
            status: DeviceOnlineStatus::Online,
            registered_at: Instant::now() - std::time::Duration::from_secs(7200),
            last_keepalive: Instant::now() - std::time::Duration::from_secs(7200),
            expires: 3600,
            call_id: "call-123".to_string(),
            name: None,
            manufacturer: None,
            model: None,
            ip_address: None,
            port: None,
            sub_devices: Vec::new(),
            longitude: None,
            latitude: None,
        };

        assert!(device.is_expired());
        assert_eq!(device.remaining_expires(), 0);
    }

    #[test]
    fn test_registered_device_keepalive_timeout() {
        let device = RegisteredDevice {
            device_id: "34020000001320000001".to_string(),
            contact: "sip:34020000001320000001@192.168.1.100:5060".to_string(),
            server_addr: "192.168.1.1:5060".to_string(),
            status: DeviceOnlineStatus::Online,
            registered_at: Instant::now(),
            last_keepalive: Instant::now() - std::time::Duration::from_secs(200),
            expires: 3600,
            call_id: "call-123".to_string(),
            name: None,
            manufacturer: None,
            model: None,
            ip_address: None,
            port: None,
            sub_devices: Vec::new(),
            longitude: None,
            latitude: None,
        };

        assert!(device.is_keepalive_timeout(180));
        assert!(!device.is_keepalive_timeout(300));
    }

    // ── DeviceRegistry 注册/注销 测试 ────────────────────────────────

    #[tokio::test]
    async fn test_register_device() {
        let registry = make_registry();

        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.100:5060",
                "192.168.1.1:5060",
                3600,
                "call-123",
            )
            .await;

        let device = registry.get_device("34020000001320000001").await;
        assert!(device.is_some());

        let device = device.unwrap();
        assert_eq!(device.device_id, "34020000001320000001");
        assert_eq!(
            device.contact,
            "sip:34020000001320000001@192.168.1.100:5060"
        );
        assert_eq!(device.server_addr, "192.168.1.1:5060");
        assert_eq!(device.status, DeviceOnlineStatus::Online);
        assert_eq!(device.expires, 3600);
        assert_eq!(device.call_id, "call-123");
    }

    #[tokio::test]
    async fn test_register_device_events() {
        let mut registry = make_registry();

        // 先获取事件流
        let mut event_rx = registry.event_stream().unwrap();

        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.100:5060",
                "192.168.1.1:5060",
                3600,
                "call-123",
            )
            .await;

        // 应收到 DeviceRegistered 和 DeviceOnline 事件
        let event1 = event_rx.recv().await.unwrap();
        assert!(matches!(
            event1,
            DeviceRegistryEvent::DeviceRegistered { ref device_id }
            if device_id == "34020000001320000001"
        ));

        let event2 = event_rx.recv().await.unwrap();
        assert!(matches!(
            event2,
            DeviceRegistryEvent::DeviceOnline { ref device_id }
            if device_id == "34020000001320000001"
        ));
    }

    #[tokio::test]
    async fn test_unregister_device() {
        let registry = make_registry();

        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.100:5060",
                "192.168.1.1:5060",
                3600,
                "call-123",
            )
            .await;

        assert!(registry.unregister_device("34020000001320000001").await);
        assert!(registry.get_device("34020000001320000001").await.is_none());
    }

    #[tokio::test]
    async fn test_unregister_unknown_device() {
        let registry = make_registry();
        assert!(!registry.unregister_device("unknown").await);
    }

    #[tokio::test]
    async fn test_unregister_device_events() {
        let mut registry = make_registry();

        let mut event_rx = registry.event_stream().unwrap();

        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.100:5060",
                "192.168.1.1:5060",
                3600,
                "call-123",
            )
            .await;

        // 消费注册事件
        let _ = event_rx.recv().await; // DeviceRegistered
        let _ = event_rx.recv().await; // DeviceOnline

        registry.unregister_device("34020000001320000001").await;

        // 应收到 DeviceOffline 和 DeviceUnregistered 事件
        let event1 = event_rx.recv().await.unwrap();
        assert!(matches!(
            event1,
            DeviceRegistryEvent::DeviceOffline { ref device_id }
            if device_id == "34020000001320000001"
        ));

        let event2 = event_rx.recv().await.unwrap();
        assert!(matches!(
            event2,
            DeviceRegistryEvent::DeviceUnregistered { ref device_id }
            if device_id == "34020000001320000001"
        ));
    }

    #[tokio::test]
    async fn test_re_register_device() {
        let registry = make_registry();

        // 首次注册
        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.100:5060",
                "192.168.1.1:5060",
                3600,
                "call-123",
            )
            .await;

        // 重新注册（更新信息）
        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.200:5060",
                "192.168.1.2:5060",
                7200,
                "call-456",
            )
            .await;

        let device = registry.get_device("34020000001320000001").await.unwrap();
        assert_eq!(
            device.contact,
            "sip:34020000001320000001@192.168.1.200:5060"
        );
        assert_eq!(device.server_addr, "192.168.1.2:5060");
        assert_eq!(device.expires, 7200);
        assert_eq!(device.call_id, "call-456");
        assert_eq!(device.status, DeviceOnlineStatus::Online);
    }

    // ── 心跳测试 ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_update_keepalive() {
        let registry = make_registry();

        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.100:5060",
                "192.168.1.1:5060",
                3600,
                "call-123",
            )
            .await;

        assert!(registry.update_keepalive("34020000001320000001").await);
        assert!(!registry.update_keepalive("unknown").await);
    }

    #[tokio::test]
    async fn test_check_heartbeat_no_timeout() {
        let registry = make_registry();

        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.100:5060",
                "192.168.1.1:5060",
                3600,
                "call-123",
            )
            .await;

        // 刚注册，不应超时
        let offline_count = registry.check_heartbeat().await;
        assert_eq!(offline_count, 0);

        let device = registry.get_device("34020000001320000001").await.unwrap();
        assert_eq!(device.status, DeviceOnlineStatus::Online);
    }

    #[tokio::test]
    async fn test_check_heartbeat_with_timeout() {
        let registry = make_registry();

        // 手动构造一个心跳超时的设备
        let device = RegisteredDevice {
            device_id: "34020000001320000001".to_string(),
            contact: "sip:34020000001320000001@192.168.1.100:5060".to_string(),
            server_addr: "192.168.1.1:5060".to_string(),
            status: DeviceOnlineStatus::Online,
            registered_at: Instant::now() - std::time::Duration::from_secs(300),
            last_keepalive: Instant::now() - std::time::Duration::from_secs(300),
            expires: 3600,
            call_id: "call-123".to_string(),
            name: None,
            manufacturer: None,
            model: None,
            ip_address: None,
            port: None,
            sub_devices: Vec::new(),
            longitude: None,
            latitude: None,
        };

        {
            let mut devices = registry.devices.lock().await;
            devices.insert("34020000001320000001".to_string(), device);
        }

        let offline_count = registry.check_heartbeat().await;
        assert_eq!(offline_count, 1);

        let device = registry.get_device("34020000001320000001").await.unwrap();
        assert_eq!(device.status, DeviceOnlineStatus::Offline);
    }

    #[tokio::test]
    async fn test_check_heartbeat_offline_event() {
        let mut registry = make_registry();
        let mut event_rx = registry.event_stream().unwrap();

        // 手动构造一个心跳超时的设备
        let device = RegisteredDevice {
            device_id: "34020000001320000001".to_string(),
            contact: "sip:34020000001320000001@192.168.1.100:5060".to_string(),
            server_addr: "192.168.1.1:5060".to_string(),
            status: DeviceOnlineStatus::Online,
            registered_at: Instant::now() - std::time::Duration::from_secs(300),
            last_keepalive: Instant::now() - std::time::Duration::from_secs(300),
            expires: 3600,
            call_id: "call-123".to_string(),
            name: None,
            manufacturer: None,
            model: None,
            ip_address: None,
            port: None,
            sub_devices: Vec::new(),
            longitude: None,
            latitude: None,
        };

        {
            let mut devices = registry.devices.lock().await;
            devices.insert("34020000001320000001".to_string(), device);
        }

        registry.check_heartbeat().await;

        let event = event_rx.recv().await.unwrap();
        assert!(matches!(
            event,
            DeviceRegistryEvent::DeviceOffline { ref device_id }
            if device_id == "34020000001320000001"
        ));
    }

    #[tokio::test]
    async fn test_keepalive_revive_offline_device() {
        let mut registry = make_registry();
        let mut event_rx = registry.event_stream().unwrap();

        // 手动构造一个离线设备
        let device = RegisteredDevice {
            device_id: "34020000001320000001".to_string(),
            contact: "sip:34020000001320000001@192.168.1.100:5060".to_string(),
            server_addr: "192.168.1.1:5060".to_string(),
            status: DeviceOnlineStatus::Offline,
            registered_at: Instant::now(),
            last_keepalive: Instant::now() - std::time::Duration::from_secs(300),
            expires: 3600,
            call_id: "call-123".to_string(),
            name: None,
            manufacturer: None,
            model: None,
            ip_address: None,
            port: None,
            sub_devices: Vec::new(),
            longitude: None,
            latitude: None,
        };

        {
            let mut devices = registry.devices.lock().await;
            devices.insert("34020000001320000001".to_string(), device);
        }

        // 更新心跳，设备应恢复在线
        assert!(registry.update_keepalive("34020000001320000001").await);

        let device = registry.get_device("34020000001320000001").await.unwrap();
        assert_eq!(device.status, DeviceOnlineStatus::Online);

        // 应收到 DeviceOnline 事件
        let event = event_rx.recv().await.unwrap();
        assert!(matches!(
            event,
            DeviceRegistryEvent::DeviceOnline { ref device_id }
            if device_id == "34020000001320000001"
        ));
    }

    // ── 设备信息更新测试 ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_update_device_info() {
        let registry = make_registry();

        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.100:5060",
                "192.168.1.1:5060",
                3600,
                "call-123",
            )
            .await;

        let result = registry
            .update_device_info(
                "34020000001320000001",
                Some("Camera1".to_string()),
                Some("Hikvision".to_string()),
                Some("DS-2CD2143".to_string()),
                Some("192.168.1.100".to_string()),
                Some(5060),
            )
            .await;

        assert!(result);

        let device = registry.get_device("34020000001320000001").await.unwrap();
        assert_eq!(device.name.as_deref(), Some("Camera1"));
        assert_eq!(device.manufacturer.as_deref(), Some("Hikvision"));
        assert_eq!(device.model.as_deref(), Some("DS-2CD2143"));
        assert_eq!(device.ip_address.as_deref(), Some("192.168.1.100"));
        assert_eq!(device.port, Some(5060));
    }

    #[tokio::test]
    async fn test_update_device_info_partial() {
        let registry = make_registry();

        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.100:5060",
                "192.168.1.1:5060",
                3600,
                "call-123",
            )
            .await;

        // 只更新名称
        let result = registry
            .update_device_info(
                "34020000001320000001",
                Some("Camera1".to_string()),
                None,
                None,
                None,
                None,
            )
            .await;

        assert!(result);

        let device = registry.get_device("34020000001320000001").await.unwrap();
        assert_eq!(device.name.as_deref(), Some("Camera1"));
        assert!(device.manufacturer.is_none());
        assert!(device.model.is_none());
    }

    #[tokio::test]
    async fn test_update_device_info_unknown_device() {
        let registry = make_registry();
        let result = registry
            .update_device_info(
                "unknown",
                Some("Camera1".to_string()),
                None,
                None,
                None,
                None,
            )
            .await;
        assert!(!result);
    }

    // ── 设备目录更新测试 ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_update_catalog() {
        let registry = make_registry();

        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.100:5060",
                "192.168.1.1:5060",
                3600,
                "call-123",
            )
            .await;

        // 构建子设备列表
        let sub1_id = gb28181_codec::DeviceId::parse("34020000001320000001").unwrap();
        let mut sub1 = DeviceItem::new(sub1_id);
        sub1.name = Some("Camera1".to_string());
        sub1.status = Some("ON".to_string());

        let sub2_id = gb28181_codec::DeviceId::parse("34020000001310000001").unwrap();
        let mut sub2 = DeviceItem::new(sub2_id);
        sub2.name = Some("Encoder1".to_string());
        sub2.status = Some("OFF".to_string());

        let sub_devices = vec![sub1, sub2];

        let result = registry
            .update_catalog("34020000001320000001", sub_devices)
            .await;
        assert!(result);

        let sub = registry.get_sub_devices("34020000001320000001").await;
        assert_eq!(sub.len(), 2);
        assert_eq!(sub[0].name.as_deref(), Some("Camera1"));
        assert_eq!(sub[1].name.as_deref(), Some("Encoder1"));
    }

    #[tokio::test]
    async fn test_update_catalog_event() {
        let mut registry = make_registry();
        let mut event_rx = registry.event_stream().unwrap();

        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.100:5060",
                "192.168.1.1:5060",
                3600,
                "call-123",
            )
            .await;

        // 消费注册事件
        let _ = event_rx.recv().await; // DeviceRegistered
        let _ = event_rx.recv().await; // DeviceOnline

        // 构建子设备列表
        let sub1_id = gb28181_codec::DeviceId::parse("34020000001320000001").unwrap();
        let sub1 = DeviceItem::new(sub1_id);

        registry
            .update_catalog("34020000001320000001", vec![sub1])
            .await;

        let event = event_rx.recv().await.unwrap();
        assert!(matches!(
            event,
            DeviceRegistryEvent::DeviceCatalogUpdated {
                ref device_id,
                count: 1
            } if device_id == "34020000001320000001"
        ));
    }

    #[tokio::test]
    async fn test_update_catalog_unknown_device() {
        let registry = make_registry();
        let result = registry.update_catalog("unknown", vec![]).await;
        assert!(!result);
    }

    // ── 设备列表测试 ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_list_devices() {
        let registry = make_registry();

        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.100:5060",
                "192.168.1.1:5060",
                3600,
                "call-123",
            )
            .await;

        registry
            .register_device(
                "34020000001310000001",
                "sip:34020000001310000001@192.168.1.101:5060",
                "192.168.1.1:5060",
                3600,
                "call-456",
            )
            .await;

        let devices = registry.list_devices().await;
        assert_eq!(devices.len(), 2);
    }

    #[tokio::test]
    async fn test_list_online_devices() {
        let registry = make_registry();

        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.100:5060",
                "192.168.1.1:5060",
                3600,
                "call-123",
            )
            .await;

        // 手动添加一个离线设备
        let offline_device = RegisteredDevice {
            device_id: "34020000001310000001".to_string(),
            contact: "sip:34020000001310000001@192.168.1.101:5060".to_string(),
            server_addr: "192.168.1.1:5060".to_string(),
            status: DeviceOnlineStatus::Offline,
            registered_at: Instant::now(),
            last_keepalive: Instant::now() - std::time::Duration::from_secs(300),
            expires: 3600,
            call_id: "call-456".to_string(),
            name: None,
            manufacturer: None,
            model: None,
            ip_address: None,
            port: None,
            sub_devices: Vec::new(),
            longitude: None,
            latitude: None,
        };

        {
            let mut devices = registry.devices.lock().await;
            devices.insert("34020000001310000001".to_string(), offline_device);
        }

        let online = registry.list_online_devices().await;
        assert_eq!(online.len(), 1);
        assert_eq!(online[0].device_id, "34020000001320000001");
    }

    #[tokio::test]
    async fn test_device_count() {
        let registry = make_registry();

        assert_eq!(registry.device_count().await, 0);

        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.100:5060",
                "192.168.1.1:5060",
                3600,
                "call-123",
            )
            .await;

        assert_eq!(registry.device_count().await, 1);
        assert_eq!(registry.online_device_count().await, 1);
    }

    // ── 设备树测试 ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_device_tree() {
        let registry = make_registry();

        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.100:5060",
                "192.168.1.1:5060",
                3600,
                "call-123",
            )
            .await;

        // 更新设备信息
        registry
            .update_device_info(
                "34020000001320000001",
                Some("Camera1".to_string()),
                None,
                None,
                None,
                None,
            )
            .await;

        let tree = registry.get_device_tree("34020000002000000001").await;

        assert_eq!(tree.root.device_id, "34020000002000000001");
        assert_eq!(tree.root.children.len(), 1);
        assert_eq!(tree.root.children[0].device_id, "34020000001320000001");
        assert_eq!(tree.root.children[0].name.as_deref(), Some("Camera1"));
        assert_eq!(tree.root.children[0].status, DeviceOnlineStatus::Online);
    }

    #[tokio::test]
    async fn test_device_tree_with_sub_devices() {
        let registry = make_registry();

        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.100:5060",
                "192.168.1.1:5060",
                3600,
                "call-123",
            )
            .await;

        // 添加子设备
        let sub1_id = gb28181_codec::DeviceId::parse("34020000001320000002").unwrap();
        let mut sub1 = DeviceItem::new(sub1_id);
        sub1.name = Some("SubCamera1".to_string());
        sub1.status = Some("ON".to_string());

        let sub2_id = gb28181_codec::DeviceId::parse("34020000001320000003").unwrap();
        let mut sub2 = DeviceItem::new(sub2_id);
        sub2.name = Some("SubCamera2".to_string());
        sub2.status = Some("OFF".to_string());

        registry
            .update_catalog("34020000001320000001", vec![sub1, sub2])
            .await;

        let tree = registry.get_device_tree("34020000002000000001").await;

        assert_eq!(tree.root.children.len(), 1);
        let device_node = &tree.root.children[0];
        assert_eq!(device_node.children.len(), 2);
        assert_eq!(device_node.children[0].device_id, "34020000001320000002");
        assert_eq!(device_node.children[0].status, DeviceOnlineStatus::Online);
        assert_eq!(device_node.children[1].device_id, "34020000001320000003");
        assert_eq!(device_node.children[1].status, DeviceOnlineStatus::Offline);
    }

    #[tokio::test]
    async fn test_device_tree_empty() {
        let registry = make_registry();

        let tree = registry.get_device_tree("34020000002000000001").await;

        assert_eq!(tree.root.device_id, "34020000002000000001");
        assert!(tree.root.children.is_empty());
        assert_eq!(tree.total_count(), 1); // 只有根节点
    }

    #[tokio::test]
    async fn test_device_tree_total_count() {
        let registry = make_registry();

        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.100:5060",
                "192.168.1.1:5060",
                3600,
                "call-123",
            )
            .await;

        // 添加子设备
        let sub1_id = gb28181_codec::DeviceId::parse("34020000001320000002").unwrap();
        let sub1 = DeviceItem::new(sub1_id);

        let sub2_id = gb28181_codec::DeviceId::parse("34020000001320000003").unwrap();
        let sub2 = DeviceItem::new(sub2_id);

        registry
            .update_catalog("34020000001320000001", vec![sub1, sub2])
            .await;

        let tree = registry.get_device_tree("34020000002000000001").await;

        // 根节点 + 1个设备 + 2个子设备 = 4
        assert_eq!(tree.total_count(), 4);
    }

    // ── get_sub_devices 测试 ──────────────────────────────────────────

    #[tokio::test]
    async fn test_get_sub_devices() {
        let registry = make_registry();

        // 不存在的设备应返回空列表
        let sub = registry.get_sub_devices("unknown").await;
        assert!(sub.is_empty());

        // 注册设备
        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.100:5060",
                "192.168.1.1:5060",
                3600,
                "call-123",
            )
            .await;

        // 无子设备
        let sub = registry.get_sub_devices("34020000001320000001").await;
        assert!(sub.is_empty());

        // 添加子设备
        let sub1_id = gb28181_codec::DeviceId::parse("34020000001320000002").unwrap();
        let mut sub1 = DeviceItem::new(sub1_id);
        sub1.name = Some("SubCamera1".to_string());

        registry
            .update_catalog("34020000001320000001", vec![sub1])
            .await;

        let sub = registry.get_sub_devices("34020000001320000001").await;
        assert_eq!(sub.len(), 1);
        assert_eq!(sub[0].name.as_deref(), Some("SubCamera1"));
    }

    // ── 事件流测试 ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_event_stream_once() {
        let mut registry = make_registry();

        // 第一次调用应返回 Some
        let rx = registry.event_stream();
        assert!(rx.is_some());

        // 第二次调用应返回 None
        let rx2 = registry.event_stream();
        assert!(rx2.is_none());
    }

    // ── 心跳超时配置测试 ──────────────────────────────────────────────

    #[test]
    fn test_heartbeat_timeout_config() {
        let mut registry = DeviceRegistry::new(180);
        assert_eq!(registry.heartbeat_timeout(), 180);

        registry.set_heartbeat_timeout(300);
        assert_eq!(registry.heartbeat_timeout(), 300);
    }

    // ── 多设备并发测试 ────────────────────────────────────────────────

    #[tokio::test]
    async fn test_multiple_devices() {
        let registry = make_registry();

        // 注册多个设备
        for i in 1..=5 {
            let device_id = format!("3402000000132000000{}", i);
            let contact = format!("sip:{}@192.168.1.{}:5060", device_id, 100 + i);
            registry
                .register_device(
                    &device_id,
                    &contact,
                    "192.168.1.1:5060",
                    3600,
                    &format!("call-{}", i),
                )
                .await;
        }

        assert_eq!(registry.device_count().await, 5);
        assert_eq!(registry.online_device_count().await, 5);

        // 注销部分设备
        assert!(registry.unregister_device("34020000001320000002").await);
        assert!(registry.unregister_device("34020000001320000004").await);

        assert_eq!(registry.device_count().await, 3);
    }

    #[tokio::test]
    async fn test_catalog_update_replaces_sub_devices() {
        let registry = make_registry();

        registry
            .register_device(
                "34020000001320000001",
                "sip:34020000001320000001@192.168.1.100:5060",
                "192.168.1.1:5060",
                3600,
                "call-123",
            )
            .await;

        // 第一次更新目录
        let sub1_id = gb28181_codec::DeviceId::parse("34020000001320000002").unwrap();
        let sub1 = DeviceItem::new(sub1_id);
        registry
            .update_catalog("34020000001320000001", vec![sub1])
            .await;

        assert_eq!(
            registry.get_sub_devices("34020000001320000001").await.len(),
            1
        );

        // 第二次更新目录（替换）
        let sub2_id = gb28181_codec::DeviceId::parse("34020000001320000003").unwrap();
        let sub3_id = gb28181_codec::DeviceId::parse("34020000001320000004").unwrap();
        let sub2 = DeviceItem::new(sub2_id);
        let sub3 = DeviceItem::new(sub3_id);
        registry
            .update_catalog("34020000001320000001", vec![sub2, sub3])
            .await;

        let sub = registry.get_sub_devices("34020000001320000001").await;
        assert_eq!(sub.len(), 2);
        assert_eq!(sub[0].device_id.to_string(), "34020000001320000003");
        assert_eq!(sub[1].device_id.to_string(), "34020000001320000004");
    }

    // ── DeviceTreeNode 测试 ───────────────────────────────────────────

    #[test]
    fn test_device_tree_node() {
        let mut root = DeviceTreeNode::new("34020000002000000001");
        root.name = Some("Platform".to_string());
        root.status = DeviceOnlineStatus::Online;

        let child1 = DeviceTreeNode {
            device_id: "34020000001320000001".to_string(),
            name: Some("Camera1".to_string()),
            status: DeviceOnlineStatus::Online,
            children: Vec::new(),
        };

        let child2 = DeviceTreeNode {
            device_id: "34020000001320000002".to_string(),
            name: Some("Camera2".to_string()),
            status: DeviceOnlineStatus::Offline,
            children: Vec::new(),
        };

        root.children.push(child1);
        root.children.push(child2);

        assert_eq!(root.total_count(), 3);
        assert_eq!(root.children.len(), 2);
    }

    #[test]
    fn test_device_tree_node_nested() {
        let mut root = DeviceTreeNode::new("root");
        let mut child = DeviceTreeNode::new("child1");
        child.children.push(DeviceTreeNode::new("grandchild1"));
        child.children.push(DeviceTreeNode::new("grandchild2"));
        root.children.push(child);

        assert_eq!(root.total_count(), 4);
    }
}
