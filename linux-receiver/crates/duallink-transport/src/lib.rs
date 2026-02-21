//! duallink-transport — Phase 4 (TLS-secured signaling)
//!
//! Receives video and control data from the macOS DualLink client.
//!
//! # Architecture
//!
//! ```text
//! macOS                          Linux (this crate)
//! ──────────────────────────     ──────────────────────────────────
//! VideoSender  ──UDP:7878──►  UdpReceiver → FrameReassembler ──►  EncodedFrame channel
//! SignalingClient ─TLS:7879─►  SignalingServer (TLS)         ──►  SignalingEvent channel
//! ```
//!
//! # DualLink UDP Frame Protocol v1 (matches Streaming.swift)
//!
//! ```text
//! [0..4]   magic      u32 BE   0x444C4E4B ("DLNK")
//! [4..8]   frame_seq  u32 BE   monotonic frame counter
//! [8..10]  frag_idx   u16 BE   0-based fragment index
//! [10..12] frag_count u16 BE   total fragments for this frame
//! [12..16] pts_ms     u32 BE   presentation timestamp (ms)
//! [16]     flags      u8       bit0 = keyframe
//! [17]     display_index u8   zero-based display stream index (was reserved[0])
//! [18..20] reserved   [u8; 2]
//! [20..]   payload    [u8]     H.264 NAL unit slice
//! ```
//!
//! # Signaling Protocol v2 (TLS-secured, matches Signaling.swift)
//!
//! Length-prefixed JSON over TLS/TCP:
//! ```text
//! [0..4]  length  u32 BE  byte length of JSON payload
//! [4..]   json    UTF-8   SignalingMessage
//! ```
//!
//! The server generates an ephemeral self-signed certificate at startup.
//! The certificate's SHA-256 fingerprint is displayed alongside a 6-digit
//! pairing PIN that the Mac client must include in its `hello` message.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use duallink_core::{EncodedFrame, InputEvent, StreamConfig, VideoCodec};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::mpsc;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, info, warn};

// ── Ports ──────────────────────────────────────────────────────────────────────

pub const VIDEO_PORT: u16 = 7878;
pub const SIGNALING_PORT: u16 = 7879;

/// UDP video port for a given display index: 7878, 7880, 7882, …
pub fn video_port(display_index: u8) -> u16 {
    VIDEO_PORT + (display_index as u16) * 2
}

/// TCP signaling port for a given display index: 7879, 7881, 7883, …
pub fn signaling_port(display_index: u8) -> u16 {
    SIGNALING_PORT + (display_index as u16) * 2
}

// ── TLS certificate generation ─────────────────────────────────────────────────

/// Ephemeral TLS identity generated at server startup.
pub struct TlsIdentity {
    pub acceptor: TlsAcceptor,
    /// SHA-256 fingerprint of the certificate (hex-encoded, colon-separated).
    pub fingerprint: String,
}

/// Generate a self-signed TLS certificate and return a TlsAcceptor.
pub fn generate_tls_identity() -> anyhow::Result<TlsIdentity> {
    // Install the ring crypto provider as the process-level default.
    // This is required by rustls 0.23+ before any ServerConfig is built.
    // `install_default` fails if already installed — we ignore that error.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let subject_alt_names = vec![
        "duallink.local".to_string(),
        "localhost".to_string(),
        "10.0.1.1".to_string(),
    ];
    let key_pair = rcgen::KeyPair::generate()?;
    let cert_params = rcgen::CertificateParams::new(subject_alt_names)?;
    let cert = cert_params.self_signed(&key_pair)?;

    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivateKeyDer::try_from(key_pair.serialize_der())
        .map_err(|e| anyhow::anyhow!("Failed to serialise private key: {}", e))?;

    // Compute SHA-256 fingerprint
    use std::fmt::Write;
    let digest = sha256_digest(cert_der.as_ref());
    let mut fingerprint = String::with_capacity(3 * digest.len());
    for (i, byte) in digest.iter().enumerate() {
        if i > 0 { fingerprint.push(':'); }
        write!(fingerprint, "{:02X}", byte).unwrap();
    }

    let server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)?;

    let acceptor = TlsAcceptor::from(Arc::new(server_config));

    Ok(TlsIdentity { acceptor, fingerprint })
}

