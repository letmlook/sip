//! SIP Core metrics types
//!
//! 运行指标监控模块，使用 `AtomicU64` 无锁计数器跟踪 SIP 协议栈关键运行指标。
//! 支持消息计数、事务计数、对话计数、注册计数、传输计数五大类指标。

use std::sync::atomic::{AtomicU64, Ordering};

/// SIP 协议栈运行指标
///
/// 所有计数器使用 `AtomicU64`，无锁并发安全。
/// `SipMetrics` 自动实现 `Send + Sync`，可安全跨线程共享。
#[derive(Debug)]
pub struct SipMetrics {
    // 消息指标
    messages_received: AtomicU64,
    messages_sent: AtomicU64,
    parse_errors: AtomicU64,

    // 事务指标
    active_client_transactions: AtomicU64,
    active_server_transactions: AtomicU64,
    transactions_created: AtomicU64,
    transaction_timeouts: AtomicU64,

    // 对话指标
    active_dialogs: AtomicU64,
    dialogs_created: AtomicU64,

    // 注册指标
    active_registrations: AtomicU64,

    // 传输指标
    udp_messages: AtomicU64,
    tcp_messages: AtomicU64,
    tls_messages: AtomicU64,
    active_connections: AtomicU64,
}

/// 运行指标快照
///
/// 包含某一时刻所有指标的当前值，用于上报和展示。
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    // 消息指标
    pub messages_received: u64,
    pub messages_sent: u64,
    pub parse_errors: u64,

    // 事务指标
    pub active_client_transactions: u64,
    pub active_server_transactions: u64,
    pub transactions_created: u64,
    pub transaction_timeouts: u64,

    // 对话指标
    pub active_dialogs: u64,
    pub dialogs_created: u64,

    // 注册指标
    pub active_registrations: u64,

    // 传输指标
    pub udp_messages: u64,
    pub tcp_messages: u64,
    pub tls_messages: u64,
    pub active_connections: u64,
}

impl SipMetrics {
    /// 创建新的指标收集器，所有计数器初始化为 0
    pub fn new() -> Self {
        Self {
            messages_received: AtomicU64::new(0),
            messages_sent: AtomicU64::new(0),
            parse_errors: AtomicU64::new(0),

            active_client_transactions: AtomicU64::new(0),
            active_server_transactions: AtomicU64::new(0),
            transactions_created: AtomicU64::new(0),
            transaction_timeouts: AtomicU64::new(0),

            active_dialogs: AtomicU64::new(0),
            dialogs_created: AtomicU64::new(0),

            active_registrations: AtomicU64::new(0),

            udp_messages: AtomicU64::new(0),
            tcp_messages: AtomicU64::new(0),
            tls_messages: AtomicU64::new(0),
            active_connections: AtomicU64::new(0),
        }
    }

    // ---- 消息指标方法 ----

