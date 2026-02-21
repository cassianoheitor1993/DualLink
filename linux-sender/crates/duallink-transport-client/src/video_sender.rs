//! UDP DLNK-framed video **sender** (mirrors Swift `VideoSender` + `DualLinkPacket.packetize`).
//!
//! # Packet Layout (20-byte DLNK header)
//!
//! ```text
//! [0..4]   magic         u32 BE  0x444C4E4B ("DLNK")
//! [4..8]   frame_seq     u32 BE  monotonically increasing frame counter
//! [8..10]  frag_index    u16 BE  0-based fragment index within this frame
//! [10..12] frag_count    u16 BE  total fragments for this frame
//! [12..16] pts_ms        u32 BE  presentation timestamp (milliseconds)
//! [16]     flags         u8      bit0 = key-frame
//! [17]     display_index u8      zero-based display stream index
//! [18..20] reserved      [u8;2]  0x00 0x00
//! [20..]   payload       [u8]    H.264 NAL unit slice
//! ```
//!
//! Packet size = 20 (header) + up to `MAX_PAYLOAD_BYTES` payload ≤ ~1404 bytes.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use anyhow::Context;
use duallink_core::EncodedFrame;
use tokio::net::UdpSocket;
use tracing::debug;

use crate::video_port;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum payload bytes per UDP fragment (matches Swift kMaxPayloadBytes).
/// Each UDP datagram = 20-byte header + MAX_PAYLOAD_BYTES ≤ 1404 bytes total.
const MAX_PAYLOAD_BYTES: usize = 1_384;
const HEADER_SIZE: usize = 20;
const MAGIC: u32 = 0x444C_4E4B;

// ── VideoSender ───────────────────────────────────────────────────────────────

/// UDP video sender.  Packetizes [`EncodedFrame`]s into DLNK-header datagrams
/// and fires them at the receiver's UDP video port.
///
/// `VideoSender` is `Clone` — cheap to fan-out across tasks.
#[derive(Clone)]
pub struct VideoSender {
    socket: Arc<UdpSocket>,
    remote_addr: SocketAddr,
    display_index: u8,
    frame_seq: Arc<AtomicU32>,
}

impl VideoSender {
    // ── Construction ─────────────────────────────────────────────────────────

    /// Create a sender targeting `host`, auto-resolving the UDP video port
    /// from `display_index` (port = 7878 + 2 × display_index).
    pub async fn connect(host: &str, display_index: u8) -> anyhow::Result<Self> {
        let port = video_port(display_index);
        Self::connect_with_port(host, port, display_index).await
    }

    /// Create a sender targeting `host:port` with the given display index.
    pub async fn connect_with_port(
        host: &str,
        port: u16,
        display_index: u8,
    ) -> anyhow::Result<Self> {
        let remote: SocketAddr = format!("{}:{}", host, port)
            .parse()
            .with_context(|| format!("Parsing remote address {}:{}", host, port))?;

        // Bind to an OS-assigned local port on all interfaces.
        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .context("Binding UDP socket")?;

        // "Connect" sets the default destination so we use `send()` below.
        socket.connect(remote).await.context("UDP connect")?;

        Ok(Self {
            socket: Arc::new(socket),
            remote_addr: remote,
            display_index,
            frame_seq: Arc::new(AtomicU32::new(0)),
        })
    }

    // ── Sending ───────────────────────────────────────────────────────────────

    /// Packetize and send one encoded frame to the receiver.
    ///
    /// Returns the number of fragments sent.
    pub async fn send_frame(&self, frame: &EncodedFrame) -> anyhow::Result<u32> {
        let data = &frame.data;
        if data.is_empty() {
            return Ok(0);
        }

        let frame_seq = self.frame_seq.fetch_add(1, Ordering::Relaxed);
        let pts_ms = (frame.timestamp_us / 1_000) as u32;
        let flags: u8 = if frame.is_keyframe { 0x01 } else { 0x00 };

        let total_bytes = data.len();
        let num_fragments = ((total_bytes + MAX_PAYLOAD_BYTES - 1) / MAX_PAYLOAD_BYTES).max(1);
        let frag_count = num_fragments as u16;

        for i in 0..num_fragments {
            let offset = i * MAX_PAYLOAD_BYTES;
            let length = (MAX_PAYLOAD_BYTES).min(total_bytes - offset);
            let payload = &data[offset..offset + length];

            let mut datagram = Vec::with_capacity(HEADER_SIZE + length);

            // magic
            datagram.extend_from_slice(&MAGIC.to_be_bytes());
            // frame_seq
            datagram.extend_from_slice(&frame_seq.to_be_bytes());
            // frag_index
            datagram.extend_from_slice(&(i as u16).to_be_bytes());
            // frag_count
            datagram.extend_from_slice(&frag_count.to_be_bytes());
            // pts_ms
            datagram.extend_from_slice(&pts_ms.to_be_bytes());
            // flags
            datagram.push(flags);
            // display_index (byte [17])
            datagram.push(self.display_index);
            // reserved [18..20]
            datagram.extend_from_slice(&[0x00, 0x00]);
            // payload
            datagram.extend_from_slice(payload);

            self.socket
                .send(&datagram)
                .await
                .with_context(|| {
                    format!(
                        "UDP send frag {}/{} to {} (frame_seq={})",
                        i + 1,
                        frag_count,
                        self.remote_addr,
                        frame_seq
                    )
                })?;
        }

        debug!(
            "Sent frame seq={} frags={} bytes={} keyframe={} display={}",
            frame_seq,
            num_fragments,
            total_bytes,
            frame.is_keyframe,
            self.display_index
        );

        Ok(num_fragments as u32)
    }

    // ── Diagnostics ───────────────────────────────────────────────────────────

    /// Remote address this sender is targeting.
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    /// Total frames sent so far (frame sequence counter).
    pub fn frames_sent(&self) -> u32 {
        self.frame_seq.load(Ordering::Relaxed)
    }
}
