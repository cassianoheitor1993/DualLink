//! TLS TCP signaling **client** (sender role).
//!
//! Mirrors `Signaling.swift`'s `SignalingClient` for the Rust platform senders.
//!
//! # Lifecycle
//!
//! ```text
//! 1. SignalingClient::connect(host, display_index)
//! 2. client.send_hello(session_id, device_name, config, pairing_pin)
//!       └─ returns HelloAck { accepted, reason }
//! 3. let (writer, input_rx) = client.start_recv_loop()
//!       ├─ writer: SignalingWriter for keepalive / stop / config_update
//!       └─ input_rx: channel for InputEvents from the receiver
//! 4. writer.send_keepalive(timestamp_ms)  ← every 1 Hz
//! 5. writer.send_stop(session_id)
//! ```

use std::sync::Arc;

use anyhow::Context;
use duallink_core::{InputEvent, StreamConfig};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::signaling_port;

// ── Internal alias ────────────────────────────────────────────────────────────

type TlsClientStream = tokio_rustls::client::TlsStream<TcpStream>;

// ── Signaling wire types (mirrors duallink-transport/src/lib.rs) ─────────────

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MessageType {
    Hello,
    HelloAck,
    ConfigUpdate,
    Keepalive,
    Stop,
    InputEvent,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct SignalingMessage {
    #[serde(rename = "type")]
    pub msg_type: MessageType,
    #[serde(rename = "sessionID", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(rename = "deviceName", skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<StreamConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(rename = "timestampMs", skip_serializing_if = "Option::is_none")]
    pub timestamp_ms: Option<u64>,
    #[serde(rename = "inputEvent", skip_serializing_if = "Option::is_none")]
    pub input_event: Option<InputEvent>,
    #[serde(rename = "pairingPin", skip_serializing_if = "Option::is_none")]
    pub pairing_pin: Option<String>,
    #[serde(rename = "displayIndex", skip_serializing_if = "Option::is_none")]
    pub display_index: Option<u8>,
}

impl SignalingMessage {
    pub(crate) fn hello(
        session_id: &str,
        device_name: &str,
        config: StreamConfig,
        pairing_pin: &str,
        display_index: u8,
    ) -> Self {
        Self {
            msg_type: MessageType::Hello,
            session_id: Some(session_id.to_owned()),
            device_name: Some(device_name.to_owned()),
            config: Some(config),
            accepted: None,
            reason: None,
            timestamp_ms: None,
            input_event: None,
            pairing_pin: Some(pairing_pin.to_owned()),
            display_index: Some(display_index),
        }
    }

    pub(crate) fn keepalive(timestamp_ms: u64) -> Self {
        Self {
            msg_type: MessageType::Keepalive,
            session_id: None,
            device_name: None,
            config: None,
            accepted: None,
            reason: None,
            timestamp_ms: Some(timestamp_ms),
            input_event: None,
            pairing_pin: None,
            display_index: None,
        }
    }

    pub(crate) fn config_update(session_id: &str, config: StreamConfig) -> Self {
        Self {
            msg_type: MessageType::ConfigUpdate,
            session_id: Some(session_id.to_owned()),
            device_name: None,
            config: Some(config),
            accepted: None,
            reason: None,
            timestamp_ms: None,
            input_event: None,
            pairing_pin: None,
            display_index: None,
        }
    }

    pub(crate) fn stop(session_id: &str) -> Self {
        Self {
            msg_type: MessageType::Stop,
            session_id: Some(session_id.to_owned()),
            device_name: None,
            config: None,
            accepted: None,
            reason: None,
            timestamp_ms: None,
            input_event: None,
            pairing_pin: None,
            display_index: None,
        }
    }
}

// ── Length-prefixed framing ───────────────────────────────────────────────────

async fn write_msg(
    stream: &mut (impl AsyncWriteExt + Unpin),
    msg: &SignalingMessage,
) -> anyhow::Result<()> {
    let json = serde_json::to_vec(msg)?;
    let len = json.len() as u32;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(&json).await?;
    stream.flush().await?;
    debug!("Sent {:?} ({} bytes)", msg.msg_type, json.len());
    Ok(())
}

async fn read_msg(
    stream: &mut (impl AsyncReadExt + Unpin),
) -> anyhow::Result<SignalingMessage> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await.context("reading message length")?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 1_048_576 {
        anyhow::bail!("Message too large: {} bytes", len);
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await.context("reading message body")?;
    let msg: SignalingMessage = serde_json::from_slice(&body).context("parsing signaling message")?;
    debug!("Received {:?} ({} bytes)", msg.msg_type, len);
    Ok(msg)
}

// ── TOFU certificate verifier (accepts any self-signed cert) ─────────────────

#[derive(Debug)]
struct TofuCertVerifier;

impl rustls::client::danger::ServerCertVerifier for TofuCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        // TOFU: accept any self-signed certificate.
        // Production: pin the SHA-256 fingerprint on first connect.
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

// ── Public result types ───────────────────────────────────────────────────────

/// Result of the `hello` / `hello_ack` handshake.
#[derive(Debug, Clone)]
pub struct HelloAck {
    pub accepted: bool,
    pub reason: Option<String>,
    pub session_id: Option<String>,
}

// ── SignalingClient ───────────────────────────────────────────────────────────

/// Manages the TLS TCP control channel to a DualLink receiver (sender role).
///
/// Use [`SignalingClient::connect`] to open the connection, then
/// [`send_hello`](SignalingClient::send_hello) for the initial handshake. Once
/// accepted, call [`start_recv_loop`](SignalingClient::start_recv_loop) to
/// obtain a [`SignalingWriter`] + an `InputEvent` channel.
pub struct SignalingClient {
    stream: TlsClientStream,
    display_index: u8,
}

impl SignalingClient {
    // ── Construction ─────────────────────────────────────────────────────────

    /// Connect to a DualLink receiver at `host`, auto-resolving the signaling
    /// port from `display_index` (port = 7879 + 2 × display_index).
    pub async fn connect(host: &str, display_index: u8) -> anyhow::Result<Self> {
        let port = signaling_port(display_index);
        Self::connect_with_port(host, port, display_index).await
    }

    /// Connect with an explicit port number.
    pub async fn connect_with_port(
        host: &str,
        port: u16,
        display_index: u8,
    ) -> anyhow::Result<Self> {
        // Install ring crypto provider (ignored if already installed)
        let _ = rustls::crypto::ring::default_provider().install_default();

        let client_config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(TofuCertVerifier))
            .with_no_client_auth();

        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));

        let tcp = TcpStream::connect((host, port))
            .await
            .with_context(|| format!("TCP connect to {}:{}", host, port))?;
        tcp.set_nodelay(true)?;

        // Build a ServerName for SNI/handshake.  IP addresses and DNS names
        // are both handled; the cert is accepted regardless (TOFU).
        let server_name: rustls::pki_types::ServerName =
            if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                rustls::pki_types::ServerName::IpAddress(ip.into())
            } else {
                rustls::pki_types::ServerName::try_from(host.to_owned())
                    .map_err(|_| anyhow::anyhow!("Invalid hostname: {}", host))?
            };

        let tls = connector
            .connect(server_name, tcp)
            .await
            .with_context(|| format!("TLS handshake with {}:{}", host, port))?;

        info!("Signaling connected to {}:{} (display_index={})", host, port, display_index);
        Ok(Self { stream: tls, display_index })
    }

    // ── Handshake ─────────────────────────────────────────────────────────────

    /// Send `hello` and wait for `hello_ack`.
    ///
    /// Returns [`HelloAck`] which indicates whether the receiver accepted the
    /// session. On rejection, the connection should be closed.
    pub async fn send_hello(
        &mut self,
        session_id: &str,
        device_name: &str,
        config: StreamConfig,
        pairing_pin: &str,
    ) -> anyhow::Result<HelloAck> {
        let msg = SignalingMessage::hello(
            session_id,
            device_name,
            config,
            pairing_pin,
            self.display_index,
        );
        write_msg(&mut self.stream, &msg).await?;
        info!("Sent hello (session={}, display={})", session_id, self.display_index);

        // Wait for hello_ack — ignore any non-ack messages (defensive)
        loop {
            let reply = read_msg(&mut self.stream).await?;
            match reply.msg_type {
                MessageType::HelloAck => {
                    let accepted = reply.accepted.unwrap_or(false);
                    let reason = reply.reason.clone();
                    let sid = reply.session_id.clone();
                    if accepted {
                        info!("hello_ack: session accepted (id={:?})", sid);
                    } else {
                        warn!("hello_ack: session rejected: {:?}", reason);
                    }
                    return Ok(HelloAck { accepted, reason, session_id: sid });
                }
                other => {
                    debug!("Ignoring {:?} while waiting for hello_ack", other);
                }
            }
        }
    }

    // ── Post-handshake: split into writer + recv loop ─────────────────────────

    /// Consume this client, spawning a background receive task.
    ///
    /// Returns:
    /// - [`SignalingWriter`] — for sending keepalive / stop / config_update
    /// - `Receiver<InputEvent>` — input events forwarded from the receiver
    pub fn start_recv_loop(self) -> (SignalingWriter, mpsc::Receiver<InputEvent>) {
        let (input_tx, input_rx) = mpsc::channel::<InputEvent>(256);
        let (read_half, write_half) = tokio::io::split(self.stream);
        let display_index = self.display_index;

        tokio::spawn(recv_loop(read_half, input_tx, display_index));

        (SignalingWriter { writer: write_half }, input_rx)
    }
}

