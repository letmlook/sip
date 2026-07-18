//! SIP Rustls TLS 传输实现
//!
//! 使用 `tokio-rustls` 和 `rustls` 实现 TLS 加密的 SIP 传输。
//! 支持 TLS 1.2 和 TLS 1.3，支持服务端证书验证。

use std::net::SocketAddr;
use std::sync::Arc;

use bytes::BytesMut;
use futures::{SinkExt, StreamExt};
use rustls::ClientConfig;
use sip_core::{TlsError, TransportError, TransportProtocol};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_util::codec::Framed;
use tracing;

use crate::codec::SipCodec;
use crate::traits::{ReceivedMessage, TransportEvent};

// ============================================================================
// TlsConnection - TLS 连接
// ============================================================================

/// TLS 连接
///
/// 在 TCP 连接基础上增加 TLS 加密层，使用 `Framed<TlsStream, SipCodec>` 进行
/// 基于 Content-Length 的消息分帧。
pub struct TlsConnection {
    stream: Framed<tokio_rustls::client::TlsStream<TcpStream>, SipCodec>,
    peer_addr: SocketAddr,
    local_addr: SocketAddr,
}

impl TlsConnection {
    /// 连接到远端 TLS 服务器
    ///
    /// # 参数
    ///
    /// - `addr` - 远端地址
    /// - `max_message_size` - 最大消息大小限制
    /// - `verify_certificate` - 是否验证服务端证书
    /// - `server_name` - TLS SNI 服务器名称（应从 SIP URI 的 host 部分提取，需为 'static 生命周期）
    ///
    /// # 错误
    ///
    /// - `TlsError::HandshakeFailed` - TLS 握手失败
    /// - `TlsError::CertificateVerification` - 证书验证失败
    /// - `TransportError::ConnectionFailed` - TCP 连接失败
    pub async fn connect(
        addr: SocketAddr,
        max_message_size: usize,
        verify_certificate: bool,
        server_name: impl Into<String>,
    ) -> Result<Self, TransportError> {
        // 建立 TCP 连接
        let tcp_stream =
            TcpStream::connect(addr)
                .await
                .map_err(|e| TransportError::ConnectionFailed {
                    addr: addr.to_string(),
                    reason: format!("TCP connect failed: {}", e),
                })?;

        let local_addr = tcp_stream
            .local_addr()
            .map_err(|e| TransportError::ConnectionFailed {
                addr: addr.to_string(),
                reason: format!("get local addr failed: {}", e),
            })?;

        // 配置 TLS 客户端
        let client_config = create_client_config(verify_certificate).map_err(|e| {
            TransportError::ConnectionFailed {
                addr: addr.to_string(),
                reason: format!("TLS config error: {}", e),
            }
        })?;
        let connector = TlsConnector::from(client_config);

        // 使用提供的 server_name 作为 SNI（应从 SIP URI 的 host 部分提取）
        let server_name_str = server_name.into();
        let server_name =
            rustls_pki_types::ServerName::try_from(server_name_str.clone()).map_err(|e| {
                TransportError::ConnectionFailed {
                    addr: addr.to_string(),
                    reason: format!("invalid server name: {}", e),
                }
            })?;

        // TLS 握手
        let tls_stream = connector
            .connect(server_name, tcp_stream)
            .await
            .map_err(|e| TransportError::ConnectionFailed {
                addr: addr.to_string(),
                reason: format!("TLS handshake failed: {}", e),
            })?;

        let codec = SipCodec::new(max_message_size);
        let framed = Framed::new(tls_stream, codec);

        Ok(Self {
            stream: framed,
            peer_addr: addr,
            local_addr,
        })
    }

    /// 获取对端地址
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    /// 获取本地地址
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// 获取传输协议类型
    pub fn protocol(&self) -> TransportProtocol {
        TransportProtocol::Tls
    }

