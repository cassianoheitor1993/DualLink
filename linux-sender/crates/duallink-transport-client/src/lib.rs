//! duallink-transport-client — Phase 5C
//!
//! Sender-side transport layer. Mirrors the Swift `SignalingClient` and
//! `VideoSender` used in mac-client, enabling Rust senders (Linux and Windows)
//! to connect to any DualLink receiver that runs `duallink-transport` (receiver
//! crate).
//!
//! # Architecture
//!
//! ```text
//! Rust Sender (Linux / Windows)          Linux / macOS Receiver
//! ──────────────────────────────         ─────────────────────────────
//! VideoSender  ─── UDP:7878+2n ──────►  UdpReceiver → FrameReassembler
//! SignalingClient ─ TLS:7879+2n ─────►  SignalingServer (TLS)
//! ```
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use duallink_transport_client::{SignalingClient, VideoSender};
//! use duallink_core::StreamConfig;
//!
//! # tokio_test::block_on(async {
//! let mut sig = SignalingClient::connect("192.168.1.100", 0).await.unwrap();
//! let config  = StreamConfig::default();
//! let ack = sig.send_hello("session-1", "My Linux Box", config.clone(), "123456").await.unwrap();
//! assert!(ack.accepted);
//!
//! let video = VideoSender::connect("192.168.1.100", 0).await.unwrap();
//! // … encode frames and call video.send_frame(&frame).await?
//! # })
//! ```

pub mod signaling;
pub mod video_sender;

pub use signaling::{HelloAck, SignalingClient, SignalingWriter};
pub use video_sender::VideoSender;

// ── Port helpers (mirrors duallink-transport receiver) ───────────────────────

pub const VIDEO_PORT: u16 = 7878;
pub const SIGNALING_PORT: u16 = 7879;

/// UDP video port for a given display index: 7878, 7880, 7882, …
#[inline]
pub fn video_port(display_index: u8) -> u16 {
    VIDEO_PORT + (display_index as u16) * 2
}

/// TCP signaling port for a given display index: 7879, 7881, 7883, …
#[inline]
pub fn signaling_port(display_index: u8) -> u16 {
    SIGNALING_PORT + (display_index as u16) * 2
}
