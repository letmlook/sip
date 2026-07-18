//! 对话管理器
//!
//! 统一管理所有对话的生命周期，提供对话创建、查找、更新、终止等功能，
//! 以及对话事件分发和运行指标收集。

use std::collections::HashMap;
use std::sync::Arc;

use siprs_core::{metrics::SipMetrics, CSeqNumber, DialogError, Tag};
use siprs_message::{Method, SipRequest, SipResponse};
use tokio::sync::{mpsc, Mutex};

use crate::state::{
    build_ack_for_2xx, create_uac_dialog_from_response, create_uas_dialog,
    is_invite_2xx_retransmit, update_dialog_on_response, validate_incoming_cseq,
    CSeqValidationResult, DialogUpdateResult,
};
use crate::types::{DialogEvent, DialogId, DialogInfo, DialogState};

// ============================================================================
// DialogManager - 对话管理器
// ============================================================================

/// SIP 对话管理器
///
/// 统一管理所有对话的生命周期，提供：
/// - 对话创建与查找
/// - 对话状态更新
/// - 对话内请求构建
/// - 对话终止与资源释放
/// - 对话事件分发
/// - 运行指标收集
///
/// # 线程安全
///
/// `DialogManager` 内部使用 `Arc<Mutex<...>>` 保证线程安全，
/// 可以在多个异步任务之间共享。
///
/// # 示例
///
/// ```ignore
/// use siprs_dialog::DialogManager;
/// use siprs_core::metrics::SipMetrics;
/// use std::sync::Arc;
///
/// let metrics = Arc::new(SipMetrics::new());
/// let (manager, event_rx) = DialogManager::with_event_channel(metrics);
/// ```
pub struct DialogManager {
    /// 对话集合
    dialogs: Arc<Mutex<HashMap<DialogId, DialogInfo>>>,
    /// 对话事件发送端
    event_tx: mpsc::UnboundedSender<DialogEvent>,
    /// 运行指标
    metrics: Arc<SipMetrics>,
    /// ACK 请求存储（用于 2xx 响应重传时重新发送 ACK）
    ack_store: Arc<Mutex<HashMap<DialogId, SipRequest>>>,
}