    /// 递增接收消息计数
    pub fn inc_messages_received(&self) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
    }

    /// 递增发送消息计数
    pub fn inc_messages_sent(&self) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
    }

    /// 递增解析错误计数
    pub fn inc_parse_errors(&self) {
        self.parse_errors.fetch_add(1, Ordering::Relaxed);
    }

    // ---- 事务指标方法 ----

    /// 递增活跃客户端事务计数
    pub fn inc_active_client_transactions(&self) {
        self.active_client_transactions
            .fetch_add(1, Ordering::Relaxed);
    }

    /// 递减活跃客户端事务计数
    ///
    /// 使用 `fetch_sub` 递减，`debug_assert` 在 debug 模式下检测 inc/dec 不匹配。
    /// release 模式下若发生下溢，计数器会回绕为极大值，应在 `snapshot()` 中处理。
    pub fn dec_active_client_transactions(&self) {
        let prev = self
            .active_client_transactions
            .fetch_sub(1, Ordering::Relaxed);
        debug_assert!(prev > 0, "Metrics underflow: active_client_transactions");
    }

    /// 递增活跃服务端事务计数
    pub fn inc_active_server_transactions(&self) {
        self.active_server_transactions
            .fetch_add(1, Ordering::Relaxed);
    }

    /// 递减活跃服务端事务计数
    ///
    /// 使用 `fetch_sub` 递减，`debug_assert` 在 debug 模式下检测 inc/dec 不匹配。
    pub fn dec_active_server_transactions(&self) {
        let prev = self
            .active_server_transactions
            .fetch_sub(1, Ordering::Relaxed);
        debug_assert!(prev > 0, "Metrics underflow: active_server_transactions");
    }

    /// 递增已创建事务计数
    pub fn inc_transactions_created(&self) {
        self.transactions_created.fetch_add(1, Ordering::Relaxed);
    }

    /// 递增事务超时计数
    pub fn inc_transaction_timeouts(&self) {
        self.transaction_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    // ---- 对话指标方法 ----

    /// 递增活跃对话计数
    pub fn inc_active_dialogs(&self) {
        self.active_dialogs.fetch_add(1, Ordering::Relaxed);
    }

    /// 递减活跃对话计数
    ///
    /// 使用 `fetch_sub` 递减，`debug_assert` 在 debug 模式下检测 inc/dec 不匹配。
    pub fn dec_active_dialogs(&self) {
        let prev = self.active_dialogs.fetch_sub(1, Ordering::Relaxed);
        debug_assert!(prev > 0, "Metrics underflow: active_dialogs");
    }

    /// 递增已创建对话计数
    pub fn inc_dialogs_created(&self) {
        self.dialogs_created.fetch_add(1, Ordering::Relaxed);
    }

    // ---- 注册指标方法 ----

    /// 递增活跃注册计数
    pub fn inc_active_registrations(&self) {
        self.active_registrations.fetch_add(1, Ordering::Relaxed);
    }

    /// 递减活跃注册计数
    ///
    /// 使用 `fetch_sub` 递减，`debug_assert` 在 debug 模式下检测 inc/dec 不匹配。
    pub fn dec_active_registrations(&self) {
        let prev = self.active_registrations.fetch_sub(1, Ordering::Relaxed);
        debug_assert!(prev > 0, "Metrics underflow: active_registrations");
    }

    // ---- 传输指标方法 ----

    /// 递增 UDP 消息计数
    pub fn inc_udp_messages(&self) {
        self.udp_messages.fetch_add(1, Ordering::Relaxed);
    }

    /// 递增 TCP 消息计数
    pub fn inc_tcp_messages(&self) {
        self.tcp_messages.fetch_add(1, Ordering::Relaxed);
    }

    /// 递增 TLS 消息计数
    pub fn inc_tls_messages(&self) {
        self.tls_messages.fetch_add(1, Ordering::Relaxed);
    }

    /// 递增活跃连接计数
    pub fn inc_active_connections(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// 递减活跃连接计数
    ///
    /// 使用 `fetch_sub` 递减，`debug_assert` 在 debug 模式下检测 inc/dec 不匹配。
    pub fn dec_active_connections(&self) {
        let prev = self.active_connections.fetch_sub(1, Ordering::Relaxed);
        debug_assert!(prev > 0, "Metrics underflow: active_connections");
    }

    /// 获取当前所有指标的快照
    ///
    /// 使用 `Ordering::SeqCst` 确保一致性读取。
    /// 对于可能因 `fetch_sub` 下溢回绕的计数器，使用 `clamp_underflow` 防御性处理。
    pub fn snapshot(&self) -> MetricsSnapshot {
        /// 防御性处理：若 `fetch_sub` 导致无符号整数下溢回绕为极大值，
        /// 将其截断为 0。仅当 inc/dec 调用不匹配时才会触发。
        fn clamp_underflow(value: u64) -> u64 {
            // 下溢回绕后的值接近 u64::MAX，远超任何合理的活跃计数。
            // 使用阈值判断：若值超过 i64::MAX 则视为下溢回绕，返回 0。
            if value > i64::MAX as u64 {
                0
            } else {
                value
            }
        }

        MetricsSnapshot {
            messages_received: self.messages_received.load(Ordering::SeqCst),
            messages_sent: self.messages_sent.load(Ordering::SeqCst),
            parse_errors: self.parse_errors.load(Ordering::SeqCst),

            active_client_transactions: clamp_underflow(
                self.active_client_transactions.load(Ordering::SeqCst),
            ),
            active_server_transactions: clamp_underflow(
                self.active_server_transactions.load(Ordering::SeqCst),
            ),
            transactions_created: self.transactions_created.load(Ordering::SeqCst),
            transaction_timeouts: self.transaction_timeouts.load(Ordering::SeqCst),

            active_dialogs: clamp_underflow(self.active_dialogs.load(Ordering::SeqCst)),
            dialogs_created: self.dialogs_created.load(Ordering::SeqCst),

            active_registrations: clamp_underflow(self.active_registrations.load(Ordering::SeqCst)),

            udp_messages: self.udp_messages.load(Ordering::SeqCst),
            tcp_messages: self.tcp_messages.load(Ordering::SeqCst),
            tls_messages: self.tls_messages.load(Ordering::SeqCst),
            active_connections: clamp_underflow(self.active_connections.load(Ordering::SeqCst)),
        }
    }
}

impl Default for SipMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_new_metrics_all_zero() {
        let metrics = SipMetrics::new();
        let snap = metrics.snapshot();
        assert_eq!(snap.messages_received, 0);
        assert_eq!(snap.messages_sent, 0);
        assert_eq!(snap.parse_errors, 0);
        assert_eq!(snap.active_client_transactions, 0);
        assert_eq!(snap.active_server_transactions, 0);
        assert_eq!(snap.transactions_created, 0);
        assert_eq!(snap.transaction_timeouts, 0);
        assert_eq!(snap.active_dialogs, 0);
        assert_eq!(snap.dialogs_created, 0);
        assert_eq!(snap.active_registrations, 0);
        assert_eq!(snap.udp_messages, 0);
        assert_eq!(snap.tcp_messages, 0);
        assert_eq!(snap.tls_messages, 0);
        assert_eq!(snap.active_connections, 0);
    }

    #[test]
    fn test_default_metrics() {
        let metrics = SipMetrics::default();
        let snap = metrics.snapshot();
        assert_eq!(snap.messages_received, 0);
    }

    #[test]
    fn test_message_counters() {
        let metrics = SipMetrics::new();

        metrics.inc_messages_received();
        metrics.inc_messages_received();
        metrics.inc_messages_received();
        metrics.inc_messages_sent();
        metrics.inc_parse_errors();
        metrics.inc_parse_errors();

        let snap = metrics.snapshot();
        assert_eq!(snap.messages_received, 3);
        assert_eq!(snap.messages_sent, 1);
        assert_eq!(snap.parse_errors, 2);
    }

    #[test]
    fn test_transaction_counters() {
        let metrics = SipMetrics::new();

        metrics.inc_active_client_transactions();
        metrics.inc_active_client_transactions();
        metrics.dec_active_client_transactions();
        metrics.inc_active_server_transactions();
        metrics.inc_transactions_created();
        metrics.inc_transactions_created();
        metrics.inc_transaction_timeouts();

        let snap = metrics.snapshot();
        assert_eq!(snap.active_client_transactions, 1);
        assert_eq!(snap.active_server_transactions, 1);
        assert_eq!(snap.transactions_created, 2);
        assert_eq!(snap.transaction_timeouts, 1);
    }

    #[test]
    fn test_dialog_counters() {
        let metrics = SipMetrics::new();

        metrics.inc_active_dialogs();
        metrics.inc_active_dialogs();
        metrics.inc_active_dialogs();
        metrics.dec_active_dialogs();
        metrics.inc_dialogs_created();
        metrics.inc_dialogs_created();

        let snap = metrics.snapshot();
        assert_eq!(snap.active_dialogs, 2);
        assert_eq!(snap.dialogs_created, 2);
    }

    #[test]
    fn test_registration_counters() {
        let metrics = SipMetrics::new();

        metrics.inc_active_registrations();
        metrics.inc_active_registrations();
        metrics.dec_active_registrations();

        let snap = metrics.snapshot();
        assert_eq!(snap.active_registrations, 1);
    }

    #[test]
    fn test_transport_counters() {
        let metrics = SipMetrics::new();

        metrics.inc_udp_messages();
        metrics.inc_udp_messages();
        metrics.inc_udp_messages();
        metrics.inc_tcp_messages();
        metrics.inc_tcp_messages();
        metrics.inc_tls_messages();
        metrics.inc_active_connections();
        metrics.inc_active_connections();
        metrics.inc_active_connections();
        metrics.dec_active_connections();

        let snap = metrics.snapshot();
        assert_eq!(snap.udp_messages, 3);
        assert_eq!(snap.tcp_messages, 2);
        assert_eq!(snap.tls_messages, 1);
        assert_eq!(snap.active_connections, 2);
    }

    #[test]
    fn test_snapshot_clone() {
        let metrics = SipMetrics::new();
        metrics.inc_messages_received();
        metrics.inc_messages_sent();

        let snap = metrics.snapshot();
        let cloned = snap.clone();

        assert_eq!(cloned.messages_received, 1);
        assert_eq!(cloned.messages_sent, 1);
    }

    #[test]
    fn test_concurrent_access() {
        let metrics = Arc::new(SipMetrics::new());
        let mut handles = vec![];

        // 启动多个线程并发递增计数器
        for _ in 0..10 {
            let m = Arc::clone(&metrics);
            handles.push(thread::spawn(move || {
                for _ in 0..1000 {
                    m.inc_messages_received();
                    m.inc_messages_sent();
                    m.inc_active_client_transactions();
                    m.inc_active_dialogs();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let snap = metrics.snapshot();
        assert_eq!(snap.messages_received, 10_000);
        assert_eq!(snap.messages_sent, 10_000);
        assert_eq!(snap.active_client_transactions, 10_000);
        assert_eq!(snap.active_dialogs, 10_000);
    }

    #[test]
    fn test_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SipMetrics>();
    }

    #[test]
    fn test_snapshot_clamp_underflow() {
        // 验证 clamp_underflow 辅助函数的正确性
        // 正常值应原样返回
        fn clamp_underflow(value: u64) -> u64 {
            if value > i64::MAX as u64 {
                0
            } else {
                value
            }
        }

        // 正常值
        assert_eq!(clamp_underflow(0), 0);
        assert_eq!(clamp_underflow(1), 1);
        assert_eq!(clamp_underflow(1000), 1000);
        assert_eq!(clamp_underflow(i64::MAX as u64), i64::MAX as u64);

        // 下溢回绕值（u64::MAX 附近）应截断为 0
        assert_eq!(clamp_underflow(u64::MAX), 0);
        assert_eq!(clamp_underflow(u64::MAX - 1), 0);
        assert_eq!(clamp_underflow((i64::MAX as u64) + 1), 0);
    }

    #[test]
    fn test_dec_after_inc_matches() {
        // 验证正常的 inc/dec 配对不会触发 debug_assert
        let metrics = SipMetrics::new();
        metrics.inc_active_client_transactions();
        metrics.dec_active_client_transactions();
        assert_eq!(metrics.snapshot().active_client_transactions, 0);

        metrics.inc_active_dialogs();
        metrics.dec_active_dialogs();
        assert_eq!(metrics.snapshot().active_dialogs, 0);

        metrics.inc_active_server_transactions();
        metrics.dec_active_server_transactions();
        assert_eq!(metrics.snapshot().active_server_transactions, 0);

        metrics.inc_active_registrations();
        metrics.dec_active_registrations();
        assert_eq!(metrics.snapshot().active_registrations, 0);

        metrics.inc_active_connections();
        metrics.dec_active_connections();
        assert_eq!(metrics.snapshot().active_connections, 0);
    }
}