/// SHA-256 digest (no external dep — using built-in implementation).
fn sha256_digest(data: &[u8]) -> [u8; 32] {
    sha2_256(data)
}

/// Minimal SHA-256 implementation (FIPS 180-4).
/// Used only for certificate fingerprint display — not security-critical path.
fn sha2_256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
        0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
        0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
        0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
        0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
        0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
        0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
        0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19,
    ];

    // Pre-processing: padding
    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 { msg.push(0); }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    // Process each 512-bit block
    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[i*4..i*4+4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i-15].rotate_right(7) ^ w[i-15].rotate_right(18) ^ (w[i-15] >> 3);
            let s1 = w[i-2].rotate_right(17) ^ w[i-2].rotate_right(19) ^ (w[i-2] >> 10);
            w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
        }

        let [mut a,mut b,mut c,mut d,mut e,mut f,mut g,mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g; g = f; f = e; e = d.wrapping_add(t1);
            d = c; c = b; b = a; a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c); h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e); h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g); h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, val) in h.iter().enumerate() {
        out[i*4..i*4+4].copy_from_slice(&val.to_be_bytes());
    }
    out
}

/// Generate a random 6-digit pairing PIN.
pub fn generate_pairing_pin() -> String {
    use std::time::SystemTime;
    let seed = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!("{:06}", seed % 1_000_000)
}

// ── Protocol constants ─────────────────────────────────────────────────────────

const MAGIC: u32 = 0x444C_4E4B;
/// Header bytes written by Swift: magic(4)+frameSeq(4)+fragIdx(2)+fragCount(2)+pts(4)+flags(1)+display_index(1)+reserved(2) = 20
const HEADER_SIZE: usize = 20;
const UDP_BUF_SIZE: usize = 65_535;
const REASSEMBLY_TIMEOUT: Duration = Duration::from_secs(2);

// ── Packet parsing ─────────────────────────────────────────────────────────────

#[derive(Debug)]
struct DualLinkPacket {
    frame_seq: u32,
    frag_index: u16,
    frag_count: u16,
    pts_ms: u32,
    is_keyframe: bool,
    /// Zero-based display stream index from byte [17] of the DLNK header.
    display_index: u8,
    payload: Bytes,
}

fn parse_packet(buf: &[u8]) -> Option<DualLinkPacket> {
    if buf.len() < HEADER_SIZE {
        return None;
    }
    let magic = u32::from_be_bytes(buf[0..4].try_into().ok()?);
    if magic != MAGIC {
        debug!("Dropped packet: bad magic 0x{:08X}", magic);
        return None;
    }
    let frame_seq   = u32::from_be_bytes(buf[4..8].try_into().ok()?);
    let frag_index  = u16::from_be_bytes(buf[8..10].try_into().ok()?);
    let frag_count  = u16::from_be_bytes(buf[10..12].try_into().ok()?);
    let pts_ms      = u32::from_be_bytes(buf[12..16].try_into().ok()?);
    let flags       = buf[16];
    let display_index = buf[17];  // byte [17]: display_index (was reserved[0])
    // buf[18..20] = reserved
    if frag_count == 0 { return None; }
    let payload = Bytes::copy_from_slice(&buf[HEADER_SIZE..]);
    Some(DualLinkPacket { frame_seq, frag_index, frag_count, pts_ms, is_keyframe: flags & 0x01 != 0, display_index, payload })
}

// ── Frame reassembler ──────────────────────────────────────────────────────────

