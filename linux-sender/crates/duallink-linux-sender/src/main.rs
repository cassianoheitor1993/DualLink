//! DualLink Linux Sender — Phase 5C.
//!
//! Turns a Linux machine into a DualLink **sender**, mirroring or extending its
//! screen to a DualLink **receiver** (another Linux machine, Windows, or macOS).
//!
//! # Architecture
//!
//! ```text
//! Linux (this app)                     Receiver (Linux / Windows / macOS)
//! ───────────────────────────────────  ──────────────────────────────────────
//! PipeWire / XShm capture               display decoder (GStreamer / VT)
//!   │
//!   ▼
//! GStreamer encode
//!   vaapih264enc / nvh264enc / x264enc
//!   │
//!   ▼
//! VideoSender (UDP:7878+2n) ─────────► UdpReceiver → FrameReassembler
//! SignalingClient (TLS:7879+2n) ─────► TLS SignalingServer
//! ```
//!
//! # Phase 5C status
//!
//! - [x] `duallink-capture-linux` — PipeWire portal + GStreamer appsink capture
//! - [x] `duallink-transport-client` — TLS signaling client + UDP DLNK sender
//! - [x] `duallink-linux-sender/src/encoder.rs` — GStreamer H.264 encode pipeline
//! - [ ] Full capture → encode → send loop (in-progress, wired below)
//! - [ ] egui settings UI (receiver IP, PIN, display count, resolution, fps)

mod encoder;

use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use duallink_capture_linux::{CaptureConfig, ScreenCapturer};
use duallink_core::StreamConfig;
use duallink_transport_client::{SignalingClient, VideoSender};
use encoder::GstEncoder;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .init();

    info!("DualLink Linux Sender v{}", env!("CARGO_PKG_VERSION"));

    // ── Configuration from environment variables (egui UI in a later phase) ──
    let host          = env::var("DUALLINK_HOST").unwrap_or_else(|_| "192.168.1.100".to_owned());
    let pin           = env::var("DUALLINK_PIN").unwrap_or_else(|_| "000000".to_owned());
    let display_index = env::var("DUALLINK_DISPLAY")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(0);
    let width  = env::var("DUALLINK_WIDTH").ok().and_then(|v| v.parse().ok()).unwrap_or(1920u32);
    let height = env::var("DUALLINK_HEIGHT").ok().and_then(|v| v.parse().ok()).unwrap_or(1080u32);
    let fps    = env::var("DUALLINK_FPS").ok().and_then(|v| v.parse().ok()).unwrap_or(60u32);
    let kbps   = env::var("DUALLINK_KBPS").ok().and_then(|v| v.parse().ok()).unwrap_or(8000u32);

    info!(
        "Connecting to {} display_index={} {}x{} @{}fps bitrate={}kbps",
        host, display_index, width, height, fps, kbps
    );

    // ── 1. Connect signaling ──────────────────────────────────────────────────
    let mut sig = SignalingClient::connect(&host, display_index)
        .await
        .with_context(|| format!("Signaling connect to {}", host))?;

    let session_id = format!("linux-sender-{}", ts_ms());
    let config = StreamConfig {
        width,
        height,
        fps,
        ..Default::default()
    };

    let ack = sig
        .send_hello(&session_id, hostname(), config.clone(), &pin)
        .await
        .context("send_hello")?;

    if !ack.accepted {
        anyhow::bail!("Session rejected by receiver: {:?}", ack.reason);
    }
    info!("Session accepted (id={})", session_id);

    let (mut sig_writer, mut input_rx) = sig.start_recv_loop();

    // ── 2. Connect UDP video sender ───────────────────────────────────────────
    let video = VideoSender::connect(&host, display_index)
        .await
        .with_context(|| format!("UDP connect to {}", host))?;

    // ── 3. Open screen capture ────────────────────────────────────────────────
    let capture_cfg = CaptureConfig { display_index, width, height, fps };
    let mut capturer = ScreenCapturer::open(capture_cfg)
        .await
        .context("PipeWire capture open")?;

    // ── 4. Create GStreamer H.264 encoder ─────────────────────────────────────
    gstreamer::init().context("GStreamer init")?;
    let mut encoder = GstEncoder::new(width, height, fps, kbps)
        .context("Creating GStreamer encoder")?;

    info!("Capture + encode pipeline started.  Streaming to {} ...", host);

    // ── 5. Keepalive timer ────────────────────────────────────────────────────
    let mut keepalive_ticker = tokio::time::interval(tokio::time::Duration::from_secs(1));

    // ── 6. Main loop: capture → encode → send ────────────────────────────────
    loop {
        tokio::select! {
            // Capture a raw frame
            maybe_raw = capturer.next_frame() => {
                let Some(raw) = maybe_raw else {
                    info!("Capture ended (EOS)");
                    break;
                };
                // Push into GStreamer encoder (synchronous, fast)
                if let Err(e) = encoder.push_frame(raw) {
                    tracing::warn!("push_frame error: {:#}", e);
                }
            }

            // Pull an encoded frame and send it
            maybe_enc = encoder.next_encoded() => {
                let Some(enc) = maybe_enc else {
                    info!("Encoder pipeline ended");
                    break;
                };
                if let Err(e) = video.send_frame(&enc).await {
                    tracing::warn!("send_frame error: {:#}", e);
                }
            }

            // Keepalive heartbeat (1 Hz)
            _ = keepalive_ticker.tick() => {
                if let Err(e) = sig_writer.send_keepalive(ts_ms()).await {
                    tracing::warn!("keepalive error: {:#}", e);
                    break;
                }
            }

            // Input events from receiver (injected into local X11 / Wayland)
            maybe_event = input_rx.recv() => {
                match maybe_event {
                    Some(event) => {
                        tracing::debug!("Input event: {:?}", event);
                        // TODO Phase 5D: inject into Wayland/X11 via uinput or libinput
                    }
                    None => {
                        info!("Signaling recv loop closed");
                        break;
                    }
                }
            }
        }
    }

    // ── 7. Graceful shutdown ──────────────────────────────────────────────────
    encoder.send_eos();
    let _ = sig_writer.send_stop(&session_id).await;
    info!("DualLink Linux Sender stopped.");
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn ts_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn hostname() -> &'static str {
    // Leak is fine — called once at startup
    Box::leak(
        hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "linux-sender".to_owned())
            .into_boxed_str(),
    )
}

