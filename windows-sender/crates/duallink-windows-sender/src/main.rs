//! DualLink Windows Sender — Phase 5C.
//!
//! Turns a Windows machine into a DualLink **sender**, mirroring or extending its
//! screen to a DualLink receiver (Linux, macOS, or another Windows machine).
//!
//! # Architecture
//!
//! ```text
//! Windows (this app)                   Receiver (any DualLink receiver)
//! ─────────────────────────────── ─── ─────────────────────────────────────
//! Windows.Graphics.Capture (WGC)      display decoder (GStreamer / VT)
//!   │
//!   ▼
//! GStreamer encode
//!   mfh264enc / nvh264enc
//!   │
//!   ▼
//! VideoSender (UDP:7878+2n) ──────────► UDP receiver
//! SignalingClient (TLS:7879+2n) ──────► TLS signaling server
//! ```
//!
//! # Virtual display
//!
//! To extend (not mirror), a virtual display driver is required.
//! Options:
//! - **IddCx** (Indirect Display driver): zero-cost, requires a driver package
//! - **parsec-vdd**: open-source IddCx-based virtual display from Parsec
//!
//! # Phase 5C status
//!
//! - [x] Workspace scaffold + dependency declarations
//! - [x] `duallink-capture-windows` stub (WGC API surface)
//! - [x] `duallink-transport-client` — TLS signaling client + UDP DLNK sender
//! - [ ] WGC capture implementation (GraphicsCaptureSession + FramePool)
//! - [ ] GStreamer H.264 encode pipeline (appsrc → mfh264enc / nvh264enc)
//! - [ ] egui settings UI
//! - [ ] Virtual display (IddCx / parsec-vdd integration)

use anyhow::Result;
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

    info!("DualLink Windows Sender v{}", env!("CARGO_PKG_VERSION"));

    // TODO Phase 5C: implement WGC capture via duallink-capture-windows
    // TODO Phase 5C: build GStreamer H.264 encode pipeline
    // TODO Phase 5C: wire duallink_transport_client::{SignalingClient, VideoSender}
    // TODO Phase 5C: launch egui settings window

    info!("Windows sender skeleton — capture pipeline not yet implemented.");
    info!("Transport client (signaling + UDP sender) is ready in duallink-transport-client.");
    info!("See windows-sender/README.md for development roadmap.");

    Ok(())
}