impl DialogManager {
    /// 创建新的对话管理器
    ///
    /// # 参数
    ///
    /// - `event_tx` - 对话事件发送端
    /// - `metrics` - 运行指标收集器
    pub fn new(event_tx: mpsc::UnboundedSender<DialogEvent>, metrics: Arc<SipMetrics>) -> Self {
        Self {
            dialogs: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
            metrics,
            ack_store: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 创建带事件通道的对话管理器
    ///
    /// 返回管理器和事件接收端，便于获取对话事件流。
    ///
    /// # 参数
    ///
    /// - `metrics` - 运行指标收集器
    pub fn with_event_channel(
        metrics: Arc<SipMetrics>,
    ) -> (Self, mpsc::UnboundedReceiver<DialogEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let manager = Self::new(event_tx, metrics);
        (manager, event_rx)
    }

    // ========================================================================
    // UAC 对话创建
    // ========================================================================

    /// 创建 UAC 对话
    ///
    /// 当 UAC 收到 INVITE 的 1xx（含 To Tag）或 2xx 响应时调用。
    ///
    /// - 收到 1xx 响应 → 创建早期对话
    /// - 收到 2xx 响应 → 如果早期对话已存在则升级，否则创建确认对话
    ///
    /// # 参数
    ///
    /// - `invite` - 原始 INVITE 请求
    /// - `response` - 收到的响应
    ///
    /// # 返回
    ///
    /// 成功返回对话信息的引用，失败返回 DialogError。
    pub async fn create_uac_dialog(
        &self,
        invite: &SipRequest,
        response: &SipResponse,
    ) -> Result<DialogInfo, DialogError> {
        let status_code = response.status_line.status_code;

        // 对于 2xx 响应，检查是否已有早期对话需要升级
        if status_code.is_success() {
            let call_id = response
                .headers
                .get(&siprs_message::HeaderName::CallId)
                .and_then(|v| v.as_call_id())
                .map(|cid| cid.0.clone());

            let from_tag = response
                .headers
                .get(&siprs_message::HeaderName::From)
                .and_then(|v| v.as_from_to())
                .and_then(|ft| ft.tag.as_ref())
                .map(|t| t.0.clone());

            let to_tag = response
                .headers
                .get(&siprs_message::HeaderName::To)
                .and_then(|v| v.as_from_to())
                .and_then(|ft| ft.tag.as_ref())
                .map(|t| t.0.clone());

            if let (Some(cid), Some(ft), Some(tt)) = (call_id, from_tag, to_tag) {
                let dialog_id = DialogId::new(cid, ft, tt);
                let mut dialogs = self.dialogs.lock().await;

                if let Some(existing_dialog) = dialogs.get_mut(&dialog_id) {
                    // 早期对话升级为确认对话
                    if existing_dialog.state == DialogState::Early {
                        existing_dialog.state = DialogState::Confirmed;

                        // 更新远端目标
                        if let Some(contact_uri) = response
                            .headers
                            .get(&siprs_message::HeaderName::Contact)
                            .and_then(|v| v.as_contact())
                            .map(|c| c.uri.clone())
                        {
                            existing_dialog.update_remote_target(contact_uri);
                        }

                        let _ = self.event_tx.send(DialogEvent::DialogConfirmed {
                            dialog_id: dialog_id.clone(),
                        });

                        tracing::info!(
                            "DialogManager: early dialog {} upgraded to confirmed",
                            dialog_id
                        );
                        return Ok(existing_dialog.clone());
                    }
                    // 已确认的对话收到 2xx 重传
                    return Ok(existing_dialog.clone());
                }
            }
        }

        // 创建新对话
        let dialog = create_uac_dialog_from_response(invite, response)?;

        let dialog_id = dialog.id.clone();
        let state = dialog.state;
        let mut dialogs = self.dialogs.lock().await;

        // 检查对话是否已存在
        if dialogs.contains_key(&dialog_id) {
            return Err(DialogError::AlreadyExists {
                id: dialog_id.to_string(),
            });
        }

        // 更新指标
        self.metrics.inc_active_dialogs();
        self.metrics.inc_dialogs_created();

        // 存储对话
        dialogs.insert(dialog_id.clone(), dialog.clone());

        // 发送事件
        let event = if state == DialogState::Confirmed {
            DialogEvent::DialogConfirmed {
                dialog_id: dialog_id.clone(),
            }
        } else {
            DialogEvent::DialogCreated {
                dialog_id: dialog_id.clone(),
            }
        };
        let _ = self.event_tx.send(event);

        tracing::info!(
            "DialogManager: created UAC dialog {} ({})",
            dialog_id,
            state
        );
        Ok(dialog)
    }

    // ========================================================================
    // UAS 对话创建
    // ========================================================================

    /// 创建 UAS 对话
    ///
    /// 当 UAS 准备发送 1xx（含 To Tag）或 2xx 响应时调用。
    ///
    /// # 参数
    ///
    /// - `invite` - 收到的 INVITE 请求
    /// - `local_tag` - UAS 为 To 头部生成的 tag
    /// - `state` - 初始对话状态（Early 或 Confirmed）
    ///
    /// # 返回
    ///
    /// 成功返回对话信息，失败返回 DialogError。
    pub async fn create_uas_dialog(
        &self,
        invite: &SipRequest,
        local_tag: Tag,
        state: DialogState,
    ) -> Result<DialogInfo, DialogError> {
        let dialog = create_uas_dialog(invite, local_tag, state)?;

        let dialog_id = dialog.id.clone();
        let dlg_state = dialog.state;
        let mut dialogs = self.dialogs.lock().await;

        // 检查对话是否已存在
        if dialogs.contains_key(&dialog_id) {
            // 如果早期对话已存在，升级为确认对话
            if let Some(existing_dialog) = dialogs.get_mut(&dialog_id) {
                if existing_dialog.state == DialogState::Early
                    && dlg_state == DialogState::Confirmed
                {
                    existing_dialog.state = DialogState::Confirmed;
                    let _ = self.event_tx.send(DialogEvent::DialogConfirmed {
                        dialog_id: dialog_id.clone(),
                    });
                    tracing::info!(
                        "DialogManager: UAS early dialog {} upgraded to confirmed",
                        dialog_id
                    );
                    return Ok(existing_dialog.clone());
                }
            }
            return Err(DialogError::AlreadyExists {
                id: dialog_id.to_string(),
            });
        }

        // 更新指标
        self.metrics.inc_active_dialogs();
        self.metrics.inc_dialogs_created();

        // 存储对话
        dialogs.insert(dialog_id.clone(), dialog.clone());

        // 发送事件
        let event = if dlg_state == DialogState::Confirmed {
            DialogEvent::DialogConfirmed {
                dialog_id: dialog_id.clone(),
            }
        } else {
            DialogEvent::DialogCreated {
                dialog_id: dialog_id.clone(),
            }
        };
        let _ = self.event_tx.send(event);

        tracing::info!(
            "DialogManager: created UAS dialog {} ({})",
            dialog_id,
            dlg_state
        );
        Ok(dialog)
    }

    // ========================================================================
    // 对话查找
    // ========================================================================

    /// 按 DialogId 查找对话
    ///
    /// # 参数
    ///
    /// - `dialog_id` - 对话标识
    ///
    /// # 返回
    ///
    /// 找到返回对话信息的克隆，未找到返回 DialogError::NotFound。
    pub async fn find_dialog(&self, dialog_id: &DialogId) -> Result<DialogInfo, DialogError> {
        let dialogs = self.dialogs.lock().await;
        dialogs
            .get(dialog_id)
            .cloned()
            .ok_or_else(|| DialogError::NotFound {
                id: dialog_id.to_string(),
            })
    }

    /// 按 Call-ID + Tags 查找对话
    ///
    /// 当不知道哪个 Tag 是本地 Tag、哪个是远端 Tag 时使用。
    /// 会尝试两种组合：(tag1=local, tag2=remote) 和 (tag1=remote, tag2=local)。
    ///
    /// # 参数
    ///
    /// - `call_id` - Call-ID 头部值
    /// - `tag1` - 第一个 Tag
    /// - `tag2` - 第二个 Tag
    ///
    /// # 返回
    ///
    /// 找到返回对话信息的克隆，未找到返回 DialogError::NotFound。
    pub async fn find_dialog_by_tags(
        &self,
        call_id: &str,
        tag1: &str,
        tag2: &str,
    ) -> Result<DialogInfo, DialogError> {
        let dialogs = self.dialogs.lock().await;

        // 尝试第一种组合：tag1=local, tag2=remote
        let id1 = DialogId::new(call_id.to_string(), tag1.to_string(), tag2.to_string());
        if let Some(dialog) = dialogs.get(&id1) {
            return Ok(dialog.clone());
        }

        // 尝试第二种组合：tag1=remote, tag2=local
        let id2 = DialogId::new(call_id.to_string(), tag2.to_string(), tag1.to_string());
        if let Some(dialog) = dialogs.get(&id2) {
            return Ok(dialog.clone());
        }

        Err(DialogError::NotFound {
            id: format!("{}:{}:{}", call_id, tag1, tag2),
        })
    }

    // ========================================================================
    // 对话更新
    // ========================================================================

    /// 更新对话状态
    ///
    /// 当 UAC 收到对话内响应时调用，根据响应更新对话状态。
    ///
    /// # 参数
    ///
    /// - `dialog_id` - 对话标识
    /// - `response` - 收到的响应
    ///
    /// # 返回
    ///
    /// 成功返回 Ok(())，失败返回 DialogError。
    pub async fn update_dialog(
        &self,
        dialog_id: &DialogId,
        response: &SipResponse,
    ) -> Result<(), DialogError> {
        let mut dialogs = self.dialogs.lock().await;

        let dialog = dialogs
            .get_mut(dialog_id)
            .ok_or_else(|| DialogError::NotFound {
                id: dialog_id.to_string(),
            })?;

        let result = update_dialog_on_response(dialog, response)?;

        match result {
            DialogUpdateResult::Updated => {
                let _ = self.event_tx.send(DialogEvent::DialogUpdated {
                    dialog_id: dialog_id.clone(),
                });
                tracing::debug!("DialogManager: dialog {} updated", dialog_id);
                Ok(())
            }
            DialogUpdateResult::EarlyDialogDestroyed => {
                // 早期对话被销毁
                let _dialog = dialogs.remove(dialog_id).unwrap();
                self.metrics.dec_active_dialogs();

                let _ = self.event_tx.send(DialogEvent::DialogTerminated {
                    dialog_id: dialog_id.clone(),
                    reason: format!(
                        "early dialog destroyed by {} response",
                        response.status_line.status_code
                    ),
                });

                tracing::info!("DialogManager: early dialog {} destroyed", dialog_id);
                Ok(())
            }
            DialogUpdateResult::NoChange => Ok(()),
        }
    }

    // ========================================================================
    // 对话内请求构建
    // ========================================================================

    /// 在对话内构建请求
    ///
    /// 自动处理 CSeq 递增、Route 头部和 Request-URI。
    ///
    /// # 参数
    ///
    /// - `dialog_id` - 对话标识
    /// - `method` - 请求方法
    ///
    /// # 返回
    ///
    /// 成功返回构建的 SipRequest，失败返回 DialogError。
    pub async fn build_in_dialog_request(
        &self,
        dialog_id: &DialogId,
        method: Method,
    ) -> Result<SipRequest, DialogError> {
        let mut dialogs = self.dialogs.lock().await;

        let dialog = dialogs
            .get_mut(dialog_id)
            .ok_or_else(|| DialogError::NotFound {
                id: dialog_id.to_string(),
            })?;

        // 验证对话状态
        if dialog.state == DialogState::Terminated {
            return Err(DialogError::InvalidState {
                detail: "cannot build request in terminated dialog".to_string(),
            });
        }

        crate::state::build_in_dialog_request(dialog, method)
    }

    // ========================================================================
    // 对话终止
    // ========================================================================

    /// 终止对话
    ///
    /// 当发送或收到 BYE 请求时调用。对话状态变为 Terminated，
    /// 并释放相关资源。
    ///
    /// # 参数
    ///
    /// - `dialog_id` - 对话标识
    /// - `reason` - 终止原因
    ///
    /// # 返回
    ///
    /// 成功返回 Ok(())，失败返回 DialogError。
    pub async fn terminate_dialog(
        &self,
        dialog_id: &DialogId,
        reason: String,
    ) -> Result<(), DialogError> {
        let mut dialogs = self.dialogs.lock().await;

        let dialog = dialogs
            .get_mut(dialog_id)
            .ok_or_else(|| DialogError::NotFound {
                id: dialog_id.to_string(),
            })?;

        if dialog.state == DialogState::Terminated {
            return Err(DialogError::InvalidState {
                detail: "dialog already terminated".to_string(),
            });
        }

        let prev_state = dialog.state;
        dialog.state = DialogState::Terminated;

        // 更新指标
        self.metrics.dec_active_dialogs();

        // 清理 ACK 存储
        {
            let mut ack_store = self.ack_store.lock().await;
            ack_store.remove(dialog_id);
        }

        // 发送事件
        let _ = self.event_tx.send(DialogEvent::DialogTerminated {
            dialog_id: dialog_id.clone(),
            reason: reason.clone(),
        });

        tracing::info!(
            "DialogManager: dialog {} terminated (was {}, reason: {})",
            dialog_id,
            prev_state,
            reason
        );

        // 从集合中移除已终止的对话
        dialogs.remove(dialog_id);

        Ok(())
    }

    // ========================================================================
    // INVITE 2xx 响应处理
    // ========================================================================

    /// 处理 INVITE 的 2xx 响应
    ///
    /// 当 UAC 收到 INVITE 的 2xx 响应时调用。处理以下情况：
    /// - 首次收到 2xx → 创建确认对话
    /// - 早期对话收到 2xx → 升级为确认对话
    /// - 已确认对话收到 2xx → 重传，返回需要重发的 ACK
    ///
    /// # 参数
    ///
    /// - `invite` - 原始 INVITE 请求
    /// - `response` - 收到的 2xx 响应
    ///
    /// # 返回
    ///
    /// 返回元组 (`DialogInfo`, `Option<SipRequest>`)：
    /// - `DialogInfo`: 对话信息
    /// - `Option<SipRequest>`: 如果是重传，返回需要重发的 ACK 请求
    pub async fn handle_invite_2xx(
        &self,
        invite: &SipRequest,
        response: &SipResponse,
    ) -> Result<(DialogInfo, Option<SipRequest>), DialogError> {
        // 提取对话标识信息
        let call_id = response
            .headers
            .get(&siprs_message::HeaderName::CallId)
            .and_then(|v| v.as_call_id())
            .map(|cid| cid.0.clone());

        let from_tag = response
            .headers
            .get(&siprs_message::HeaderName::From)
            .and_then(|v| v.as_from_to())
            .and_then(|ft| ft.tag.as_ref())
            .map(|t| t.0.clone());

        let to_tag = response
            .headers
            .get(&siprs_message::HeaderName::To)
            .and_then(|v| v.as_from_to())
            .and_then(|ft| ft.tag.as_ref())
            .map(|t| t.0.clone());

        // 检查是否为重传
        if let (Some(cid), Some(ft), Some(tt)) = (call_id, from_tag, to_tag) {
            let dialog_id = DialogId::new(cid, ft, tt);
            let dialogs = self.dialogs.lock().await;

            if let Some(dialog) = dialogs.get(&dialog_id) {
                if is_invite_2xx_retransmit(dialog, response) {
                    // 2xx 重传：返回存储的 ACK
                    let dialog_info = dialog.clone();
                    drop(dialogs);

                    let ack_store = self.ack_store.lock().await;
                    let ack = ack_store.get(&dialog_id).cloned();

                    tracing::debug!(
                        "DialogManager: INVITE 2xx retransmit detected for dialog {}",
                        dialog_id
                    );
                    return Ok((dialog_info, ack));
                }
            }
        }

        // 非重传：创建或升级对话
        let dialog = self.create_uac_dialog(invite, response).await?;

        // 构建 ACK 并存储
        let invite_cseq = invite
            .headers
            .get(&siprs_message::HeaderName::CSeq)
            .and_then(|v| v.as_cseq())
            .map(|cseq| cseq.sequence)
            .unwrap_or(CSeqNumber(1));

        let ack = build_ack_for_2xx(&dialog, invite_cseq)?;

        // 存储 ACK 用于重传
        {
            let mut ack_store = self.ack_store.lock().await;
            ack_store.insert(dialog.id.clone(), ack.clone());
        }

        Ok((dialog, Some(ack)))
    }

    // ========================================================================
    // 对话内请求处理
    // ========================================================================

    /// 处理对话内收到的请求
    ///
    /// 验证 CSeq、更新远端 CSeq，如果是 BYE 则终止对话。
    ///
    /// # 参数
    ///
    /// - `request` - 收到的请求
    ///
    /// # 返回
    ///
    /// 成功返回 (DialogId, CSeqValidationResult)：
    /// - DialogId: 对话标识
    /// - CSeqValidationResult: CSeq 验证结果
    pub async fn handle_in_dialog_request(
        &self,
        request: &SipRequest,
    ) -> Result<(DialogId, CSeqValidationResult), DialogError> {
        // 从请求中提取对话标识信息
        let call_id = request
            .headers
            .get(&siprs_message::HeaderName::CallId)
            .and_then(|v| v.as_call_id())
            .map(|cid| cid.0.clone())
            .ok_or_else(|| DialogError::InvalidState {
                detail: "missing Call-ID in request".to_string(),
            })?;

        let from_tag = request
            .headers
            .get(&siprs_message::HeaderName::From)
            .and_then(|v| v.as_from_to())
            .and_then(|ft| ft.tag.as_ref())
            .map(|t| t.0.clone())
            .ok_or_else(|| DialogError::InvalidState {
                detail: "missing From tag in request".to_string(),
            })?;

        let to_tag = request
            .headers
            .get(&siprs_message::HeaderName::To)
            .and_then(|v| v.as_from_to())
            .and_then(|ft| ft.tag.as_ref())
            .map(|t| t.0.clone())
            .ok_or_else(|| DialogError::InvalidState {
                detail: "missing To tag in request".to_string(),
            })?;

        // 查找对话
        let dialog = self
            .find_dialog_by_tags(&call_id, &from_tag, &to_tag)
            .await?;
        let dialog_id = dialog.id.clone();

        // 验证 CSeq
        let validation = validate_incoming_cseq(&dialog, request);

        match validation {
            CSeqValidationResult::Valid(new_cseq) => {
                // 更新远端 CSeq
                {
                    let mut dialogs = self.dialogs.lock().await;
                    if let Some(dlg) = dialogs.get_mut(&dialog_id) {
                        dlg.update_remote_cseq(new_cseq);
                    }
                }

                // 如果是 BYE 请求，终止对话
                if request.request_line.method == Method::Bye {
                    self.terminate_dialog(&dialog_id, "BYE received".to_string())
                        .await?;
                }

                Ok((dialog_id, CSeqValidationResult::Valid(new_cseq)))
            }
            CSeqValidationResult::Invalid { received, expected } => {
                tracing::warn!(
                    "DialogManager: invalid CSeq in request for dialog {} (received={}, expected>={})",
                    dialog_id,
                    received.0,
                    expected.0
                );
                Ok((
                    dialog_id,
                    CSeqValidationResult::Invalid { received, expected },
                ))
            }
        }
    }

    // ========================================================================
    // 辅助方法
    // ========================================================================

    /// 获取活跃对话数量
    pub async fn active_dialog_count(&self) -> usize {
        let dialogs = self.dialogs.lock().await;
        dialogs.len()
    }

    /// 获取所有对话标识
    pub async fn dialog_ids(&self) -> Vec<DialogId> {
        let dialogs = self.dialogs.lock().await;
        dialogs.keys().cloned().collect()
    }

    /// 检查对话是否存在
    pub async fn dialog_exists(&self, dialog_id: &DialogId) -> bool {
        let dialogs = self.dialogs.lock().await;
        dialogs.contains_key(dialog_id)
    }

    /// 清理所有已终止的对话
    pub async fn cleanup_terminated(&self) {
        let mut dialogs = self.dialogs.lock().await;
        let terminated: Vec<DialogId> = dialogs
            .iter()
            .filter(|(_, d)| d.state == DialogState::Terminated)
            .map(|(id, _)| id.clone())
            .collect();

        for id in terminated {
            dialogs.remove(&id);
            self.metrics.dec_active_dialogs();
        }
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use siprs_core::{Host, SipVersion, StatusCode, TransportProtocol};
    use siprs_message::{
        CSeqHeader, ContactHeader, FromToHeader, HeaderCollection, HeaderName, HeaderValue, SipUri,
        Tag as MsgTag, ViaHeader,
    };

    /// 创建测试用 INVITE 请求
    fn create_test_invite() -> SipRequest {
        let uri = SipUri::parse("sip:bob@example.com").unwrap();
        let mut headers = HeaderCollection::new();

        headers.insert(
            HeaderName::Via,
            HeaderValue::Via(ViaHeader::new(
                TransportProtocol::Udp,
                Host::Domain("192.168.1.1".to_string()),
                Some(5060),
            )),
        );
        headers.insert(
            HeaderName::From,
            HeaderValue::FromTo(
                FromToHeader::new(SipUri::parse("sip:alice@example.com").unwrap())
                    .with_tag(MsgTag("local-tag".to_string())),
            ),
        );
        headers.insert(
            HeaderName::To,
            HeaderValue::FromTo(FromToHeader::new(
                SipUri::parse("sip:bob@example.com").unwrap(),
            )),
        );
        headers.insert(
            HeaderName::CallId,
            HeaderValue::CallId(siprs_message::CallId("test-call-id@example.com".to_string())),
        );
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(CSeqHeader::new(1, Method::Invite)),
        );
        headers.insert(
            HeaderName::Contact,
            HeaderValue::Contact(ContactHeader::new(
                SipUri::parse("sip:alice@192.168.1.1:5060").unwrap(),
            )),
        );
        headers.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

        SipRequest {
            request_line: siprs_message::RequestLine {
                method: Method::Invite,
                request_uri: uri,
                version: SipVersion,
            },
            headers,
            body: None,
        }
    }

    /// 创建测试用 200 OK 响应
    fn create_test_200_response() -> SipResponse {
        let mut headers = HeaderCollection::new();

        headers.insert(
            HeaderName::Via,
            HeaderValue::Via(ViaHeader::new(
                TransportProtocol::Udp,
                Host::Domain("192.168.1.1".to_string()),
                Some(5060),
            )),
        );
        headers.insert(
            HeaderName::From,
            HeaderValue::FromTo(
                FromToHeader::new(SipUri::parse("sip:alice@example.com").unwrap())
                    .with_tag(MsgTag("local-tag".to_string())),
            ),
        );
        headers.insert(
            HeaderName::To,
            HeaderValue::FromTo(
                FromToHeader::new(SipUri::parse("sip:bob@example.com").unwrap())
                    .with_tag(MsgTag("remote-tag".to_string())),
            ),
        );
        headers.insert(
            HeaderName::CallId,
            HeaderValue::CallId(siprs_message::CallId("test-call-id@example.com".to_string())),
        );
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(CSeqHeader::new(1, Method::Invite)),
        );
        headers.insert(
            HeaderName::Contact,
            HeaderValue::Contact(ContactHeader::new(
                SipUri::parse("sip:bob@192.168.1.2:5060").unwrap(),
            )),
        );

        SipResponse {
            status_line: siprs_message::StatusLine {
                version: SipVersion,
                status_code: StatusCode::OK,
                reason_phrase: "OK".to_string(),
            },
            headers,
            body: None,
        }
    }

    /// 创建测试用 180 Ringing 响应
    fn create_test_180_response() -> SipResponse {
        let mut headers = HeaderCollection::new();

        headers.insert(
            HeaderName::Via,
            HeaderValue::Via(ViaHeader::new(
                TransportProtocol::Udp,
                Host::Domain("192.168.1.1".to_string()),
                Some(5060),
            )),
        );
        headers.insert(
            HeaderName::From,
            HeaderValue::FromTo(
                FromToHeader::new(SipUri::parse("sip:alice@example.com").unwrap())
                    .with_tag(MsgTag("local-tag".to_string())),
            ),
        );
        headers.insert(
            HeaderName::To,
            HeaderValue::FromTo(
                FromToHeader::new(SipUri::parse("sip:bob@example.com").unwrap())
                    .with_tag(MsgTag("remote-tag".to_string())),
            ),
        );
        headers.insert(
            HeaderName::CallId,
            HeaderValue::CallId(siprs_message::CallId("test-call-id@example.com".to_string())),
        );
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(CSeqHeader::new(1, Method::Invite)),
        );
        headers.insert(
            HeaderName::Contact,
            HeaderValue::Contact(ContactHeader::new(
                SipUri::parse("sip:bob@192.168.1.2:5060").unwrap(),
            )),
        );

        SipResponse {
            status_line: siprs_message::StatusLine {
                version: SipVersion,
                status_code: StatusCode(180),
                reason_phrase: "Ringing".to_string(),
            },
            headers,
            body: None,
        }
    }

