//! SIP 传输连接池
//!
//! 管理面向连接的传输（TCP/TLS）的连接复用与生命周期。
//! 同一远端地址的多个消息复用同一连接，空闲超时自动关闭。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sip_core::TransportProtocol;
use tokio::sync::Mutex;
use tracing;

use crate::tcp::TcpWriteStream;

// ============================================================================
// PooledConnection - 连接池中的连接
// ============================================================================

/// 连接池中的连接条目
///
/// 包含连接实例和最后活动时间，用于空闲超时检测。
#[derive(Debug)]
pub struct PooledConnection {
    /// 连接类型
    pub transport: TransportProtocol,
    /// 最后活动时间
    pub last_activity: Instant,
    /// 连接是否已断开
    pub is_closed: bool,
}

impl PooledConnection {
    /// 创建新的连接池条目
    pub fn new(transport: TransportProtocol) -> Self {
        Self {
            transport,
            last_activity: Instant::now(),
            is_closed: false,
        }
    }

    /// 更新最后活动时间为当前时刻
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    /// 检查连接是否已空闲超时
    pub fn is_idle_timeout(&self, timeout: Duration) -> bool {
        self.last_activity.elapsed() > timeout
    }
}

// ============================================================================
// ConnectionPool - 连接池
// ============================================================================

/// 连接池
///
/// 管理面向连接的传输（TCP/TLS）的连接，支持：
/// - 同一远端地址的连接复用
/// - 空闲超时自动关闭
/// - 连接断开通知
///
/// UDP 不需要连接管理，不使用连接池。
pub struct ConnectionPool {
    /// TCP 连接池：远端地址 → 写入流
    tcp_connections: HashMap<SocketAddr, Arc<Mutex<TcpWriteStream>>>,
    /// TCP 连接元数据
    tcp_meta: HashMap<SocketAddr, PooledConnection>,
    /// 连接空闲超时时间
    idle_timeout: Duration,
}

impl ConnectionPool {
    /// 创建新的连接池
    ///
    /// # 参数
    ///
    /// - `idle_timeout` - 连接空闲超时时间（默认 30 秒）
    pub fn new(idle_timeout: Duration) -> Self {
        Self {
            tcp_connections: HashMap::new(),
            tcp_meta: HashMap::new(),
            idle_timeout,
        }
    }

    /// 使用默认空闲超时（30 秒）创建连接池
    pub fn default_pool() -> Self {
        Self::new(Duration::from_secs(30))
    }

    /// 添加 TCP 连接到连接池
    ///
    /// 如果该远端地址已有连接，则替换旧连接。
    pub fn add_tcp_connection(&mut self, addr: SocketAddr, conn: Arc<Mutex<TcpWriteStream>>) {
        tracing::debug!("ConnectionPool: adding TCP connection to {}", addr);
        self.tcp_connections.insert(addr, conn);
        self.tcp_meta
            .insert(addr, PooledConnection::new(TransportProtocol::Tcp));
    }

    /// 获取 TCP 连接
    ///
    /// 如果连接存在且未超时，返回连接并更新活动时间。
    /// 如果连接已超时，移除连接并返回 None。
    pub fn get_tcp_connection(&mut self, addr: &SocketAddr) -> Option<Arc<Mutex<TcpWriteStream>>> {
        // 检查元数据
        if let Some(meta) = self.tcp_meta.get_mut(addr) {
            if meta.is_idle_timeout(self.idle_timeout) {
                tracing::debug!("ConnectionPool: TCP connection to {} idle timeout", addr);
                self.remove_tcp_connection(addr);
                return None;
            }
            meta.touch();
        }

        self.tcp_connections.get(addr).cloned()
    }

    /// 移除 TCP 连接
    pub fn remove_tcp_connection(&mut self, addr: &SocketAddr) {
        self.tcp_connections.remove(addr);
        self.tcp_meta.remove(addr);
    }

    /// 检查是否存在到指定地址的 TCP 连接
    pub fn has_tcp_connection(&self, addr: &SocketAddr) -> bool {
        self.tcp_meta.contains_key(addr)
    }

