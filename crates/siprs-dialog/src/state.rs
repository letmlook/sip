//! 对话创建与状态维护
//!
//! 按照 RFC 3261 Section 12 实现对话的创建、维护、路由集管理和对话内请求构建。
//!
//! # 对话创建规则
//!
//! - UAC 收到 INVITE 的 1xx 响应（含 To Tag）时创建早期对话
//! - UAC 收到 INVITE 的 2xx 响应时创建/升级为确认对话
//! - UAS 发送 1xx 响应（含 To Tag）时创建早期对话
//! - UAS 发送 2xx 响应时升级为确认对话
//!
//! # 路由集规则
//!
//! - UAC 侧的路由集来自 2xx 响应中的 Record-Route 头部（逆序）
//! - UAS 侧的路由集来自 INVITE 请求中的 Record-Route 头部（正序）

use siprs_core::{CSeqNumber, CallId, DialogError, Host, SipVersion, Tag, TransportProtocol};
use siprs_message::{
    CSeqHeader, FromToHeader, HeaderCollection, HeaderName, HeaderValue, Method, RouteHeader,
    SipRequest, SipResponse, SipUri, Tag as MsgTag, ViaHeader,
};

use crate::types::{DialogId, DialogInfo, DialogState};

// ============================================================================
// 头部提取辅助函数
// ============================================================================

/// 从消息头中提取 Call-ID
fn extract_call_id(headers: &HeaderCollection) -> Result<CallId, DialogError> {
    headers
        .get(&HeaderName::CallId)
        .and_then(|v| v.as_call_id())
        .map(|cid| cid.0.clone())
        .ok_or_else(|| DialogError::InvalidState {
            detail: "missing Call-ID header".to_string(),
        })
}

/// 从 From 头部提取 Tag
fn extract_from_tag(headers: &HeaderCollection) -> Result<Tag, DialogError> {
    headers
        .get(&HeaderName::From)
        .and_then(|v| v.as_from_to())
        .and_then(|ft| ft.tag.as_ref())
        .map(|t| t.0.clone())
        .ok_or_else(|| DialogError::InvalidState {
            detail: "missing tag in From header".to_string(),
        })
}

/// 从 To 头部提取 Tag
fn extract_to_tag(headers: &HeaderCollection) -> Result<Tag, DialogError> {
    headers
        .get(&HeaderName::To)
        .and_then(|v| v.as_from_to())
        .and_then(|ft| ft.tag.as_ref())
        .map(|t| t.0.clone())
        .ok_or_else(|| DialogError::InvalidState {
            detail: "missing tag in To header".to_string(),
        })
}

/// 从 From 头部提取 URI
fn extract_from_uri(headers: &HeaderCollection) -> Result<SipUri, DialogError> {
    headers
        .get(&HeaderName::From)
        .and_then(|v| v.as_from_to())
        .map(|ft| ft.uri.clone())
        .ok_or_else(|| DialogError::InvalidState {
            detail: "missing From header".to_string(),
        })
}

/// 从 To 头部提取 URI
fn extract_to_uri(headers: &HeaderCollection) -> Result<SipUri, DialogError> {
    headers
        .get(&HeaderName::To)
        .and_then(|v| v.as_from_to())
        .map(|ft| ft.uri.clone())
        .ok_or_else(|| DialogError::InvalidState {
            detail: "missing To header".to_string(),
        })
}

/// 从 CSeq 头部提取序列号和方法
fn extract_cseq(headers: &HeaderCollection) -> Result<CSeqHeader, DialogError> {
    headers
        .get(&HeaderName::CSeq)
        .and_then(|v| v.as_cseq())
        .cloned()
        .ok_or_else(|| DialogError::InvalidState {
            detail: "missing CSeq header".to_string(),
        })
}

/// 从 Contact 头部提取 URI
fn extract_contact_uri(headers: &HeaderCollection) -> Option<SipUri> {
    headers
        .get(&HeaderName::Contact)
        .and_then(|v| v.as_contact())
        .map(|c| c.uri.clone())
}

// ============================================================================
// 路由集构建
// ============================================================================