struct PartialFrame {
    fragments:      Vec<Option<Bytes>>,
    received_count: u16,
    total_count:    u16,
    pts_ms:         u32,
    is_keyframe:    bool,
    first_seen:     Instant,
}

impl PartialFrame {
    fn new(frag_count: u16, pts_ms: u32, is_keyframe: bool) -> Self {
        Self {
            fragments: vec![None; frag_count as usize],
            received_count: 0,
            total_count: frag_count,
            pts_ms,
            is_keyframe,
            first_seen: Instant::now(),
        }
    }

    /// Returns true when all fragments have arrived.
    fn push(&mut self, index: u16, payload: Bytes) -> bool {
        let idx = index as usize;
        if idx >= self.fragments.len() { return false; }
        if self.fragments[idx].is_none() {
            self.fragments[idx] = Some(payload);
            self.received_count += 1;
        }
        self.received_count == self.total_count
    }

    fn assemble(self) -> Bytes {
        let total: usize = self.fragments.iter().flatten().map(|f| f.len()).sum();
        let mut buf = bytes::BytesMut::with_capacity(total);
        for frag in self.fragments.into_iter().flatten() {
            buf.extend_from_slice(&frag);
        }
        buf.freeze()
    }
}

#[derive(Default)]
struct FrameReassembler {
    frames: HashMap<u32, PartialFrame>,
}

impl FrameReassembler {
    fn push(&mut self, packet: DualLinkPacket) -> Option<EncodedFrame> {
        // Evict stale partial frames
        let now = Instant::now();
        self.frames.retain(|seq, f| {
            let keep = now.duration_since(f.first_seen) <= REASSEMBLY_TIMEOUT;
            if !keep { warn!("Dropped stale partial frame seq={}", seq); }
            keep
        });

        let seq = packet.frame_seq;
        let entry = self.frames.entry(seq).or_insert_with(|| {
            PartialFrame::new(packet.frag_count, packet.pts_ms, packet.is_keyframe)
        });

        if !entry.push(packet.frag_index, packet.payload) {
            return None; // frame not complete yet
        }

        let partial = self.frames.remove(&seq)?;
        let pts_ms = partial.pts_ms;
        let is_keyframe = partial.is_keyframe;
        let data = partial.assemble();
        debug!("Assembled frame seq={} {} bytes keyframe={}", seq, data.len(), is_keyframe);

        Some(EncodedFrame {
            data,
            timestamp_us: pts_ms as u64 * 1_000,
            is_keyframe,
            codec: VideoCodec::H264,
        })
    }
}

// ── Signaling wire types ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum MessageType {
    Hello,
    HelloAck,
    ConfigUpdate,
    Keepalive,
    Stop,
    InputEvent,
}

#[derive(Debug, Deserialize, Serialize)]
struct SignalingMessage {
    #[serde(rename = "type")]
    msg_type: MessageType,
    #[serde(rename = "sessionID", skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(rename = "deviceName", skip_serializing_if = "Option::is_none")]
    device_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    config: Option<StreamConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    accepted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(rename = "timestampMs", skip_serializing_if = "Option::is_none")]
    timestamp_ms: Option<u64>,
    #[serde(rename = "inputEvent", skip_serializing_if = "Option::is_none")]
    input_event: Option<InputEvent>,
    #[serde(rename = "pairingPin", skip_serializing_if = "Option::is_none")]
    pairing_pin: Option<String>,
    #[serde(rename = "displayIndex", skip_serializing_if = "Option::is_none")]
    display_index: Option<u8>,
}

impl SignalingMessage {
    fn hello_ack(session_id: String, accepted: bool, reason: Option<String>) -> Self {
        Self {
            msg_type: MessageType::HelloAck,
            session_id: Some(session_id),
            device_name: None,
            config: None,
            accepted: Some(accepted),
            reason,
            timestamp_ms: None,
            input_event: None,
            pairing_pin: None,
            display_index: None,
        }
    }