    /// 清理所有空闲超时的连接
    ///
    /// 遍历所有连接，移除已超时的连接。返回被清理的连接地址列表。
    pub fn cleanup_idle_connections(&mut self) -> Vec<(SocketAddr, TransportProtocol)> {
        let mut closed = Vec::new();

        // 收集超时的 TCP 连接
        let timeout_addrs: Vec<SocketAddr> = self
            .tcp_meta
            .iter()
            .filter(|(_, meta)| meta.is_idle_timeout(self.idle_timeout))
            .map(|(addr, _)| *addr)
            .collect();

        for addr in &timeout_addrs {
            tracing::debug!("ConnectionPool: closing idle TCP connection to {}", addr);
            self.tcp_connections.remove(addr);
            self.tcp_meta.remove(addr);
            closed.push((*addr, TransportProtocol::Tcp));
        }

        closed
    }

    /// 获取当前活跃连接数
    pub fn active_connection_count(&self) -> usize {
        self.tcp_meta.len()
    }

    /// 关闭所有连接
    pub fn close_all(&mut self) {
        self.tcp_connections.clear();
        self.tcp_meta.clear();
    }

    /// 获取所有活跃连接的地址列表
    pub fn active_addresses(&self) -> Vec<(SocketAddr, TransportProtocol)> {
        self.tcp_meta
            .iter()
            .map(|(addr, meta)| (*addr, meta.transport))
            .collect()
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pooled_connection_new() {
        let conn = PooledConnection::new(TransportProtocol::Tcp);
        assert_eq!(conn.transport, TransportProtocol::Tcp);
        assert!(!conn.is_closed);
        assert!(!conn.is_idle_timeout(Duration::from_secs(30)));
    }

    #[test]
    fn test_pooled_connection_touch() {
        let mut conn = PooledConnection::new(TransportProtocol::Tcp);
        let before = conn.last_activity;
        conn.touch();
        assert!(conn.last_activity >= before);
    }

    #[test]
    fn test_pooled_connection_idle_timeout() {
        let mut conn = PooledConnection::new(TransportProtocol::Tcp);
        // 模拟超时：将 last_activity 设置为很久以前
        conn.last_activity = Instant::now() - Duration::from_secs(60);
        assert!(conn.is_idle_timeout(Duration::from_secs(30)));
    }

    #[test]
    fn test_connection_pool_new() {
        let pool = ConnectionPool::new(Duration::from_secs(30));
        assert_eq!(pool.active_connection_count(), 0);
    }

    #[test]
    fn test_connection_pool_default() {
        let pool = ConnectionPool::default_pool();
        assert_eq!(pool.active_connection_count(), 0);
    }

    #[test]
    fn test_connection_pool_add_and_get() {
        let mut pool = ConnectionPool::new(Duration::from_secs(30));
        let addr: SocketAddr = "192.168.1.1:5060".parse().unwrap();

        // 没有连接时应返回 None
        assert!(pool.get_tcp_connection(&addr).is_none());

        // 添加元数据后检查
        pool.tcp_meta
            .insert(addr, PooledConnection::new(TransportProtocol::Tcp));
        assert!(pool.has_tcp_connection(&addr));
    }

    #[test]
    fn test_connection_pool_cleanup() {
        let mut pool = ConnectionPool::new(Duration::from_secs(30));
        let addr: SocketAddr = "192.168.1.1:5060".parse().unwrap();

        // 添加一个已超时的连接元数据
        let mut meta = PooledConnection::new(TransportProtocol::Tcp);
        meta.last_activity = Instant::now() - Duration::from_secs(60);
        pool.tcp_meta.insert(addr, meta);

        let closed = pool.cleanup_idle_connections();
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].0, addr);
        assert_eq!(pool.active_connection_count(), 0);
    }

    #[test]
    fn test_connection_pool_close_all() {
        let mut pool = ConnectionPool::new(Duration::from_secs(30));
        let addr: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        pool.tcp_meta
            .insert(addr, PooledConnection::new(TransportProtocol::Tcp));
        assert_eq!(pool.active_connection_count(), 1);

        pool.close_all();
        assert_eq!(pool.active_connection_count(), 0);
    }

    #[test]
    fn test_connection_pool_active_addresses() {
        let mut pool = ConnectionPool::new(Duration::from_secs(30));
        let addr1: SocketAddr = "192.168.1.1:5060".parse().unwrap();
        let addr2: SocketAddr = "192.168.1.2:5060".parse().unwrap();

        pool.tcp_meta
            .insert(addr1, PooledConnection::new(TransportProtocol::Tcp));
        pool.tcp_meta
            .insert(addr2, PooledConnection::new(TransportProtocol::Tcp));

        let addrs = pool.active_addresses();
        assert_eq!(addrs.len(), 2);
    }
}