/// 构建路由集（UAC 侧，从 2xx 响应的 Record-Route 逆序）
///
/// 按照 RFC 3261 Section 12.1.2，UAC 侧的路由集来自响应中的
/// Record-Route 头部，需要逆序排列。
pub fn build_route_set_uac(headers: &HeaderCollection) -> Vec<RouteHeader> {
    let mut routes: Vec<RouteHeader> = headers
        .get_all(&HeaderName::RecordRoute)
        .iter()
        .filter_map(|v| {
            if let HeaderValue::Route(r) = v {
                Some(r.clone())
            } else {
                None
            }
        })
        .collect();
    routes.reverse();
    routes
}

/// 构建路由集（UAS 侧，从 INVITE 请求的 Record-Route 正序）
///
/// 按照 RFC 3261 Section 12.1.2，UAS 侧的路由集来自请求中的
/// Record-Route 头部，保持原始顺序。
pub fn build_route_set_uas(headers: &HeaderCollection) -> Vec<RouteHeader> {
    headers
        .get_all(&HeaderName::RecordRoute)
        .iter()
        .filter_map(|v| {
            if let HeaderValue::Route(r) = v {
                Some(r.clone())
            } else {
                None
            }
        })
        .collect()
}

// ============================================================================
// 对话创建
// ============================================================================

/// UAC 侧从响应创建对话
///
/// 当 UAC 收到 INVITE 的 1xx（含 To Tag）或 2xx 响应时调用。
///
/// # 参数
///
/// - `invite` - 原始 INVITE 请求
/// - `response` - 收到的响应
///
/// # 返回
///
/// 成功返回创建的 DialogInfo，失败返回 DialogError。
///
/// # 对话标识
///
/// - call_id: 来自响应的 Call-ID 头部
/// - local_tag: 来自响应的 From 头部 tag（UAC 的 tag）
/// - remote_tag: 来自响应的 To 头部 tag（UAS 的 tag）
pub fn create_uac_dialog_from_response(
    invite: &SipRequest,
    response: &SipResponse,
) -> Result<DialogInfo, DialogError> {
    // 提取 Call-ID
    let call_id = extract_call_id(&response.headers)?;

    // 提取本地 Tag（From 头部的 tag - UAC 的 tag）
    let local_tag = extract_from_tag(&response.headers)?;

    // 提取远端 Tag（To 头部的 tag - UAS 的 tag）
    let remote_tag = extract_to_tag(&response.headers)?;

    // 提取本地 URI（From URI）
    let local_uri = extract_from_uri(&response.headers)?;

    // 提取远端 URI（To URI）
    let remote_uri = extract_to_uri(&response.headers)?;

    // 提取本地 CSeq（来自 INVITE 的 CSeq）
    let invite_cseq = extract_cseq(&invite.headers)?;
    let local_cseq = invite_cseq.sequence;

    // 根据响应状态码决定对话状态
    let state = if response.status_line.status_code.is_provisional() {
        DialogState::Early
    } else if response.status_line.status_code.is_success() {
        DialogState::Confirmed
    } else {
        return Err(DialogError::InvalidState {
            detail: format!(
                "cannot create dialog from {} response",
                response.status_line.status_code
            ),
        });
    };

    // 提取远端目标（Contact URI）
    let remote_target = extract_contact_uri(&response.headers);

    // 构建路由集（UAC 侧：Record-Route 逆序）
    let route_set = build_route_set_uac(&response.headers);

    Ok(DialogInfo {
        id: DialogId::new(call_id, local_tag, remote_tag),
        state,
        local_uri,
        remote_uri,
        local_cseq,
        remote_cseq: CSeqNumber(0),
        remote_target,
        route_set,
        is_uac: true,
    })
}

