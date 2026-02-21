//! DualLink Windows Sender — Phase 5B skeleton.
//!
//! Turns a Windows machine into a DualLink **sender**, mirroring or extending its
//! screen to a DualLink receiver (Linux, macOS, or another Windows machine).
//!
//! # Architecture (planned)
//!
//! ```text
//! Windows (this app)                   Receiver (any DualLink receiver)
//! ─────────────────────────────── ─── ─────────────────────────────────────
//! Windows.Graphics.Capture (WGC)      display decoder (GStreamer / VT)
//!   │                                   │
//!   ▼                                   │
//! GStreamer encode                       │
//!   mfh264enc / nvh264enc              │
//!   │                                   │
//!   ▼                                   │
//! duallink-transport UDP:7878 ────────► UDP receiver
//! TLS signaling      TCP:7879 ────────► TLS signaling server
//! ```
//!
//! # Virtual display
//!
//! To extend (not mirror), a virtual display driver is required.
//! Options:
//! - **IddCx** (Indirect Display driver): zero-cost, requires a driver package
//! - **parsec-vdd**: open-source IddCx-based virtual display from Parsec
//!
//! # Phase 5B status
//!
//! - [x] Workspace scaffold + dependency declarations
//! - [x] `duallink-capture-windows` stub (WGC API surface)
//! - [ ] WGC capture implementation (GraphicsCaptureSession + FramePool)
//! - [ ] GStreamer H.264 encode pipeline (appsrc → mfh264enc / nvh264enc)
//! - [ ] Signaling client (TCP TLS `hello` handshake)
//! - [ ] UDP video sender (DLNK-framed packets)
//! - [ ] egui settings UI
//! - [ ] Virtual display (IddCx / parsec-vdd integration)

use anyhow::Result;
use tracing::{info};
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

    // TODO Phase 5B: launch egui settings window
    // TODO Phase 5B: start WGC capture + GStreamer encode + UDP send pipeline

    info!("Windows sender skeleton — capture pipeline not yet implemented.");
    info!("See windows-sender/README.md for development roadmap.");

    Ok(())
}