    /// 发送原始字节消息
    pub async fn send_raw(&mut self, message: BytesMut) -> Result<(), TransportError> {
        self.stream
            .send(message)
            .await
            .map_err(|e| TransportError::SendFailed {
                reason: format!("TLS send to {} failed: {}", self.peer_addr, e),
            })
    }

    /// 启动 TLS 连接的接收循环
    ///
    /// 从 Framed 流中持续读取消息，解析后通过事件通道发送。
    pub async fn receive_loop(mut self, event_tx: tokio::sync::mpsc::Sender<TransportEvent>) {
        let peer_addr = self.peer_addr;
        let parser = sip_message::MessageParser::default();

        // 通知连接建立
        let _ = event_tx
            .send(TransportEvent::ConnectionEstablished(
                peer_addr,
                TransportProtocol::Tls,
            ))
            .await;

        loop {
            match self.stream.next().await {
                Some(Ok(data)) => match parser.parse(&data) {
                    Ok(message) => {
                        let received = ReceivedMessage {
                            message,
                            source_addr: peer_addr,
                            transport: TransportProtocol::Tls,
                        };
                        if event_tx
                            .send(TransportEvent::Message(Box::new(received)))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("TLS: failed to parse message from {}: {}", peer_addr, e);
                    }
                },
                Some(Err(e)) => {
                    tracing::warn!("TLS read error from {}: {}", peer_addr, e);
                    break;
                }
                None => {
                    tracing::debug!("TLS connection closed: {}", peer_addr);
                    break;
                }
            }
        }

        // 通知连接断开
        let _ = event_tx
            .send(TransportEvent::ConnectionLost(
                peer_addr,
                TransportProtocol::Tls,
            ))
            .await;
    }
}

/// 创建 TLS 客户端配置
///
/// 当 `verify_certificate` 为 true 时，使用 webpki_roots 进行证书验证。
/// 当 `verify_certificate` 为 false 时：
/// - 在测试构建中，使用 `NoVerifier` 跳过证书验证
/// - 在生产构建中，返回错误（不允许跳过证书验证）
fn create_client_config(verify_certificate: bool) -> Result<Arc<ClientConfig>, TlsError> {
    let config = if verify_certificate {
        // 使用 webpki_roots 进行证书验证
        let root_store = get_root_cert_store();
        ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth()
    } else {
        #[cfg(test)]
        {
            // 仅在测试构建中允许跳过证书验证
            ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(NoVerifier))
                .with_no_client_auth()
        }
        #[cfg(not(test))]
        {
            return Err(TlsError::CertificateVerification {
                reason: "certificate verification cannot be disabled in production builds"
                    .to_string(),
            });
        }
    };

    Ok(Arc::new(config))
}

/// 获取根证书存储
fn get_root_cert_store() -> rustls::RootCertStore {
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    root_store
}

// ============================================================================
// NoVerifier - 跳过证书验证（仅用于测试）
// ============================================================================

/// 跳过证书验证的验证器（仅用于测试环境）
///
/// # Safety
///
/// 此验证器跳过所有证书验证，包括：
/// - 服务器证书链验证
/// - 服务器名称（SNI）匹配
/// - 证书有效期检查
/// - TLS 签名验证
///
/// **绝对不要在生产环境使用**，否则将面临中间人攻击风险。
/// 仅用于本地开发测试或受控的测试环境中。
///
/// 此类型仅在测试构建中可用（`#[cfg(test)]`），
/// 生产构建中无法实例化，从编译层面防止误用。
#[cfg(test)]
#[derive(Debug)]
struct NoVerifier;

#[cfg(test)]
impl rustls::client::danger::ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls_pki_types::CertificateDer<'_>,
        _intermediates: &[rustls_pki_types::CertificateDer<'_>],
        _server_name: &rustls_pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls_pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls_pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls_pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
        ]
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_client_config_with_verify() {
        let config = create_client_config(true);
        assert!(config.is_ok());
    }

    #[test]
    fn test_create_client_config_without_verify() {
        let config = create_client_config(false);
        assert!(config.is_ok());
    }
}