/// UAS 侧创建对话
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
/// 成功返回创建的 DialogInfo，失败返回 DialogError。
///
/// # 对话标识
///
/// - call_id: 来自请求的 Call-ID 头部
/// - local_tag: UAS 生成的 tag（将放入 To 头部）
/// - remote_tag: 来自请求的 From 头部 tag（UAC 的 tag）
pub fn create_uas_dialog(
    invite: &SipRequest,
    local_tag: Tag,
    state: DialogState,
) -> Result<DialogInfo, DialogError> {
    // 提取 Call-ID
    let call_id = extract_call_id(&invite.headers)?;

    // 提取远端 Tag（From 头部的 tag - UAC 的 tag）
    let remote_tag = extract_from_tag(&invite.headers)?;

    // 提取远端 URI（From URI）
    let remote_uri = extract_from_uri(&invite.headers)?;

    // 提取本地 URI（To URI - 来自 INVITE 请求）
    let local_uri = extract_to_uri(&invite.headers)?;

    // 提取远端 CSeq（来自 INVITE 的 CSeq）
    let invite_cseq = extract_cseq(&invite.headers)?;
    let remote_cseq = invite_cseq.sequence;

    // 提取远端目标（来自 INVITE 的 Contact URI）
    let remote_target = extract_contact_uri(&invite.headers);

    // 构建路由集（UAS 侧：Record-Route 正序）
    let route_set = build_route_set_uas(&invite.headers);

    Ok(DialogInfo {
        id: DialogId::new(call_id, local_tag, remote_tag),
        state,
        local_uri,
        remote_uri,
        local_cseq: CSeqNumber(0),
        remote_cseq,
        remote_target,
        route_set,
        is_uac: false,
    })
}

// ============================================================================
// 对话状态更新
// ============================================================================

/// 对话状态更新结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogUpdateResult {
    /// 对话已更新
    Updated,
    /// 早期对话应被销毁（收到非 2xx 最终响应）
    EarlyDialogDestroyed,
    /// 无需更新
    NoChange,
}

/// 更新对话状态（UAC 侧收到响应时调用）
///
/// # 规则
///
/// - 收到 2xx 响应 → 更新远端目标，升级为确认对话
/// - 收到 1xx 响应 → 无需更新（早期对话已创建）
/// - 早期对话收到非 2xx 最终响应 → 销毁对话
/// - 确认对话收到非 2xx 最终响应 → 不改变对话状态
///
/// # 注意
///
/// 路由集在对话创建时确定，后续不会更新（RFC 3261 Section 12.2.2）。
pub fn update_dialog_on_response(
    dialog: &mut DialogInfo,
    response: &SipResponse,
) -> Result<DialogUpdateResult, DialogError> {
    let status_code = response.status_line.status_code;

    if status_code.is_success() {
        // 2xx 响应：更新远端目标，升级为确认对话
        let mut updated = false;

        // 更新远端目标
        if let Some(contact_uri) = extract_contact_uri(&response.headers) {
            dialog.update_remote_target(contact_uri);
            updated = true;
        }

        // 升级为确认对话
        if dialog.state == DialogState::Early {
            dialog.state = DialogState::Confirmed;
            updated = true;
        }

        if updated {
            Ok(DialogUpdateResult::Updated)
        } else {
            Ok(DialogUpdateResult::NoChange)
        }
    } else if status_code.is_provisional() {
        // 1xx 响应：无需更新
        Ok(DialogUpdateResult::NoChange)
    } else {
        // 3xx-6xx 最终响应
        if dialog.state == DialogState::Early {
            // 早期对话收到非 2xx 最终响应 → 销毁
            Ok(DialogUpdateResult::EarlyDialogDestroyed)
        } else {
            // 确认对话不受影响
            Ok(DialogUpdateResult::NoChange)
        }
    }
}

// ============================================================================
// 对话内请求构建
// ============================================================================

