//! SIP 事务定时器管理器
//!
//! 实现 RFC 3261 定义的所有事务定时器（Timer A~K），
//! 支持指数退避重传和定时器取消。

use std::collections::HashMap;
use std::sync::Arc;

use sip_core::config::TransactionConfig;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio::time::Duration;

use crate::event::{TimerEvent, TransactionId};

// ============================================================================
// TimerHandle - 定时器句柄
// ============================================================================

/// 定时器句柄，用于取消定时器
struct TimerHandle {
    /// 取消令牌
    cancel_tx: tokio::sync::watch::Sender<bool>,
    /// 异步任务句柄
    _handle: JoinHandle<()>,
}

// ============================================================================
// TimerManager - 定时器管理器
// ============================================================================

/// SIP 事务定时器管理器
///
/// 管理所有事务定时器的启动、触发和取消。
/// 定时器超时后通过 `timer_tx` 通道发送 `TimerEvent`。
pub struct TimerManager {
    /// 事务层配置
    config: TransactionConfig,
    /// 定时器事件发送端
    timer_tx: mpsc::UnboundedSender<TimerEvent>,
    /// 每个事务的定时器句柄列表
    timer_handles: HashMap<TransactionId, Vec<TimerHandle>>,
}

impl TimerManager {
    /// 创建新的定时器管理器
    ///
    /// # 参数
    ///
    /// - `config` - 事务层配置（T1、T2、T4 等定时器值）
    /// - `timer_tx` - 定时器事件发送端
    pub fn new(config: TransactionConfig, timer_tx: mpsc::UnboundedSender<TimerEvent>) -> Self {
        Self {
            config,
            timer_tx,
            timer_handles: HashMap::new(),
        }
    }

