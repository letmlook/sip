//! SIP 传输管理器
//!
//! 统一管理所有传输协议（UDP/TCP/TLS），提供消息发送、接收和
//! 传输协议自动选择功能。

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use siprs_core::config::{TlsConfig, TransportConfig};
use siprs_core::metrics::SipMetrics;
use siprs_core::{Host, TransportError, TransportProtocol};
use siprs_message::builder::MessageBuilder;
use siprs_message::headers::{HeaderName, HeaderValue, ViaHeader};
use siprs_message::parser::MessageParser;
use siprs_message::SipMessage;
use tokio::sync::{mpsc, Mutex};
use tracing;

use crate::connection_pool::ConnectionPool;
use crate::dns::{DnsResolver, SystemDnsResolver};
use crate::tcp::{TcpConnection, TcpListener, TcpWriteStream};
#[cfg(feature = "tls-rustls")]
use crate::tls::TlsConnection;
use crate::traits::{Transport, TransportEvent};
use crate::udp::UdpTransport;

/// SIP 默认最大消息大小
const SIP_DEFAULT_MAX_MESSAGE_SIZE: usize = 65535;

// ============================================================================
// TransportManager - 传输管理器
// ============================================================================

/// SIP 传输管理器
///
/// 统一管理所有传输协议，提供：
/// - 传输协议自动选择（sips: → TLS, transport=tcp → TCP, 默认 UDP）
/// - UDP 消息截断自动切换 TCP
/// - Via 头部自动添加
/// - DNS 解析集成
/// - 连接池管理
/// - 接收消息流
pub struct TransportManager {
    /// 传输层配置
    config: TransportConfig,
    /// TLS 配置
    tls_config: TlsConfig,
    /// UDP 传输
    udp: Option<Arc<UdpTransport>>,
    /// TCP 监听器
    tcp_listener: Option<Arc<TcpListener>>,
    /// 连接池
    connection_pool: Arc<Mutex<ConnectionPool>>,
    /// DNS 解析器
    dns_resolver: Arc<dyn DnsResolver>,
    /// 消息解析器
    #[allow(dead_code)]
    message_parser: MessageParser,
    /// 消息构建器
    message_builder: MessageBuilder,
    /// 运行指标
    metrics: Arc<SipMetrics>,
    /// 传输事件接收端
    event_rx: Option<mpsc::Receiver<TransportEvent>>,
    /// 传输事件发送端
    event_tx: mpsc::Sender<TransportEvent>,
    /// 是否已启动
    running: bool,
}

impl TransportManager {
    /// 创建新的传输管理器
    ///
    /// # 参数
    ///
    /// - `config` - 传输层配置
    /// - `tls_config` - TLS 配置
    /// - `metrics` - 运行指标收集器
    pub fn new(config: TransportConfig, tls_config: TlsConfig, metrics: Arc<SipMetrics>) -> Self {
        let (event_tx, event_rx) = mpsc::channel(1024);
        let idle_timeout = Duration::from_secs(config.connection_idle_timeout);

        Self {
            config,
            tls_config,
            udp: None,
            tcp_listener: None,
            connection_pool: Arc::new(Mutex::new(ConnectionPool::new(idle_timeout))),
            dns_resolver: Arc::new(SystemDnsResolver::new()),
            message_parser: MessageParser::new(SIP_DEFAULT_MAX_MESSAGE_SIZE),
            message_builder: MessageBuilder::new(),
            metrics,
            event_rx: Some(event_rx),
            event_tx,
            running: false,
        }
    }

    /// 使用自定义 DNS 解析器创建传输管理器
    pub fn with_dns_resolver(
        config: TransportConfig,
        tls_config: TlsConfig,
        metrics: Arc<SipMetrics>,
        dns_resolver: Arc<dyn DnsResolver>,
    ) -> Self {
        let (event_tx, event_rx) = mpsc::channel(1024);
        let idle_timeout = Duration::from_secs(config.connection_idle_timeout);

        Self {
            config,
            tls_config,
            udp: None,
            tcp_listener: None,
            connection_pool: Arc::new(Mutex::new(ConnectionPool::new(idle_timeout))),
            dns_resolver,
            message_parser: MessageParser::new(SIP_DEFAULT_MAX_MESSAGE_SIZE),
            message_builder: MessageBuilder::new(),
            metrics,
            event_rx: Some(event_rx),
            event_tx,
            running: false,
        }
    }