    #[tokio::test]
    async fn test_create_uac_dialog_from_2xx() {
        let metrics = Arc::new(SipMetrics::new());
        let (manager, _event_rx) = DialogManager::with_event_channel(metrics);

        let invite = create_test_invite();
        let response = create_test_200_response();

        let dialog = manager.create_uac_dialog(&invite, &response).await.unwrap();

        assert_eq!(dialog.id.call_id, "test-call-id@example.com");
        assert_eq!(dialog.id.local_tag, "local-tag");
        assert_eq!(dialog.id.remote_tag, "remote-tag");
        assert_eq!(dialog.state, DialogState::Confirmed);
        assert!(dialog.is_uac);
    }

    #[tokio::test]
    async fn test_create_uac_dialog_from_1xx() {
        let metrics = Arc::new(SipMetrics::new());
        let (manager, _event_rx) = DialogManager::with_event_channel(metrics);

        let invite = create_test_invite();
        let response = create_test_180_response();

        let dialog = manager.create_uac_dialog(&invite, &response).await.unwrap();

        assert_eq!(dialog.state, DialogState::Early);
    }

    #[tokio::test]
    async fn test_upgrade_early_to_confirmed() {
        let metrics = Arc::new(SipMetrics::new());
        let (manager, _event_rx) = DialogManager::with_event_channel(metrics);

        let invite = create_test_invite();

        // 先创建早期对话
        let response_180 = create_test_180_response();
        let dialog = manager
            .create_uac_dialog(&invite, &response_180)
            .await
            .unwrap();
        assert_eq!(dialog.state, DialogState::Early);

        // 然后升级为确认对话
        let response_200 = create_test_200_response();
        let dialog = manager
            .create_uac_dialog(&invite, &response_200)
            .await
            .unwrap();
        assert_eq!(dialog.state, DialogState::Confirmed);
    }