/// 在对话内构建请求
///
/// 按照 RFC 3261 Section 12.2.1 构建对话内请求，自动处理：
/// - CSeq 递增
/// - Route 头部（从路由集）
/// - Request-URI（从远端目标）
/// - Call-ID、From、To、Via、Max-Forwards 等必要头部
///
/// # 参数
///
/// - `dialog` - 对话信息（local_cseq 会被递增）
/// - `method` - 请求方法
///
/// # 返回
///
/// 成功返回构建的 SipRequest，失败返回 DialogError。
pub fn build_in_dialog_request(
    dialog: &mut DialogInfo,
    method: Method,
) -> Result<SipRequest, DialogError> {
    // 1. 递增本地 CSeq
    let new_cseq = dialog.increment_local_cseq();

    // 2. 确定 Request-URI
    let request_uri = dialog
        .request_uri()
        .ok_or_else(|| DialogError::InvalidState {
            detail: "no remote target available for in-dialog request".to_string(),
        })?;

    // 3. 构建 Route 头部
    let route_headers = dialog.route_headers_for_request();

    // 4. 构建头部集合
    let mut headers = HeaderCollection::new();

    // Via 头部（使用默认值，调用者可以后续修改）
    let via = ViaHeader::new(
        TransportProtocol::Udp,
        Host::Domain("localhost".to_string()),
        Some(5060),
    );
    headers.insert(HeaderName::Via, HeaderValue::Via(via));

    // Max-Forwards 头部
    headers.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

    // From 头部（本地 URI + 本地 Tag）
    let from_header =
        FromToHeader::new(dialog.local_uri.clone()).with_tag(MsgTag(dialog.id.local_tag.clone()));
    headers.insert(HeaderName::From, HeaderValue::FromTo(from_header));

    // To 头部（远端 URI + 远端 Tag）
    let to_header =
        FromToHeader::new(dialog.remote_uri.clone()).with_tag(MsgTag(dialog.id.remote_tag.clone()));
    headers.insert(HeaderName::To, HeaderValue::FromTo(to_header));

    // Call-ID 头部
    headers.insert(
        HeaderName::CallId,
        HeaderValue::CallId(siprs_message::CallId(dialog.id.call_id.clone())),
    );

    // CSeq 头部
    headers.insert(
        HeaderName::CSeq,
        HeaderValue::CSeq(CSeqHeader::new(new_cseq.0, method.clone())),
    );

    // Route 头部
    for route in route_headers {
        headers.insert(HeaderName::Route, HeaderValue::Route(route));
    }

    // 5. 构建请求
    Ok(SipRequest {
        request_line: siprs_message::RequestLine {
            method,
            request_uri,
            version: SipVersion,
        },
        headers,
        body: None,
    })
}

// ============================================================================
// CSeq 验证
// ============================================================================

/// CSeq 验证结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSeqValidationResult {
    /// CSeq 有效（大于远端 CSeq）
    Valid(CSeqNumber),
    /// CSeq 无效（小于或等于远端 CSeq），应拒绝请求
    Invalid {
        received: CSeqNumber,
        expected: CSeqNumber,
    },
}

/// 验证收到的请求 CSeq
///
/// 按照 RFC 3261 Section 12.2.2，收到的请求 CSeq 必须大于远端 CSeq。
/// 如果 CSeq <= 远端 CSeq，应返回 500 响应拒绝该请求。
///
/// # 参数
///
/// - `dialog` - 对话信息
/// - `request` - 收到的请求
///
/// # 返回
///
/// - `CSeqValidationResult::Valid` - CSeq 有效，返回新的远端 CSeq
/// - `CSeqValidationResult::Invalid` - CSeq 无效，应拒绝请求
pub fn validate_incoming_cseq(dialog: &DialogInfo, request: &SipRequest) -> CSeqValidationResult {
    match extract_cseq(&request.headers) {
        Ok(cseq_header) => {
            let received = cseq_header.sequence;
            if received.0 > dialog.remote_cseq.0 {
                CSeqValidationResult::Valid(received)
            } else {
                CSeqValidationResult::Invalid {
                    received,
                    expected: CSeqNumber(dialog.remote_cseq.0.saturating_add(1)),
                }
            }
        }
        Err(_) => CSeqValidationResult::Invalid {
            received: CSeqNumber(0),
            expected: CSeqNumber(dialog.remote_cseq.0.saturating_add(1)),
        },
    }
}

// ============================================================================
// 2xx 响应重传检测
// ============================================================================

/// 检测 INVITE 的 2xx 响应是否为重传
///
/// 当 UAC 收到 INVITE 的 2xx 响应时，如果对应的对话已经是确认状态，
/// 则该响应为重传，需要重新发送 ACK。
///
/// # 参数
///
/// - `dialog` - 对话信息
/// - `response` - 收到的 2xx 响应
///
/// # 返回
///
/// 如果是重传返回 true，否则返回 false。
pub fn is_invite_2xx_retransmit(dialog: &DialogInfo, response: &SipResponse) -> bool {
    // 对话已经是确认状态，且响应是 2xx → 重传
    dialog.state == DialogState::Confirmed && response.status_line.status_code.is_success()
}

