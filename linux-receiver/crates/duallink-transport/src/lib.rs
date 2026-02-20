//! duallink-transport — Sprint 1.4
//!
//! Receives video and control data from the macOS DualLink client.
//!
//! # Architecture
//!
//! ```text
//! macOS                          Linux (this crate)
//! ──────────────────────────     ──────────────────────────────────
//! VideoSender  ──UDP:7878──►  UdpReceiver → FrameReassembler ──►  EncodedFrame channel
//! SignalingClient ─TCP:7879─►  SignalingServer              ──►  SignalingEvent channel
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
//! [17..20] reserved   [u8; 3]
//! [20..]   payload    [u8]     H.264 NAL unit slice
//! ```
//!
//! # Signaling Protocol v1 (matches Signaling.swift)
//!
//! Length-prefixed JSON over TCP:
//! ```text
//! [0..4]  length  u32 BE  byte length of JSON payload
//! [4..]   json    UTF-8   SignalingMessage
//! ```

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use duallink_core::{EncodedFrame, InputEvent, StreamConfig, VideoCodec};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

// ── Ports ──────────────────────────────────────────────────────────────────────

pub const VIDEO_PORT: u16 = 7878;
pub const SIGNALING_PORT: u16 = 7879;

// ── Protocol constants ─────────────────────────────────────────────────────────

const MAGIC: u32 = 0x444C_4E4B;
/// Header bytes written by Swift: magic(4)+frameSeq(4)+fragIdx(2)+fragCount(2)+pts(4)+flags(1)+reserved(3) = 20
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
    // buf[17..20] = reserved
    if frag_count == 0 { return None; }
    let payload = Bytes::copy_from_slice(&buf[HEADER_SIZE..]);
    Some(DualLinkPacket { frame_seq, frag_index, frag_count, pts_ms, is_keyframe: flags & 0x01 != 0, payload })
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
        }
    }
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
    /// Bind UDP:7878 + TCP:7879 and start background Tokio tasks.
    /// Returns an `InputSender` in addition to the frame/event channels.
    pub async fn start() -> anyhow::Result<(
        Self,
        mpsc::Receiver<EncodedFrame>,
        mpsc::Receiver<SignalingEvent>,
        InputSender,
    )> {
        let (frame_tx, frame_rx) = mpsc::channel::<EncodedFrame>(64);
        let (event_tx, event_rx) = mpsc::channel::<SignalingEvent>(16);
        let (input_tx, input_rx) = mpsc::channel::<InputEvent>(256);
        let counter = Arc::new(std::sync::atomic::AtomicU64::new(0));

        // UDP receiver task
        let udp = UdpSocket::bind(format!("0.0.0.0:{VIDEO_PORT}")).await?;
        info!("UDP video receiver bound on 0.0.0.0:{VIDEO_PORT}");
        let counter_clone = Arc::clone(&counter);
        tokio::spawn(async move { run_udp_receiver(udp, frame_tx, counter_clone).await });

        // TCP signaling task
        let tcp = TcpListener::bind(format!("0.0.0.0:{SIGNALING_PORT}")).await?;
        info!("TCP signaling listener bound on 0.0.0.0:{SIGNALING_PORT}");
        tokio::spawn(async move { run_signaling_server(tcp, event_tx, input_rx).await });

        Ok((Self { frames_received: counter }, frame_rx, event_rx, InputSender { tx: input_tx }))
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

async fn run_signaling_server(
    listener: TcpListener,
    event_tx: mpsc::Sender<SignalingEvent>,
    input_rx: mpsc::Receiver<InputEvent>,
) {
    // We only support one client at a time — the input_rx is moved into the
    // first accepted connection's write task.
    let input_rx = Arc::new(tokio::sync::Mutex::new(input_rx));
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                info!("Signaling connection from {}", addr);
                let tx = event_tx.clone();
                let irx = Arc::clone(&input_rx);
                tokio::spawn(async move { handle_signaling_conn(stream, addr, tx, irx).await });
            }
            Err(e) => { warn!("TCP accept error: {}", e); }
        }
    }
}

async fn handle_signaling_conn(
    stream: tokio::net::TcpStream,
    addr: SocketAddr,
    event_tx: mpsc::Sender<SignalingEvent>,
    input_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<InputEvent>>>,
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