    #[tokio::test]
    async fn test_create_uas_dialog() {
        let metrics = Arc::new(SipMetrics::new());
        let (manager, _event_rx) = DialogManager::with_event_channel(metrics);

        let invite = create_test_invite();
        let local_tag = "uas-tag".to_string();

        let dialog = manager
            .create_uas_dialog(&invite, local_tag.clone(), DialogState::Confirmed)
            .await
            .unwrap();

        assert_eq!(dialog.id.local_tag, local_tag);
        assert_eq!(dialog.id.remote_tag, "local-tag"); // From tag
        assert_eq!(dialog.state, DialogState::Confirmed);
        assert!(!dialog.is_uac);
    }

    #[tokio::test]
    async fn test_find_dialog() {
        let metrics = Arc::new(SipMetrics::new());
        let (manager, _event_rx) = DialogManager::with_event_channel(metrics);

        let invite = create_test_invite();
        let response = create_test_200_response();

        let dialog = manager.create_uac_dialog(&invite, &response).await.unwrap();

        // 按 DialogId 查找
        let found = manager.find_dialog(&dialog.id).await.unwrap();
        assert_eq!(found.id, dialog.id);
    }

    #[tokio::test]
    async fn test_find_dialog_not_found() {
        let metrics = Arc::new(SipMetrics::new());
        let (manager, _event_rx) = DialogManager::with_event_channel(metrics);

        let id = DialogId::new("nonexistent".to_string(), "a".to_string(), "b".to_string());
        let result = manager.find_dialog(&id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_find_dialog_by_tags() {
        let metrics = Arc::new(SipMetrics::new());
        let (manager, _event_rx) = DialogManager::with_event_channel(metrics);

        let invite = create_test_invite();
        let response = create_test_200_response();

        let dialog = manager.create_uac_dialog(&invite, &response).await.unwrap();

        // 按 Call-ID + Tags 查找（正向）
        let found = manager
            .find_dialog_by_tags("test-call-id@example.com", "local-tag", "remote-tag")
            .await
            .unwrap();
        assert_eq!(found.id, dialog.id);

        // 按 Call-ID + Tags 查找（反向）
        let found = manager
            .find_dialog_by_tags("test-call-id@example.com", "remote-tag", "local-tag")
            .await
            .unwrap();
        assert_eq!(found.id, dialog.id);
    }

    #[tokio::test]
    async fn test_build_in_dialog_request() {
        let metrics = Arc::new(SipMetrics::new());
        let (manager, _event_rx) = DialogManager::with_event_channel(metrics);

        let invite = create_test_invite();
        let response = create_test_200_response();

        let dialog = manager.create_uac_dialog(&invite, &response).await.unwrap();

        // 构建对话内 BYE 请求
        let request = manager
            .build_in_dialog_request(&dialog.id, Method::Bye)
            .await
            .unwrap();

        assert_eq!(request.request_line.method, Method::Bye);
        // Request-URI 应该是远端目标
        assert_eq!(
            request.request_line.request_uri,
            SipUri::parse("sip:bob@192.168.1.2:5060").unwrap()
        );
    }

    #[tokio::test]
    async fn test_terminate_dialog() {
        let metrics = Arc::new(SipMetrics::new());
        let (manager, mut event_rx) = DialogManager::with_event_channel(metrics);

        let invite = create_test_invite();
        let response = create_test_200_response();

        let dialog = manager.create_uac_dialog(&invite, &response).await.unwrap();

        // 先消费创建事件
        let _create_event = event_rx.try_recv().unwrap();

        // 终止对话
        manager
            .terminate_dialog(&dialog.id, "BYE sent".to_string())
            .await
            .unwrap();

        // 验证对话已移除
        let result = manager.find_dialog(&dialog.id).await;
        assert!(result.is_err());

        // 验证终止事件
        let event = event_rx.try_recv().unwrap();
        assert!(matches!(
            event,
            DialogEvent::DialogTerminated {
                ref dialog_id,
                ..
            } if dialog_id == &dialog.id
        ));
    }

    #[tokio::test]
    async fn test_handle_invite_2xx() {
        let metrics = Arc::new(SipMetrics::new());
        let (manager, _event_rx) = DialogManager::with_event_channel(metrics);

        let invite = create_test_invite();
        let response = create_test_200_response();

        let (dialog, ack) = manager.handle_invite_2xx(&invite, &response).await.unwrap();

        assert_eq!(dialog.state, DialogState::Confirmed);
        assert!(ack.is_some());
        assert_eq!(ack.unwrap().request_line.method, Method::Ack);
    }

    #[tokio::test]
    async fn test_handle_invite_2xx_retransmit() {
        let metrics = Arc::new(SipMetrics::new());
        let (manager, _event_rx) = DialogManager::with_event_channel(metrics);

        let invite = create_test_invite();
        let response = create_test_200_response();

        // 首次收到 2xx
        let (_, ack1) = manager.handle_invite_2xx(&invite, &response).await.unwrap();
        assert!(ack1.is_some());

        // 重传的 2xx
        let (_, ack2) = manager.handle_invite_2xx(&invite, &response).await.unwrap();
        assert!(ack2.is_some()); // 应该返回存储的 ACK
    }

    #[tokio::test]
    async fn test_handle_in_dialog_request_bye() {
        let metrics = Arc::new(SipMetrics::new());
        let (manager, _event_rx) = DialogManager::with_event_channel(metrics);

        // 先创建 UAS 对话
        let invite = create_test_invite();
        let local_tag = "uas-tag".to_string();
        let _dialog = manager
            .create_uas_dialog(&invite, local_tag, DialogState::Confirmed)
            .await
            .unwrap();

        // 创建 BYE 请求
        let mut headers = HeaderCollection::new();
        headers.insert(
            HeaderName::Via,
            HeaderValue::Via(ViaHeader::new(
                TransportProtocol::Udp,
                Host::Domain("192.168.1.1".to_string()),
                Some(5060),
            )),
        );
        headers.insert(
            HeaderName::From,
            HeaderValue::FromTo(
                FromToHeader::new(SipUri::parse("sip:alice@example.com").unwrap())
                    .with_tag(MsgTag("local-tag".to_string())),
            ),
        );
        headers.insert(
            HeaderName::To,
            HeaderValue::FromTo(
                FromToHeader::new(SipUri::parse("sip:bob@example.com").unwrap())
                    .with_tag(MsgTag("uas-tag".to_string())),
            ),
        );
        headers.insert(
            HeaderName::CallId,
            HeaderValue::CallId(siprs_message::CallId("test-call-id@example.com".to_string())),
        );
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(CSeqHeader::new(2, Method::Bye)),
        );

        let bye_request = SipRequest {
            request_line: siprs_message::RequestLine {
                method: Method::Bye,
                request_uri: SipUri::parse("sip:bob@example.com").unwrap(),
                version: SipVersion,
            },
            headers,
            body: None,
        };

        // 处理 BYE 请求
        let (dialog_id, validation) = manager
            .handle_in_dialog_request(&bye_request)
            .await
            .unwrap();

        assert!(matches!(validation, CSeqValidationResult::Valid(_)));

        // 对话应该已终止
        let result = manager.find_dialog(&dialog_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_in_dialog_request_invalid_cseq() {
        let metrics = Arc::new(SipMetrics::new());
        let (manager, _event_rx) = DialogManager::with_event_channel(metrics);

        // 先创建 UAS 对话
        let invite = create_test_invite();
        let local_tag = "uas-tag".to_string();
        let _dialog = manager
            .create_uas_dialog(&invite, local_tag, DialogState::Confirmed)
            .await
            .unwrap();

        // 创建 CSeq=1 的 BYE 请求（等于远端 CSeq，应该无效）
        let mut headers = HeaderCollection::new();
        headers.insert(
            HeaderName::Via,
            HeaderValue::Via(ViaHeader::new(
                TransportProtocol::Udp,
                Host::Domain("192.168.1.1".to_string()),
                Some(5060),
            )),
        );
        headers.insert(
            HeaderName::From,
            HeaderValue::FromTo(
                FromToHeader::new(SipUri::parse("sip:alice@example.com").unwrap())
                    .with_tag(MsgTag("local-tag".to_string())),
            ),
        );
        headers.insert(
            HeaderName::To,
            HeaderValue::FromTo(
                FromToHeader::new(SipUri::parse("sip:bob@example.com").unwrap())
                    .with_tag(MsgTag("uas-tag".to_string())),
            ),
        );
        headers.insert(
            HeaderName::CallId,
            HeaderValue::CallId(siprs_message::CallId("test-call-id@example.com".to_string())),
        );
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(CSeqHeader::new(1, Method::Bye)),
        );

        let bye_request = SipRequest {
            request_line: siprs_message::RequestLine {
                method: Method::Bye,
                request_uri: SipUri::parse("sip:bob@example.com").unwrap(),
                version: SipVersion,
            },
            headers,
            body: None,
        };

        // 处理 BYE 请求
        let (_, validation) = manager
            .handle_in_dialog_request(&bye_request)
            .await
            .unwrap();

        assert!(matches!(validation, CSeqValidationResult::Invalid { .. }));
    }

    #[tokio::test]
    async fn test_active_dialog_count() {
        let metrics = Arc::new(SipMetrics::new());
        let (manager, _event_rx) = DialogManager::with_event_channel(metrics);

        assert_eq!(manager.active_dialog_count().await, 0);

        let invite = create_test_invite();
        let response = create_test_200_response();

        manager.create_uac_dialog(&invite, &response).await.unwrap();
        assert_eq!(manager.active_dialog_count().await, 1);
    }

    #[tokio::test]
    async fn test_dialog_exists() {
        let metrics = Arc::new(SipMetrics::new());
        let (manager, _event_rx) = DialogManager::with_event_channel(metrics);

        let invite = create_test_invite();
        let response = create_test_200_response();

        let dialog = manager.create_uac_dialog(&invite, &response).await.unwrap();

        assert!(manager.dialog_exists(&dialog.id).await);

        let nonexistent = DialogId::new("no".to_string(), "a".to_string(), "b".to_string());
        assert!(!manager.dialog_exists(&nonexistent).await);
    }

    #[tokio::test]
    async fn test_duplicate_dialog_creation() {
        let metrics = Arc::new(SipMetrics::new());
        let (manager, _event_rx) = DialogManager::with_event_channel(metrics);

        let invite = create_test_invite();
        let response = create_test_200_response();

        // 第一次创建应该成功
        manager.create_uac_dialog(&invite, &response).await.unwrap();

        // 第二次创建应该失败（对话已存在且为 Confirmed）
        let result = manager.create_uac_dialog(&invite, &response).await;
        // 由于已确认对话收到 2xx 会返回已存在的对话，不会报错
        assert!(result.is_ok());
    }
}