/// 构建 ACK 请求（用于 2xx 响应的 ACK）
///
/// 按照 RFC 3261 Section 13.2.2.4，2xx 响应的 ACK 是一个新的事务，
/// 需要包含完整的对话头部。
///
/// # 参数
///
/// - `dialog` - 对话信息
/// - `invite_cseq` - 原始 INVITE 的 CSeq 序列号
///
/// # 返回
///
/// 成功返回 ACK 请求，失败返回 DialogError。
pub fn build_ack_for_2xx(
    dialog: &DialogInfo,
    invite_cseq: CSeqNumber,
) -> Result<SipRequest, DialogError> {
    // 确定 Request-URI
    let request_uri = dialog
        .request_uri()
        .ok_or_else(|| DialogError::InvalidState {
            detail: "no remote target available for ACK".to_string(),
        })?;

    // 构建 Route 头部
    let route_headers = dialog.route_headers_for_request();

    // 构建头部集合
    let mut headers = HeaderCollection::new();

    // Via 头部
    let via = ViaHeader::new(
        TransportProtocol::Udp,
        Host::Domain("localhost".to_string()),
        Some(5060),
    );
    headers.insert(HeaderName::Via, HeaderValue::Via(via));

    // Max-Forwards 头部
    headers.insert(HeaderName::MaxForwards, HeaderValue::MaxForwards(70));

    // From 头部
    let from_header =
        FromToHeader::new(dialog.local_uri.clone()).with_tag(MsgTag(dialog.id.local_tag.clone()));
    headers.insert(HeaderName::From, HeaderValue::FromTo(from_header));

    // To 头部
    let to_header =
        FromToHeader::new(dialog.remote_uri.clone()).with_tag(MsgTag(dialog.id.remote_tag.clone()));
    headers.insert(HeaderName::To, HeaderValue::FromTo(to_header));

    // Call-ID 头部
    headers.insert(
        HeaderName::CallId,
        HeaderValue::CallId(siprs_message::CallId(dialog.id.call_id.clone())),
    );

    // CSeq 头部（ACK 使用与 INVITE 相同的 CSeq 序列号，方法为 ACK）
    headers.insert(
        HeaderName::CSeq,
        HeaderValue::CSeq(CSeqHeader::new(invite_cseq.0, Method::Ack)),
    );

    // Route 头部
    for route in route_headers {
        headers.insert(HeaderName::Route, HeaderValue::Route(route));
    }

    Ok(SipRequest {
        request_line: siprs_message::RequestLine {
            method: Method::Ack,
            request_uri,
            version: SipVersion,
        },
        headers,
        body: None,
    })
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use siprs_core::StatusCode;
    use siprs_message::ContactHeader;

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

    /// 创建带 Record-Route 的 200 OK 响应
    fn create_test_200_response_with_record_route() -> SipResponse {
        let mut response = create_test_200_response();

        // 添加 Record-Route 头部
        response.headers.insert(
            HeaderName::RecordRoute,
            HeaderValue::Route(RouteHeader::new(
                SipUri::parse("sip:proxy1.example.com;lr").unwrap(),
            )),
        );
        response.headers.insert(
            HeaderName::RecordRoute,
            HeaderValue::Route(RouteHeader::new(
                SipUri::parse("sip:proxy2.example.com;lr").unwrap(),
            )),
        );

        response
    }

    /// 创建带 Record-Route 的 INVITE 请求
    fn create_test_invite_with_record_route() -> SipRequest {
        let mut invite = create_test_invite();

        invite.headers.insert(
            HeaderName::RecordRoute,
            HeaderValue::Route(RouteHeader::new(
                SipUri::parse("sip:proxy1.example.com;lr").unwrap(),
            )),
        );
        invite.headers.insert(
            HeaderName::RecordRoute,
            HeaderValue::Route(RouteHeader::new(
                SipUri::parse("sip:proxy2.example.com;lr").unwrap(),
            )),
        );

        invite
    }

    // ---- UAC 对话创建测试 ----

    #[test]
    fn test_create_uac_dialog_from_1xx() {
        let invite = create_test_invite();
        let response = create_test_180_response();

        let dialog = create_uac_dialog_from_response(&invite, &response).unwrap();

        assert_eq!(dialog.id.call_id, "test-call-id@example.com");
        assert_eq!(dialog.id.local_tag, "local-tag");
        assert_eq!(dialog.id.remote_tag, "remote-tag");
        assert_eq!(dialog.state, DialogState::Early);
        assert!(dialog.is_uac);
        assert_eq!(dialog.local_cseq.0, 1);
        assert_eq!(dialog.remote_cseq.0, 0);
        assert!(dialog.remote_target.is_some());
    }

    #[test]
    fn test_create_uac_dialog_from_2xx() {
        let invite = create_test_invite();
        let response = create_test_200_response();

        let dialog = create_uac_dialog_from_response(&invite, &response).unwrap();

        assert_eq!(dialog.id.call_id, "test-call-id@example.com");
        assert_eq!(dialog.id.local_tag, "local-tag");
        assert_eq!(dialog.id.remote_tag, "remote-tag");
        assert_eq!(dialog.state, DialogState::Confirmed);
        assert!(dialog.is_uac);
    }

    #[test]
    fn test_create_uac_dialog_with_record_route() {
        let invite = create_test_invite();
        let response = create_test_200_response_with_record_route();

        let dialog = create_uac_dialog_from_response(&invite, &response).unwrap();

        // UAC 侧路由集为 Record-Route 的逆序
        assert_eq!(dialog.route_set.len(), 2);
        assert_eq!(
            dialog.route_set[0].uri.to_string(),
            "sip:proxy2.example.com;lr"
        );
        assert_eq!(
            dialog.route_set[1].uri.to_string(),
            "sip:proxy1.example.com;lr"
        );
    }

    #[test]
    fn test_create_uac_dialog_missing_to_tag() {
        let invite = create_test_invite();
        let mut response = create_test_180_response();

        // 移除 To tag
        response.headers.remove(&HeaderName::To);
        response.headers.insert(
            HeaderName::To,
            HeaderValue::FromTo(FromToHeader::new(
                SipUri::parse("sip:bob@example.com").unwrap(),
            )),
        );

        let result = create_uac_dialog_from_response(&invite, &response);
        assert!(result.is_err());
    }

    // ---- UAS 对话创建测试 ----

    #[test]
    fn test_create_uas_dialog() {
        let invite = create_test_invite();
        let local_tag = "uas-tag".to_string();

        let dialog = create_uas_dialog(&invite, local_tag.clone(), DialogState::Early).unwrap();

        assert_eq!(dialog.id.call_id, "test-call-id@example.com");
        assert_eq!(dialog.id.local_tag, local_tag);
        assert_eq!(dialog.id.remote_tag, "local-tag"); // From tag
        assert_eq!(dialog.state, DialogState::Early);
        assert!(!dialog.is_uac);
        assert_eq!(dialog.local_cseq.0, 0);
        assert_eq!(dialog.remote_cseq.0, 1);
    }

    #[test]
    fn test_create_uas_dialog_with_record_route() {
        let invite = create_test_invite_with_record_route();
        let local_tag = "uas-tag".to_string();

        let dialog = create_uas_dialog(&invite, local_tag, DialogState::Confirmed).unwrap();

        // UAS 侧路由集为 Record-Route 的正序
        assert_eq!(dialog.route_set.len(), 2);
        assert_eq!(
            dialog.route_set[0].uri.to_string(),
            "sip:proxy1.example.com;lr"
        );
        assert_eq!(
            dialog.route_set[1].uri.to_string(),
            "sip:proxy2.example.com;lr"
        );
    }

    // ---- 对话状态更新测试 ----

    #[test]
    fn test_update_early_dialog_on_2xx() {
        let invite = create_test_invite();
        let response_180 = create_test_180_response();
        let mut dialog = create_uac_dialog_from_response(&invite, &response_180).unwrap();

        assert_eq!(dialog.state, DialogState::Early);

        let response_200 = create_test_200_response();
        let result = update_dialog_on_response(&mut dialog, &response_200).unwrap();

        assert_eq!(result, DialogUpdateResult::Updated);
        assert_eq!(dialog.state, DialogState::Confirmed);
    }

    #[test]
    fn test_update_early_dialog_on_non_2xx_final() {
        let invite = create_test_invite();
        let response_180 = create_test_180_response();
        let mut dialog = create_uac_dialog_from_response(&invite, &response_180).unwrap();

        // 创建 487 响应
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
            HeaderName::CallId,
            HeaderValue::CallId(siprs_message::CallId("test-call-id@example.com".to_string())),
        );
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(CSeqHeader::new(1, Method::Invite)),
        );

        let response_487 = SipResponse {
            status_line: siprs_message::StatusLine {
                version: SipVersion,
                status_code: StatusCode(487),
                reason_phrase: "Request Terminated".to_string(),
            },
            headers,
            body: None,
        };

        let result = update_dialog_on_response(&mut dialog, &response_487).unwrap();
        assert_eq!(result, DialogUpdateResult::EarlyDialogDestroyed);
    }

    #[test]
    fn test_update_confirmed_dialog_on_2xx() {
        let invite = create_test_invite();
        let response = create_test_200_response();
        let mut dialog = create_uac_dialog_from_response(&invite, &response).unwrap();

        assert_eq!(dialog.state, DialogState::Confirmed);

        // 收到另一个 2xx 响应（重传），会更新远端目标
        let result = update_dialog_on_response(&mut dialog, &response).unwrap();
        assert_eq!(result, DialogUpdateResult::Updated);
    }

    #[test]
    fn test_update_confirmed_dialog_on_2xx_no_contact() {
        let invite = create_test_invite();
        let mut response = create_test_200_response();
        let mut dialog = create_uac_dialog_from_response(&invite, &response).unwrap();

        assert_eq!(dialog.state, DialogState::Confirmed);

        // 创建不带 Contact 的 2xx 响应
        response.headers.remove(&HeaderName::Contact);

        let result = update_dialog_on_response(&mut dialog, &response).unwrap();
        assert_eq!(result, DialogUpdateResult::NoChange);
    }

    // ---- 对话内请求构建测试 ----

    #[test]
    fn test_build_in_dialog_request_bye() {
        let invite = create_test_invite();
        let response = create_test_200_response();
        let mut dialog = create_uac_dialog_from_response(&invite, &response).unwrap();

        let request = build_in_dialog_request(&mut dialog, Method::Bye).unwrap();

        assert_eq!(request.request_line.method, Method::Bye);
        // CSeq 应该递增
        assert_eq!(dialog.local_cseq.0, 2);

        // 验证必要头部
        assert!(request.headers.get(&HeaderName::CallId).is_some());
        assert!(request.headers.get(&HeaderName::From).is_some());
        assert!(request.headers.get(&HeaderName::To).is_some());
        assert!(request.headers.get(&HeaderName::CSeq).is_some());
        assert!(request.headers.get(&HeaderName::Via).is_some());
        assert!(request.headers.get(&HeaderName::MaxForwards).is_some());
    }

    #[test]
    fn test_build_in_dialog_request_uses_remote_target() {
        let invite = create_test_invite();
        let response = create_test_200_response();
        let mut dialog = create_uac_dialog_from_response(&invite, &response).unwrap();

        let request = build_in_dialog_request(&mut dialog, Method::Bye).unwrap();

        // Request-URI 应该是远端目标
        assert_eq!(
            request.request_line.request_uri,
            SipUri::parse("sip:bob@192.168.1.2:5060").unwrap()
        );
    }

    #[test]
    fn test_build_in_dialog_request_cseq_increments() {
        let invite = create_test_invite();
        let response = create_test_200_response();
        let mut dialog = create_uac_dialog_from_response(&invite, &response).unwrap();

        assert_eq!(dialog.local_cseq.0, 1);

        let _ = build_in_dialog_request(&mut dialog, Method::Bye).unwrap();
        assert_eq!(dialog.local_cseq.0, 2);

        let _ = build_in_dialog_request(&mut dialog, Method::Invite).unwrap();
        assert_eq!(dialog.local_cseq.0, 3);
    }

    // ---- CSeq 验证测试 ----

    #[test]
    fn test_validate_incoming_cseq_valid() {
        let invite = create_test_invite();
        let local_tag = "uas-tag".to_string();
        let dialog = create_uas_dialog(&invite, local_tag, DialogState::Confirmed).unwrap();

        // dialog.remote_cseq = 1 (from INVITE CSeq)
        // 创建 CSeq=2 的请求
        let mut headers = HeaderCollection::new();
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(CSeqHeader::new(2, Method::Bye)),
        );
        let request = SipRequest {
            request_line: siprs_message::RequestLine {
                method: Method::Bye,
                request_uri: SipUri::parse("sip:alice@example.com").unwrap(),
                version: SipVersion,
            },
            headers,
            body: None,
        };

        let result = validate_incoming_cseq(&dialog, &request);
        assert!(matches!(result, CSeqValidationResult::Valid(CSeqNumber(2))));
    }

    #[test]
    fn test_validate_incoming_cseq_invalid() {
        let invite = create_test_invite();
        let local_tag = "uas-tag".to_string();
        let dialog = create_uas_dialog(&invite, local_tag, DialogState::Confirmed).unwrap();

        // dialog.remote_cseq = 1 (from INVITE CSeq)
        // 创建 CSeq=1 的请求（等于远端 CSeq → 无效）
        let mut headers = HeaderCollection::new();
        headers.insert(
            HeaderName::CSeq,
            HeaderValue::CSeq(CSeqHeader::new(1, Method::Bye)),
        );
        let request = SipRequest {
            request_line: siprs_message::RequestLine {
                method: Method::Bye,
                request_uri: SipUri::parse("sip:alice@example.com").unwrap(),
                version: SipVersion,
            },
            headers,
            body: None,
        };

        let result = validate_incoming_cseq(&dialog, &request);
        assert!(matches!(result, CSeqValidationResult::Invalid { .. }));
    }

    // ---- 2xx 重传检测测试 ----

    #[test]
    fn test_is_invite_2xx_retransmit() {
        let invite = create_test_invite();
        let response = create_test_200_response();
        let dialog = create_uac_dialog_from_response(&invite, &response).unwrap();

        // 已确认的对话收到 2xx → 重传
        assert!(is_invite_2xx_retransmit(&dialog, &response));
    }

    #[test]
    fn test_is_not_retransmit_for_early_dialog() {
        let invite = create_test_invite();
        let response_180 = create_test_180_response();
        let dialog = create_uac_dialog_from_response(&invite, &response_180).unwrap();

        // 早期对话收到 2xx → 不是重传
        let response_200 = create_test_200_response();
        assert!(!is_invite_2xx_retransmit(&dialog, &response_200));
    }

    // ---- ACK 构建测试 ----

    #[test]
    fn test_build_ack_for_2xx() {
        let invite = create_test_invite();
        let response = create_test_200_response();
        let dialog = create_uac_dialog_from_response(&invite, &response).unwrap();

        let ack = build_ack_for_2xx(&dialog, CSeqNumber(1)).unwrap();

        assert_eq!(ack.request_line.method, Method::Ack);
        // ACK 的 CSeq 应与 INVITE 相同
        let cseq = ack
            .headers
            .get(&HeaderName::CSeq)
            .unwrap()
            .as_cseq()
            .unwrap();
        assert_eq!(cseq.sequence.0, 1);
        assert_eq!(cseq.method, Method::Ack);
    }

    // ---- 路由集测试 ----

    #[test]
    fn test_route_set_uac_reversed() {
        let mut headers = HeaderCollection::new();
        headers.insert(
            HeaderName::RecordRoute,
            HeaderValue::Route(RouteHeader::new(
                SipUri::parse("sip:proxy1.example.com;lr").unwrap(),
            )),
        );
        headers.insert(
            HeaderName::RecordRoute,
            HeaderValue::Route(RouteHeader::new(
                SipUri::parse("sip:proxy2.example.com;lr").unwrap(),
            )),
        );

        let routes = build_route_set_uac(&headers);
        assert_eq!(routes.len(), 2);
        // 应该是逆序
        assert_eq!(routes[0].uri.to_string(), "sip:proxy2.example.com;lr");
        assert_eq!(routes[1].uri.to_string(), "sip:proxy1.example.com;lr");
    }

    #[test]
    fn test_route_set_uas_original_order() {
        let mut headers = HeaderCollection::new();
        headers.insert(
            HeaderName::RecordRoute,
            HeaderValue::Route(RouteHeader::new(
                SipUri::parse("sip:proxy1.example.com;lr").unwrap(),
            )),
        );
        headers.insert(
            HeaderName::RecordRoute,
            HeaderValue::Route(RouteHeader::new(
                SipUri::parse("sip:proxy2.example.com;lr").unwrap(),
            )),
        );

        let routes = build_route_set_uas(&headers);
        assert_eq!(routes.len(), 2);
        // 应该是正序
        assert_eq!(routes[0].uri.to_string(), "sip:proxy1.example.com;lr");
        assert_eq!(routes[1].uri.to_string(), "sip:proxy2.example.com;lr");
    }

    #[test]
    fn test_route_set_empty() {
        let headers = HeaderCollection::new();
        let routes_uac = build_route_set_uac(&headers);
        let routes_uas = build_route_set_uas(&headers);
        assert!(routes_uac.is_empty());
        assert!(routes_uas.is_empty());
    }
}