    /// 启动一次性定时器
    ///
    /// 在指定延迟后发送定时器事件。
    pub fn start_timer(&mut self, event: TimerEvent, delay: Duration) {
        let transaction_id = match &event {
            TimerEvent::TimerA { transaction_id } => transaction_id.clone(),
            TimerEvent::TimerB { transaction_id } => transaction_id.clone(),
            TimerEvent::TimerD { transaction_id } => transaction_id.clone(),
            TimerEvent::TimerE { transaction_id } => transaction_id.clone(),
            TimerEvent::TimerF { transaction_id } => transaction_id.clone(),
            TimerEvent::TimerG { transaction_id } => transaction_id.clone(),
            TimerEvent::TimerH { transaction_id } => transaction_id.clone(),
            TimerEvent::TimerI { transaction_id } => transaction_id.clone(),
            TimerEvent::TimerJ { transaction_id } => transaction_id.clone(),
            TimerEvent::TimerK { transaction_id } => transaction_id.clone(),
        };

        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        let timer_tx = self.timer_tx.clone();

        let handle = tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(delay) => {
                    let _ = timer_tx.send(event);
                }
                _ = cancel_rx.changed() => {
                    // 定时器被取消
                    tracing::trace!("Timer cancelled for transaction");
                }
            }
        });

        self.timer_handles
            .entry(transaction_id)
            .or_default()
            .push(TimerHandle {
                cancel_tx,
                _handle: handle,
            });
    }

    /// 启动重传定时器（指数退避）
    ///
    /// 初始延迟为 `initial_delay`，每次超时后延迟翻倍，
    /// 最大延迟为 `max_delay`。超时后发送定时器事件，
    /// 然后以翻倍的延迟重新启动定时器，直到被取消。
    pub fn start_retransmit_timer(
        &mut self,
        event_factory: Box<dyn Fn(TransactionId) -> TimerEvent + Send + Sync>,
        transaction_id: TransactionId,
        initial_delay: Duration,
        max_delay: Duration,
    ) {
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        let timer_tx = self.timer_tx.clone();
        let tid_for_entry = transaction_id.clone();
        let tid_for_log = transaction_id.clone();

        let handle = tokio::spawn(async move {
            let mut current_delay = initial_delay;
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(current_delay) => {
                        let event = event_factory(transaction_id.clone());
                        let _ = timer_tx.send(event);

                        // 指数退避：延迟翻倍，最大不超过 max_delay
                        current_delay = (current_delay * 2).min(max_delay);
                    }
                    _ = cancel_rx.changed() => {
                        // 定时器被取消
                        tracing::trace!("Retransmit timer cancelled for transaction {}", tid_for_log);
                        break;
                    }
                }
            }
        });

        self.timer_handles
            .entry(tid_for_entry)
            .or_default()
            .push(TimerHandle {
                cancel_tx,
                _handle: handle,
            });
    }

    /// 取消指定事务的所有定时器
    pub fn cancel_timers(&mut self, transaction_id: &TransactionId) {
        if let Some(handles) = self.timer_handles.remove(transaction_id) {
            for handle in handles {
                let _ = handle.cancel_tx.send(true);
            }
            tracing::trace!("Cancelled all timers for transaction {}", transaction_id);
        }
    }

    /// 清理所有定时器
    pub fn cancel_all(&mut self) {
        for (_, handles) in self.timer_handles.drain() {
            for handle in handles {
                let _ = handle.cancel_tx.send(true);
            }
        }
    }

    // ========================================================================
    // 便捷方法：启动特定定时器
    // ========================================================================

    /// 启动 Timer A（INVITE 客户端请求重传，仅 UDP）
    ///
    /// 初始值 T1，每次超时翻倍，最大 T2。
    pub fn start_timer_a(&mut self, transaction_id: TransactionId) {
        let tid = transaction_id.clone();
        let initial = Duration::from_millis(self.config.t1);
        let max = Duration::from_millis(self.config.t2);

        self.start_retransmit_timer(
            Box::new(move |id| TimerEvent::TimerA { transaction_id: id }),
            tid,
            initial,
            max,
        );
    }

    /// 启动 Timer B（INVITE 客户端事务超时）
    ///
    /// 值 64*T1。
    pub fn start_timer_b(&mut self, transaction_id: TransactionId) {
        let tid = transaction_id.clone();
        let delay = Duration::from_millis(64 * self.config.t1);
        self.start_timer(
            TimerEvent::TimerB {
                transaction_id: tid,
            },
            delay,
        );
    }

    /// 启动 Timer D（INVITE 客户端等待延迟响应）
    ///
    /// UDP: 32s，可靠传输: 0s。
    pub fn start_timer_d(&mut self, transaction_id: TransactionId, reliable: bool) {
        if reliable {
            // 可靠传输不需要 Timer D，立即触发
            let event = TimerEvent::TimerD {
                transaction_id: transaction_id.clone(),
            };
            let _ = self.timer_tx.send(event);
        } else {
            let tid = transaction_id.clone();
            let delay = Duration::from_secs(32);
            self.start_timer(
                TimerEvent::TimerD {
                    transaction_id: tid,
                },
                delay,
            );
        }
    }

    /// 启动 Timer E（非 INVITE 客户端请求重传，仅 UDP）
    ///
    /// 初始值 T1，每次超时翻倍，最大 T2。
    pub fn start_timer_e(&mut self, transaction_id: TransactionId) {
        let tid = transaction_id.clone();
        let initial = Duration::from_millis(self.config.t1);
        let max = Duration::from_millis(self.config.t2);

        self.start_retransmit_timer(
            Box::new(move |id| TimerEvent::TimerE { transaction_id: id }),
            tid,
            initial,
            max,
        );
    }

    /// 启动 Timer F（非 INVITE 客户端事务超时）
    ///
    /// 值 64*T1。
    pub fn start_timer_f(&mut self, transaction_id: TransactionId) {
        let tid = transaction_id.clone();
        let delay = Duration::from_millis(64 * self.config.t1);
        self.start_timer(
            TimerEvent::TimerF {
                transaction_id: tid,
            },
            delay,
        );
    }

    /// 启动 Timer G（INVITE 服务端响应重传）
    ///
    /// 初始值 T1，每次超时翻倍，最大 T2。
    pub fn start_timer_g(&mut self, transaction_id: TransactionId) {
        let tid = transaction_id.clone();
        let initial = Duration::from_millis(self.config.t1);
        let max = Duration::from_millis(self.config.t2);

        self.start_retransmit_timer(
            Box::new(move |id| TimerEvent::TimerG { transaction_id: id }),
            tid,
            initial,
            max,
        );
    }

    /// 启动 Timer H（INVITE 服务端等待 ACK 超时）
    ///
    /// 值 64*T1。
    pub fn start_timer_h(&mut self, transaction_id: TransactionId) {
        let tid = transaction_id.clone();
        let delay = Duration::from_millis(64 * self.config.t1);
        self.start_timer(
            TimerEvent::TimerH {
                transaction_id: tid,
            },
            delay,
        );
    }

    /// 启动 Timer I（INVITE 服务端 Confirmed 状态等待）
    ///
    /// UDP: T4，可靠传输: 0s。
    pub fn start_timer_i(&mut self, transaction_id: TransactionId, reliable: bool) {
        if reliable {
            // 可靠传输不需要 Timer I，立即触发
            let event = TimerEvent::TimerI {
                transaction_id: transaction_id.clone(),
            };
            let _ = self.timer_tx.send(event);
        } else {
            let tid = transaction_id.clone();
            let delay = Duration::from_millis(self.config.t4);
            self.start_timer(
                TimerEvent::TimerI {
                    transaction_id: tid,
                },
                delay,
            );
        }
    }

    /// 启动 Timer J（非 INVITE 服务端事务超时）
    ///
    /// UDP: 64*T1，可靠传输: 0s。
    pub fn start_timer_j(&mut self, transaction_id: TransactionId, reliable: bool) {
        if reliable {
            // 可靠传输不需要 Timer J，立即触发
            let event = TimerEvent::TimerJ {
                transaction_id: transaction_id.clone(),
            };
            let _ = self.timer_tx.send(event);
        } else {
            let tid = transaction_id.clone();
            let delay = Duration::from_millis(64 * self.config.t1);
            self.start_timer(
                TimerEvent::TimerJ {
                    transaction_id: tid,
                },
                delay,
            );
        }
    }

    /// 启动 Timer K（非 INVITE 客户端等待响应重传）
    ///
    /// UDP: T4，可靠传输: 0s。
    pub fn start_timer_k(&mut self, transaction_id: TransactionId, reliable: bool) {
        if reliable {
            // 可靠传输不需要 Timer K，立即触发
            let event = TimerEvent::TimerK {
                transaction_id: transaction_id.clone(),
            };
            let _ = self.timer_tx.send(event);
        } else {
            let tid = transaction_id.clone();
            let delay = Duration::from_millis(self.config.t4);
            self.start_timer(
                TimerEvent::TimerK {
                    transaction_id: tid,
                },
                delay,
            );
        }
    }

    /// 获取配置引用
    pub fn config(&self) -> &TransactionConfig {
        &self.config
    }
}