    fn input_event(event: InputEvent) -> Self {
        Self {
            msg_type: MessageType::InputEvent,
            session_id: None,
            device_name: None,
            config: None,
            accepted: None,
            reason: None,
            timestamp_ms: None,
            input_event: Some(event),
            pairing_pin: None,
            display_index: None,
        }
    }
}

// ── Public startup info ───────────────────────────────────────────────────────

/// Initial values produced once by [`DualLinkReceiver::start`] that callers
/// need to display in a UI or log.
#[derive(Debug, Clone)]
pub struct StartupInfo {
    /// 6-digit pairing PIN shown to the user.
    pub pairing_pin: String,
    /// Hex SHA-256 fingerprint of the ephemeral TLS cert (for TOFU display).
    pub tls_fingerprint: String,
}

// ── Public event type ──────────────────────────────────────────────────────────

/// Events emitted by the SignalingServer to the rest of the app.
#[derive(Debug)]
pub enum SignalingEvent {
    SessionStarted {
        session_id: String,
        device_name: String,
        config: StreamConfig,
        client_addr: SocketAddr,
    },
    ConfigUpdated { config: StreamConfig },
    SessionStopped { session_id: String },
    ClientDisconnected,
}

// ── Multi-display channel bundle ───────────────────────────────────────────────

/// Frame and signaling channels for one display stream.
/// Returned by [`DualLinkReceiver::start_all`].
pub struct DisplayChannels {
    /// Decoded frame stream for this display index.
    pub frame_rx: mpsc::Receiver<EncodedFrame>,
    /// Signaling events for this display index.
    pub event_rx: mpsc::Receiver<SignalingEvent>,
    /// Zero-based display index (matches DLNK header byte [17]).
    pub display_index: u8,
}

// ── DualLinkReceiver ───────────────────────────────────────────────────────────

/// Manages UDP video reception + TCP signaling in background tasks.
///
/// # Example
/// ```rust,no_run
/// # tokio_test::block_on(async {
/// let (_recv, mut frame_rx, mut event_rx) = duallink_transport::DualLinkReceiver::start().await.unwrap();
/// while let Some(frame) = frame_rx.recv().await {
///     println!("frame {} bytes keyframe={}", frame.data.len(), frame.is_keyframe);
/// }
/// # })
/// ```
/// Sender handle for pushing input events to the connected Mac client.
///
/// Uses the same TCP signaling connection (Linux → Mac direction).
/// Clone-able and Send — pass to the decode thread.
#[derive(Clone)]
pub struct InputSender {
    tx: mpsc::Sender<InputEvent>,
}

impl InputSender {
    /// Send an input event to the Mac client.
    /// Non-blocking — returns Err only if the channel is full/closed.
    pub async fn send(&self, event: InputEvent) -> Result<(), mpsc::error::SendError<InputEvent>> {
        self.tx.send(event).await
    }

    /// Try send without awaiting (for use in blocking contexts).
    pub fn try_send(&self, event: InputEvent) -> Result<(), mpsc::error::TrySendError<InputEvent>> {
        self.tx.try_send(event)
    }
}

pub struct DualLinkReceiver {
    pub frames_received: Arc<std::sync::atomic::AtomicU64>,
}