    /// 启动所有配置的传输监听
    ///
    /// 根据 `TransportConfig` 启用相应的传输协议：
    /// - `udp_enabled` → 启动 UDP 监听
    /// - `tcp_enabled` → 启动 TCP 监听
    /// - `tls_enabled` → 启动 TLS 监听（需要 `tls-rustls` feature）
    ///
    /// # 参数
    ///
    /// - `bind_addr` - 绑定地址（如 `0.0.0.0:5060`）
    ///
    /// # 错误
    ///
    /// 返回 `TransportError` 表示启动失败。
    pub async fn start(&mut self, bind_addr: SocketAddr) -> Result<(), TransportError> {
        if self.running {
            tracing::warn!("TransportManager: already running");
            return Ok(());
        }

        // 启动 UDP
        if self.config.udp_enabled {
            match UdpTransport::bind(bind_addr).await {
                Ok(udp) => {
                    let udp = Arc::new(udp);
                    tracing::info!("TransportManager: UDP listening on {}", udp.local_addr());

                    // 启动 UDP 接收循环
                    let udp_clone = Arc::clone(&udp);
                    let event_tx = self.event_tx.clone();
                    let max_size = self.config.max_message_size;
                    tokio::spawn(async move {
                        udp_clone.receive_loop(event_tx, max_size).await;
                    });

                    self.udp = Some(udp);
                }
                Err(e) => {
                    tracing::error!("TransportManager: UDP bind failed: {}", e);
                    return Err(e);
                }
            }
        }

        // 启动 TCP
        if self.config.tcp_enabled {
            match TcpListener::bind(bind_addr).await {
                Ok(listener) => {
                    let listener = Arc::new(listener);
                    tracing::info!(
                        "TransportManager: TCP listening on {}",
                        listener.local_addr()
                    );

                    // 启动 TCP 接受连接循环
                    let event_tx = self.event_tx.clone();
                    let max_size = self.config.max_message_size;
                    let pool = Arc::clone(&self.connection_pool);
                    let (conn_tx, mut conn_rx) = mpsc::channel::<Arc<Mutex<TcpWriteStream>>>(100);

                    // 启动连接池更新任务
                    let pool_clone = Arc::clone(&pool);
                    tokio::spawn(async move {
                        while let Some(write_stream) = conn_rx.recv().await {
                            let addr = {
                                let ws = write_stream.lock().await;
                                ws.peer_addr()
                            };
                            let mut pool = pool_clone.lock().await;
                            pool.add_tcp_connection(addr, write_stream);
                        }
                    });

                    // 启动接受连接循环
                    let listener_clone = Arc::clone(&listener);
                    tokio::spawn(async move {
                        listener_clone
                            .accept_loop(event_tx, max_size, conn_tx)
                            .await;
                    });

                    self.tcp_listener = Some(listener);
                }
                Err(e) => {
                    tracing::error!("TransportManager: TCP bind failed: {}", e);
                    return Err(e);
                }
            }
        }

        // 启动连接池清理任务
        let pool = Arc::clone(&self.connection_pool);
        let idle_timeout = Duration::from_secs(self.config.connection_idle_timeout);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(idle_timeout);
            loop {
                interval.tick().await;
                let mut pool = pool.lock().await;
                let closed = pool.cleanup_idle_connections();
                for (addr, proto) in closed {
                    tracing::debug!(
                        "ConnectionPool: closed idle {} connection to {}",
                        proto,
                        addr
                    );
                }
            }
        });

        self.running = true;
        tracing::info!("TransportManager: started");
        Ok(())
    }

    /// 停止所有传输
    pub async fn stop(&mut self) {
        if !self.running {
            return;
        }

        // 关闭 UDP
        if let Some(ref udp) = self.udp {
            let _ = udp.close().await;
        }
        self.udp = None;

        // 关闭 TCP 监听器
        self.tcp_listener = None;

        // 关闭所有连接
        self.connection_pool.lock().await.close_all();

        self.running = false;
        tracing::info!("TransportManager: stopped");
    }

    /// 发送 SIP 消息
    ///
    /// 自动选择传输协议、自动添加 Via 头部、自动处理 DNS 解析。
    ///
    /// # 传输协议选择规则
    ///
    /// - `sips:` URI → TLS 传输
    /// - `transport=tcp` 参数 → TCP 传输
    /// - `transport=tls` 参数 → TLS 传输
    /// - 默认 → UDP 传输
    /// - UDP 消息超过 MTU 限制 → 自动切换 TCP
    ///
    /// # 参数
    ///
    /// - `message` - 要发送的 SIP 消息
    /// - `target_uri` - 目标 SIP URI 字符串
    ///
    /// # 错误
    ///
    /// 返回 `TransportError` 表示发送失败。
    pub async fn send(&self, message: &SipMessage, target_uri: &str) -> Result<(), TransportError> {
        // 1. 解析目标 URI
        let uri =
            siprs_message::SipUri::parse(target_uri).map_err(|e| TransportError::SendFailed {
                reason: format!("invalid target URI: {}", e),
            })?;

        // 2. 确定传输协议
        let transport = self.determine_transport(&uri);

        // 3. 序列化消息（自动添加 Via 头部）
        let mut msg = message.clone();
        self.add_via_header(&mut msg, transport);
        let bytes = self
            .message_builder
            .build(&msg)
            .map_err(|e| TransportError::SendFailed {
                reason: format!("message serialization failed: {}", e),
            })?;

        // 4. 检查 UDP 消息大小限制
        if transport == TransportProtocol::Udp && bytes.len() > self.config.udp_mtu_limit {
            tracing::warn!(
                "TransportManager: UDP message too large ({} > {}), switching to TCP",
                bytes.len(),
                self.config.udp_mtu_limit
            );
            return self.send_via_tcp(&bytes, &uri).await;
        }

        // 5. 解析目标地址
        let host_str = uri.host.as_str();
        let port = uri.port.unwrap_or(transport.default_port());
        let addrs = self.resolve_target(&host_str, port, transport).await?;

        // 6. 根据传输协议发送
        match transport {
            TransportProtocol::Udp => self.send_via_udp(&bytes, &addrs).await,
            TransportProtocol::Tcp => self.send_via_tcp(&bytes, &uri).await,
            TransportProtocol::Tls => self.send_via_tls(&bytes, &uri).await,
            _ => Err(TransportError::SendFailed {
                reason: format!("unsupported transport: {}", transport),
            }),
        }
    }

    /// 发送原始字节消息到指定地址
    ///
    /// 直接发送已序列化的消息，不进行 URI 解析或 Via 头部添加。
    ///
    /// # 参数
    ///
    /// - `message` - 序列化后的消息字节
    /// - `addr` - 目标地址
    /// - `transport` - 传输协议
    pub async fn send_raw(
        &self,
        message: &[u8],
        addr: SocketAddr,
        transport: TransportProtocol,
    ) -> Result<(), TransportError> {
        match transport {
            TransportProtocol::Udp => {
                if let Some(ref udp) = self.udp {
                    udp.send_message(message, addr).await?;
                    self.metrics.inc_udp_messages();
                    self.metrics.inc_messages_sent();
                    Ok(())
                } else {
                    Err(TransportError::SendFailed {
                        reason: "UDP transport not available".to_string(),
                    })
                }
            }
            TransportProtocol::Tcp => {
                // 尝试从连接池获取连接
                let mut pool = self.connection_pool.lock().await;
                if let Some(conn) = pool.get_tcp_connection(&addr) {
                    let mut ws = conn.lock().await;
                    use bytes::BytesMut;
                    ws.send_raw(BytesMut::from(message)).await?;
                    self.metrics.inc_tcp_messages();
                    self.metrics.inc_messages_sent();
                    return Ok(());
                }
                drop(pool);

                // 创建新连接
                let conn = TcpConnection::connect(addr, self.config.max_message_size).await?;
                let (_read_stream, mut write_stream) = conn.into_split();
                use bytes::BytesMut;
                write_stream.send_raw(BytesMut::from(message)).await?;
                // 将写入流添加到连接池
                self.connection_pool
                    .lock()
                    .await
                    .add_tcp_connection(addr, Arc::new(Mutex::new(write_stream)));
                self.metrics.inc_tcp_messages();
                self.metrics.inc_messages_sent();
                Ok(())
            }
            _ => Err(TransportError::SendFailed {
                reason: format!("unsupported transport: {}", transport),
            }),
        }
    }

    /// 获取接收消息的异步流
    ///
    /// 返回一个通道接收端，可以从中读取传输层接收到的消息。
    ///
    /// # 注意
    ///
    /// 此方法只能调用一次，后续调用将返回 None。
    pub fn message_stream(&mut self) -> Option<mpsc::Receiver<TransportEvent>> {
        self.event_rx.take()
    }

    /// 获取传输事件发送端的克隆
    ///
    /// 用于外部组件向传输管理器注入事件。
    pub fn event_sender(&self) -> mpsc::Sender<TransportEvent> {
        self.event_tx.clone()
    }

    /// 获取 UDP 传输的本地地址
    pub fn udp_local_addr(&self) -> Option<SocketAddr> {
        self.udp.as_ref().map(|u| u.local_addr())
    }

    /// 获取 TCP 监听器的本地地址
    pub fn tcp_local_addr(&self) -> Option<SocketAddr> {
        self.tcp_listener.as_ref().map(|l| l.local_addr())
    }

    /// 判断传输管理器是否正在运行
    pub fn is_running(&self) -> bool {
        self.running
    }

    // ========================================================================
    // 内部方法
    // ========================================================================

    /// 根据目标 URI 确定传输协议
    ///
    /// 选择规则：
    /// - `sips:` URI → TLS
    /// - `transport=tcp` 参数 → TCP
    /// - `transport=tls` 参数 → TLS
    /// - 默认 → UDP
    fn determine_transport(&self, uri: &siprs_message::SipUri) -> TransportProtocol {
        // sips: URI 强制使用 TLS
        if uri.scheme == siprs_message::UriScheme::Sips {
            return TransportProtocol::Tls;
        }

        // 检查 transport 参数
        if let Some(transport_param) = uri.transport() {
            match transport_param.to_lowercase().as_str() {
                "tcp" => return TransportProtocol::Tcp,
                "tls" => return TransportProtocol::Tls,
                "udp" => return TransportProtocol::Udp,
                _ => {}
            }
        }

        // 默认使用 UDP
        TransportProtocol::Udp
    }

    /// 自动添加 Via 头部
    ///
    /// 如果消息中不存在 Via 头部，自动添加一个。
    fn add_via_header(&self, message: &mut SipMessage, transport: TransportProtocol) {
        // 检查是否已有 Via 头部
        if message.headers().contains(&HeaderName::Via) {
            return;
        }

        // 获取本地地址
        let local_host = self
            .udp_local_addr()
            .map(|a| a.ip().to_string())
            .unwrap_or_else(|| "0.0.0.0".to_string());

        let local_port = self.udp_local_addr().map(|a| a.port()).unwrap_or(5060);

        let host = Host::Domain(local_host);
        let via = ViaHeader::new(transport, host, Some(local_port));

        message
            .headers_mut()
            .insert(HeaderName::Via, HeaderValue::Via(via));
    }

    /// 解析目标地址
    async fn resolve_target(
        &self,
        host: &str,
        port: u16,
        transport: TransportProtocol,
    ) -> Result<Vec<SocketAddr>, TransportError> {
        // 尝试直接解析为 IP 地址
        if let Ok(addr) = host.parse::<std::net::IpAddr>() {
            return Ok(vec![SocketAddr::new(addr, port)]);
        }

        // 尝试解析为 SocketAddr
        if let Ok(addr) = format!("{}:{}", host, port).parse::<SocketAddr>() {
            return Ok(vec![addr]);
        }

        // 使用 DNS 解析
        self.dns_resolver
            .resolve(host, transport)
            .await
            .map_err(|e| TransportError::ConnectionFailed {
                addr: host.to_string(),
                reason: format!("DNS resolution failed: {}", e),
            })
    }

    /// 通过 UDP 发送消息
    async fn send_via_udp(&self, bytes: &[u8], addrs: &[SocketAddr]) -> Result<(), TransportError> {
        if let Some(ref udp) = self.udp {
            for addr in addrs {
                match udp.send_message(bytes, *addr).await {
                    Ok(()) => {
                        self.metrics.inc_udp_messages();
                        self.metrics.inc_messages_sent();
                        tracing::debug!("TransportManager: sent UDP message to {}", addr);
                        return Ok(());
                    }
                    Err(e) => {
                        tracing::warn!("TransportManager: UDP send to {} failed: {}", addr, e);
                        continue;
                    }
                }
            }
            Err(TransportError::SendFailed {
                reason: "all UDP send attempts failed".to_string(),
            })
        } else {
            Err(TransportError::SendFailed {
                reason: "UDP transport not available".to_string(),
            })
        }
    }

    /// 通过 TCP 发送消息
    async fn send_via_tcp(
        &self,
        bytes: &[u8],
        uri: &siprs_message::SipUri,
    ) -> Result<(), TransportError> {
        let host_str = uri.host.as_str();
        let port = uri.port.unwrap_or(TransportProtocol::Tcp.default_port());
        let addrs = self
            .resolve_target(&host_str, port, TransportProtocol::Tcp)
            .await?;

        for addr in &addrs {
            // 尝试从连接池获取连接
            let mut pool = self.connection_pool.lock().await;
            if let Some(conn) = pool.get_tcp_connection(addr) {
                let mut conn = conn.lock().await;
                use bytes::BytesMut;
                match conn.send_raw(BytesMut::from(bytes)).await {
                    Ok(()) => {
                        self.metrics.inc_tcp_messages();
                        self.metrics.inc_messages_sent();
                        tracing::debug!("TransportManager: sent TCP message to {}", addr);
                        return Ok(());
                    }
                    Err(e) => {
                        tracing::warn!("TransportManager: TCP send to {} failed: {}", addr, e);
                        pool.remove_tcp_connection(addr);
                        continue;
                    }
                }
            }
            drop(pool);

            // 创建新连接
            match TcpConnection::connect(*addr, self.config.max_message_size).await {
                Ok(conn) => {
                    // 拆分为读写两半
                    let (_read_stream, write_stream) = conn.into_split();
                    use bytes::BytesMut;
                    let mut ws = write_stream;
                    match ws.send_raw(BytesMut::from(bytes)).await {
                        Ok(()) => {
                            // 将写入流添加到连接池
                            let write_stream = Arc::new(Mutex::new(ws));
                            self.connection_pool
                                .lock()
                                .await
                                .add_tcp_connection(*addr, write_stream);
                            self.metrics.inc_tcp_messages();
                            self.metrics.inc_messages_sent();
                            tracing::debug!("TransportManager: sent TCP message to {}", addr);
                            return Ok(());
                        }
                        Err(e) => {
                            tracing::warn!("TransportManager: TCP send to {} failed: {}", addr, e);
                            continue;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("TransportManager: TCP connect to {} failed: {}", addr, e);
                    continue;
                }
            }
        }

        Err(TransportError::SendFailed {
            reason: "all TCP send attempts failed".to_string(),
        })
    }

    /// 通过 TLS 发送消息
    async fn send_via_tls(
        &self,
        bytes: &[u8],
        uri: &siprs_message::SipUri,
    ) -> Result<(), TransportError> {
        #[cfg(feature = "tls-rustls")]
        {
            let host_str = uri.host.as_str();
            let port = uri.port.unwrap_or(TransportProtocol::Tls.default_port());
            let addrs = self
                .resolve_target(&host_str, port, TransportProtocol::Tls)
                .await?;

            for addr in &addrs {
                // 从 URI host 提取 SNI 服务器名称
                let sni_name = uri.host.as_str().into_owned();
                match TlsConnection::connect(
                    *addr,
                    self.config.max_message_size,
                    self.tls_config.verify_certificate,
                    sni_name,
                )
                .await
                {
                    Ok(mut conn) => {
                        use bytes::BytesMut;
                        match conn.send_raw(BytesMut::from(bytes)).await {
                            Ok(()) => {
                                self.metrics.inc_tls_messages();
                                self.metrics.inc_messages_sent();
                                tracing::debug!("TransportManager: sent TLS message to {}", addr);
                                return Ok(());
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "TransportManager: TLS send to {} failed: {}",
                                    addr,
                                    e
                                );
                                continue;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("TransportManager: TLS connect to {} failed: {}", addr, e);
                        continue;
                    }
                }
            }

            Err(TransportError::SendFailed {
                reason: "all TLS send attempts failed".to_string(),
            })
        }

        #[cfg(not(feature = "tls-rustls"))]
        {
            let _ = (bytes, uri);
            Err(TransportError::SendFailed {
                reason: "TLS transport not available (tls-rustls feature not enabled)".to_string(),
            })
        }
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_manager() -> TransportManager {
        TransportManager::new(
            TransportConfig::default(),
            TlsConfig::default(),
            Arc::new(SipMetrics::new()),
        )
    }

    #[test]
    fn test_transport_manager_creation() {
        let manager = create_test_manager();
        assert!(!manager.is_running());
    }

    #[test]
    fn test_determine_transport_sips() {
        let manager = create_test_manager();
        let uri = siprs_message::SipUri::parse("sips:bob@example.com").unwrap();
        assert_eq!(manager.determine_transport(&uri), TransportProtocol::Tls);
    }

    #[test]
    fn test_determine_transport_tcp_param() {
        let manager = create_test_manager();
        let uri = siprs_message::SipUri::parse("sip:bob@example.com;transport=tcp").unwrap();
        assert_eq!(manager.determine_transport(&uri), TransportProtocol::Tcp);
    }

    #[test]
    fn test_determine_transport_udp_param() {
        let manager = create_test_manager();
        let uri = siprs_message::SipUri::parse("sip:bob@example.com;transport=udp").unwrap();
        assert_eq!(manager.determine_transport(&uri), TransportProtocol::Udp);
    }

    #[test]
    fn test_determine_transport_default() {
        let manager = create_test_manager();
        let uri = siprs_message::SipUri::parse("sip:bob@example.com").unwrap();
        assert_eq!(manager.determine_transport(&uri), TransportProtocol::Udp);
    }

    #[tokio::test]
    async fn test_transport_manager_start_stop() {
        let mut manager = create_test_manager();
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

        let result = manager.start(bind_addr).await;
        assert!(result.is_ok());
        assert!(manager.is_running());

        manager.stop().await;
        assert!(!manager.is_running());
    }

    #[tokio::test]
    async fn test_transport_manager_message_stream() {
        let mut manager = create_test_manager();
        let stream = manager.message_stream();
        assert!(stream.is_some());

        // 第二次调用应返回 None
        let stream2 = manager.message_stream();
        assert!(stream2.is_none());
    }

    #[test]
    fn test_add_via_header() {
        let manager = create_test_manager();

        // 创建一个没有 Via 头部的消息
        let uri = siprs_message::SipUri::parse("sip:bob@example.com").unwrap();
        let mut headers = siprs_message::HeaderCollection::new();
        headers.insert(
            HeaderName::CallId,
            HeaderValue::CallId(siprs_message::CallId("test@example.com".to_string())),
        );
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(siprs_message::CSeqHeader::new(1, siprs_message::Method::Invite)),
        );

        let mut message = SipMessage::Request(siprs_message::SipRequest {
            request_line: siprs_message::RequestLine {
                method: siprs_message::Method::Invite,
                request_uri: uri,
                version: siprs_core::SipVersion,
            },
            headers,
            body: None,
        });

        manager.add_via_header(&mut message, TransportProtocol::Udp);

        // 验证 Via 头部已添加
        assert!(message.headers().contains(&HeaderName::Via));
    }

    #[test]
    fn test_add_via_header_existing() {
        let manager = create_test_manager();

        let uri = siprs_message::SipUri::parse("sip:bob@example.com").unwrap();
        let mut headers = siprs_message::HeaderCollection::new();
        let existing_via = ViaHeader::new(
            TransportProtocol::Udp,
            Host::Domain("existing.com".to_string()),
            Some(5060),
        );
        headers.insert(HeaderName::Via, HeaderValue::Via(existing_via));

        let mut message = SipMessage::Request(siprs_message::SipRequest {
            request_line: siprs_message::RequestLine {
                method: siprs_message::Method::Invite,
                request_uri: uri,
                version: siprs_core::SipVersion,
            },
            headers,
            body: None,
        });

        manager.add_via_header(&mut message, TransportProtocol::Udp);

        // 已有 Via 头部，不应再添加
        let via_count = message.headers().get_all(&HeaderName::Via).len();
        assert_eq!(via_count, 1);
    }
}