// ============================================================================
// 线程安全包装
// ============================================================================

/// 线程安全的定时器管理器
pub type SharedTimerManager = Arc<Mutex<TimerManager>>;

/// 创建线程安全的定时器管理器
pub fn create_shared_timer_manager(
    config: TransactionConfig,
    timer_tx: mpsc::UnboundedSender<TimerEvent>,
) -> SharedTimerManager {
    Arc::new(Mutex::new(TimerManager::new(config, timer_tx)))
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::TimerEvent;

    #[tokio::test]
    async fn test_timer_fires() {
        let (timer_tx, mut timer_rx) = mpsc::unbounded_channel();
        let config = TransactionConfig::default();
        let mut manager = TimerManager::new(config, timer_tx);

        let tid = TransactionId("test-timer".to_string());
        manager.start_timer(
            TimerEvent::TimerB {
                transaction_id: tid.clone(),
            },
            Duration::from_millis(50),
        );

        // 等待定时器触发
        let event = tokio::time::timeout(Duration::from_millis(200), timer_rx.recv()).await;
        assert!(event.is_ok());
        let event = event.unwrap().unwrap();
        match event {
            TimerEvent::TimerB { transaction_id } => {
                assert_eq!(transaction_id, tid);
            }
            _ => panic!("Expected TimerB event"),
        }
    }

    #[tokio::test]
    async fn test_timer_cancel() {
        let (timer_tx, mut timer_rx) = mpsc::unbounded_channel();
        let config = TransactionConfig::default();
        let mut manager = TimerManager::new(config, timer_tx);

        let tid = TransactionId("test-cancel".to_string());
        manager.start_timer(
            TimerEvent::TimerB {
                transaction_id: tid.clone(),
            },
            Duration::from_millis(100),
        );

        // 立即取消
        manager.cancel_timers(&tid);

        // 确认定时器不会触发
        let result = tokio::time::timeout(Duration::from_millis(200), timer_rx.recv()).await;
        assert!(result.is_err() || result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_retransmit_timer() {
        let (timer_tx, mut timer_rx) = mpsc::unbounded_channel();
        let config = TransactionConfig {
            t1: 50,
            t2: 200,
            ..TransactionConfig::default()
        };
        let mut manager = TimerManager::new(config, timer_tx);

        let tid = TransactionId("test-retransmit".to_string());
        manager.start_timer_a(tid.clone());

        // 等待第一次触发（T1 = 50ms）
        let event = tokio::time::timeout(Duration::from_millis(150), timer_rx.recv()).await;
        assert!(event.is_ok());
        match event.unwrap().unwrap() {
            TimerEvent::TimerA { transaction_id } => {
                assert_eq!(transaction_id, tid);
            }
            _ => panic!("Expected TimerA event"),
        }

        // 取消定时器
        manager.cancel_timers(&tid);
    }

    #[tokio::test]
    async fn test_timer_d_reliable() {
        let (timer_tx, mut timer_rx) = mpsc::unbounded_channel();
        let config = TransactionConfig::default();
        let mut manager = TimerManager::new(config, timer_tx);

        let tid = TransactionId("test-timer-d".to_string());
        // 可靠传输：Timer D 立即触发
        manager.start_timer_d(tid.clone(), true);

        let event = tokio::time::timeout(Duration::from_millis(50), timer_rx.recv()).await;
        assert!(event.is_ok());
        match event.unwrap().unwrap() {
            TimerEvent::TimerD { transaction_id } => {
                assert_eq!(transaction_id, tid);
            }
            _ => panic!("Expected TimerD event"),
        }
    }

    #[tokio::test]
    async fn test_cancel_all() {
        let (timer_tx, _timer_rx) = mpsc::unbounded_channel();
        let config = TransactionConfig::default();
        let mut manager = TimerManager::new(config, timer_tx);

        let tid1 = TransactionId("test-1".to_string());
        let tid2 = TransactionId("test-2".to_string());

        manager.start_timer(
            TimerEvent::TimerB {
                transaction_id: tid1.clone(),
            },
            Duration::from_secs(10),
        );
        manager.start_timer(
            TimerEvent::TimerF {
                transaction_id: tid2.clone(),
            },
            Duration::from_secs(10),
        );

        manager.cancel_all();
        // 无 panic 即为成功
    }
}