// ── Background receive loop ───────────────────────────────────────────────────

async fn recv_loop(
    mut reader: tokio::io::ReadHalf<TlsClientStream>,
    input_tx: mpsc::Sender<InputEvent>,
    display_index: u8,
) {
    loop {
        match read_msg(&mut reader).await {
            Ok(msg) => match msg.msg_type {
                MessageType::InputEvent => {
                    if let Some(event) = msg.input_event {
                        if input_tx.send(event).await.is_err() {
                            debug!("Input channel closed; stopping recv loop (display={})", display_index);
                            return;
                        }
                    }
                }
                MessageType::Stop => {
                    info!("Receiver sent stop (display={})", display_index);
                    return;
                }
                other => {
                    debug!("Recv loop: ignoring {:?} (display={})", other, display_index);
                }
            },
            Err(e) => {
                warn!("Signaling receive error (display={}): {:#}", display_index, e);
                return;
            }
        }
    }
}

// ── SignalingWriter ───────────────────────────────────────────────────────────

/// Write-only handle to the signaling connection, returned by
/// [`SignalingClient::start_recv_loop`].
///
/// Not `Clone` — only one writer at a time.
pub struct SignalingWriter {
    writer: WriteHalf<TlsClientStream>,
}

impl SignalingWriter {
    /// Send a 1-Hz keepalive heartbeat.
    pub async fn send_keepalive(&mut self, timestamp_ms: u64) -> anyhow::Result<()> {
        write_msg(&mut self.writer, &SignalingMessage::keepalive(timestamp_ms)).await
    }

    /// Notify the receiver of a mid-session configuration change.
    pub async fn send_config_update(
        &mut self,
        session_id: &str,
        config: StreamConfig,
    ) -> anyhow::Result<()> {
        write_msg(&mut self.writer, &SignalingMessage::config_update(session_id, config)).await
    }

    /// Gracefully end the session.
    pub async fn send_stop(&mut self, session_id: &str) -> anyhow::Result<()> {
        write_msg(&mut self.writer, &SignalingMessage::stop(session_id)).await
    }
}