impl DualLinkReceiver {
    /// Bind UDP:7878 + TLS/TCP:7879 and start background Tokio tasks.
    /// Returns an `InputSender` in addition to the frame/event channels.
    ///
    /// Generates an ephemeral self-signed TLS certificate and a 6-digit
    /// pairing PIN.  Both are printed to the console for the user.
    pub async fn start() -> anyhow::Result<(
        Self,
        mpsc::Receiver<EncodedFrame>,
        mpsc::Receiver<SignalingEvent>,
        InputSender,
        StartupInfo,
    )> {
        let (frame_tx, frame_rx) = mpsc::channel::<EncodedFrame>(64);
        let (event_tx, event_rx) = mpsc::channel::<SignalingEvent>(16);
        let (input_tx, input_rx) = mpsc::channel::<InputEvent>(256);
        let counter = Arc::new(std::sync::atomic::AtomicU64::new(0));

        // ── Generate TLS identity ──────────────────────────────────────────
        let identity = generate_tls_identity()?;
        info!("TLS certificate fingerprint: {}", identity.fingerprint);

        let pairing_pin = generate_pairing_pin();
        info!("╔══════════════════════════════════════╗");
        info!("║  DualLink Pairing PIN:  {}        ║", pairing_pin);
        info!("╚══════════════════════════════════════╝");

        let acceptor = identity.acceptor;
        let startup_fingerprint = identity.fingerprint.clone();
        let pin = pairing_pin;
        let startup_pin = pin.clone();
        let shared_input = Arc::new(tokio::sync::Mutex::new(input_rx));

        // UDP receiver task
        let udp = UdpSocket::bind(format!("0.0.0.0:{VIDEO_PORT}")).await?;
        info!("UDP video receiver bound on 0.0.0.0:{VIDEO_PORT}");
        let counter_clone = Arc::clone(&counter);
        tokio::spawn(async move { run_udp_receiver(udp, frame_tx, counter_clone).await });

        // TLS signaling task
        let tcp = TcpListener::bind(format!("0.0.0.0:{SIGNALING_PORT}")).await?;
        info!("TLS signaling listener bound on 0.0.0.0:{SIGNALING_PORT}");
        tokio::spawn(async move {
            run_signaling_server_shared(tcp, event_tx, shared_input, acceptor, pin).await
        });

        Ok((
            Self { frames_received: counter },
            frame_rx,
            event_rx,
            InputSender { tx: input_tx },
            StartupInfo { pairing_pin: startup_pin, tls_fingerprint: startup_fingerprint },
        ))
    }

    /// Bind N display port pairs and start independent background tasks for each.
    ///
    /// All displays share a single TLS identity, pairing PIN, and `InputSender`.
    /// Per-display data comes back through the returned `Vec<DisplayChannels>`.
    ///
    /// Port mapping: display `n` uses UDP `7878 + 2n` / TCP `7879 + 2n`.
    ///
    /// # Example
    /// ```rust,no_run
    /// # tokio_test::block_on(async {
    /// let (_recv, channels, input_tx, _info) =
    ///     duallink_transport::DualLinkReceiver::start_all(2).await.unwrap();
    /// for ch in channels {
    ///     println!("Display {} ready", ch.display_index);
    /// }
    /// # })
    /// ```
    pub async fn start_all(display_count: u8) -> anyhow::Result<(
        Self,
        Vec<DisplayChannels>,
        InputSender,
        StartupInfo,
    )> {
        let n_displays = display_count.max(1).min(8);

        // ── Shared TLS identity + pairing PIN ─────────────────────────────
        let identity = generate_tls_identity()?;
        info!("TLS certificate fingerprint: {}", identity.fingerprint);

        let pairing_pin = generate_pairing_pin();
        info!("╔══════════════════════════════════════╗");
        info!("║  DualLink Pairing PIN:  {}        ║", pairing_pin);
        info!("╚══════════════════════════════════════╝");
        info!("  Displays: {}", n_displays);

        let (input_tx, input_rx) = mpsc::channel::<InputEvent>(256);
        // Shared across all N signaling servers — only display-0 responds actively
        let shared_input = Arc::new(tokio::sync::Mutex::new(input_rx));
        let counter = Arc::new(std::sync::atomic::AtomicU64::new(0));

        let startup_pin = pairing_pin.clone();
        let startup_fingerprint = identity.fingerprint.clone();

        let mut channels = Vec::with_capacity(n_displays as usize);

        for n in 0..n_displays {
            let (frame_tx, frame_rx) = mpsc::channel::<EncodedFrame>(64);
            let (event_tx, event_rx) = mpsc::channel::<SignalingEvent>(16);

            let vp = video_port(n);
            let sp = signaling_port(n);

            let udp = UdpSocket::bind(format!("0.0.0.0:{vp}")).await?;
            info!("Display[{n}] UDP receiver bound on 0.0.0.0:{vp}");
            let counter_clone = Arc::clone(&counter);
            tokio::spawn(async move { run_udp_receiver(udp, frame_tx, counter_clone).await });

            let tcp = TcpListener::bind(format!("0.0.0.0:{sp}")).await?;
            info!("Display[{n}] TLS signaling bound on 0.0.0.0:{sp}");
            let acceptor = identity.acceptor.clone();
            let pin = pairing_pin.clone();
            let irx = Arc::clone(&shared_input);
            tokio::spawn(async move {
                run_signaling_server_shared(tcp, event_tx, irx, acceptor, pin).await
            });

            channels.push(DisplayChannels { frame_rx, event_rx, display_index: n });
        }

        Ok((
            Self { frames_received: counter },
            channels,
            InputSender { tx: input_tx },
            StartupInfo { pairing_pin: startup_pin, tls_fingerprint: startup_fingerprint },
        ))
    }
}

// ── UDP task ───────────────────────────────────────────────────────────────────

async fn run_udp_receiver(
    socket: UdpSocket,
    frame_tx: mpsc::Sender<EncodedFrame>,
    counter: Arc<std::sync::atomic::AtomicU64>,
) {
    let mut buf = vec![0u8; UDP_BUF_SIZE];
    let mut reassembler = FrameReassembler::default();

    loop {
        let (len, addr) = match socket.recv_from(&mut buf).await {
            Ok(v) => v,
            Err(e) => { warn!("UDP recv error: {}", e); continue; }
        };

        let Some(packet) = parse_packet(&buf[..len]) else {
            debug!("Dropped malformed packet from {}", addr);
            continue;
        };

        if let Some(frame) = reassembler.push(packet) {
            counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if frame_tx.send(frame).await.is_err() {
                info!("frame_tx closed — stopping UDP receiver");
                return;
            }
        }
    }
}

// ── TCP signaling task ─────────────────────────────────────────────────────────

async fn run_signaling_server_shared(
    listener: TcpListener,
    event_tx: mpsc::Sender<SignalingEvent>,
    input_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<InputEvent>>>,
    acceptor: TlsAcceptor,
    pairing_pin: String,
) {
    // We only support one client at a time — the input_rx is shared across displays.
    let input_rx = input_rx;
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                info!("TCP connection from {} — performing TLS handshake...", addr);
                let acc = acceptor.clone();
                match acc.accept(stream).await {
                    Ok(tls_stream) => {
                        info!("TLS handshake OK with {}", addr);
                        let tx = event_tx.clone();
                        let irx = Arc::clone(&input_rx);
                        let pin = pairing_pin.clone();
                        tokio::spawn(async move {
                            handle_signaling_conn(tls_stream, addr, tx, irx, pin).await
                        });
                    }
                    Err(e) => {
                        warn!("TLS handshake failed from {}: {}", addr, e);
                    }
                }
            }
            Err(e) => { warn!("TCP accept error: {}", e); }
        }
    }
}

async fn handle_signaling_conn(
    stream: tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
    addr: SocketAddr,
    event_tx: mpsc::Sender<SignalingEvent>,
    input_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<InputEvent>>>,
    expected_pin: String,
) {
    let (reader, writer) = tokio::io::split(stream);
    let writer = Arc::new(tokio::sync::Mutex::new(writer));

    // ── Reader: process incoming signaling messages ────────────────────────
    let writer_for_reader = Arc::clone(&writer);
    let mut reader = reader;
    let mut body_buf = Vec::new();
    let mut session_active = false;

    loop {
        let mut len_bytes = [0u8; 4];
        if reader.read_exact(&mut len_bytes).await.is_err() {
            let _ = event_tx.send(SignalingEvent::ClientDisconnected).await;
            break;
        }
        let msg_len = u32::from_be_bytes(len_bytes) as usize;

        body_buf.resize(msg_len, 0);
        if reader.read_exact(&mut body_buf).await.is_err() {
            let _ = event_tx.send(SignalingEvent::ClientDisconnected).await;
            break;
        }

        let msg: SignalingMessage = match serde_json::from_slice(&body_buf) {
            Ok(m) => m,
            Err(e) => { warn!("Bad signaling JSON from {}: {}", addr, e); continue; }
        };

        match msg.msg_type {
            MessageType::Hello => {
                let session_id  = msg.session_id.unwrap_or_default();
                let device_name = msg.device_name.unwrap_or_else(|| addr.to_string());
                let config      = msg.config.unwrap_or_default();
                info!("Hello from '{}' session={}", device_name, session_id);

                // ── Validate pairing PIN ──────────────────────────────────
                let client_pin = msg.pairing_pin.unwrap_or_default();
                if client_pin != expected_pin {
                    warn!("Pairing PIN mismatch from {} — rejecting (got '{}', expected '{}')",
                          addr, client_pin, expected_pin);
                    let ack = SignalingMessage::hello_ack(
                        session_id,
                        false,
                        Some("Invalid pairing PIN".into()),
                    );
                    {
                        let mut w = writer_for_reader.lock().await;
                        let _ = send_msg_split(&mut *w, &ack).await;
                    }
                    break;
                }
                info!("Pairing PIN accepted from {}", addr);

                // Respond with hello_ack
                let ack = SignalingMessage::hello_ack(session_id.clone(), true, None);
                {
                    let mut w = writer_for_reader.lock().await;
                    if send_msg_split(&mut *w, &ack).await.is_err() { break; }
                }

                let _ = event_tx.send(SignalingEvent::SessionStarted {
                    session_id, device_name, config, client_addr: addr,
                }).await;

                // Start forwarding input events now that session is active
                if !session_active {
                    session_active = true;
                    let w = Arc::clone(&writer);
                    let irx = Arc::clone(&input_rx);
                    tokio::spawn(async move {
                        let mut input_rx = irx.lock().await;
                        let mut events_sent: u64 = 0;
                        while let Some(event) = input_rx.recv().await {
                            let msg = SignalingMessage::input_event(event);
                            let mut w = w.lock().await;
                            if send_msg_split(&mut *w, &msg).await.is_err() { break; }
                            events_sent += 1;
                            if events_sent == 1 {
                                info!("First input event sent to Mac client");
                            }
                        }
                        debug!("Input writer task exiting (sent {} events)", events_sent);
                    });
                }
            }
            MessageType::ConfigUpdate => {
                if let Some(config) = msg.config {
                    let _ = event_tx.send(SignalingEvent::ConfigUpdated { config }).await;
                }
            }
            MessageType::Keepalive => {
                debug!("Keepalive from {} ts={:?}", addr, msg.timestamp_ms);
            }
            MessageType::Stop => {
                let session_id = msg.session_id.unwrap_or_default();
                info!("Stop from {} session={}", addr, session_id);
                let _ = event_tx.send(SignalingEvent::SessionStopped { session_id }).await;
                break;
            }
            MessageType::HelloAck | MessageType::InputEvent => { /* not expected from client */ }
        }
    }
}

async fn send_msg_split<W: AsyncWriteExt + Unpin>(writer: &mut W, msg: &SignalingMessage) -> std::io::Result<()> {
    let json = serde_json::to_vec(msg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    writer.write_all(&(json.len() as u32).to_be_bytes()).await?;
    writer.write_all(&json).await?;
    writer.flush().await
}
