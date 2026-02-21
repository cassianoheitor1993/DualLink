//! DualLink Linux Sender — Phase 5B skeleton.
//!
//! Turns a Linux machine into a DualLink **sender**, mirroring or extending its
//! screen to a DualLink **receiver** (another Linux machine, Windows, or macOS).
//!
//! # Architecture (planned)
//!
//! ```text
//! Linux (this app)                     Receiver (Linux / Windows / macOS)
//! ─────────────────────────────── ──   ─────────────────────────────────────
//! PipeWire / XShm capture               display decoder (GStreamer / VT)
//!   │                                     │
//!   ▼                                     │
//! GStreamer encode                         │
//!   vaapih264enc / nvh264enc              │
//!   │                                     │
//!   ▼                                     │
//! duallink-transport (UDP:7878) ────────► UDP receiver
//! duallink-signaling  (TCP:7879) ───────► TLS signaling server
//! ```
//!
//! # Phase 5B status
//!
//! - [x] Workspace scaffold + dependency declarations
//! - [x] `duallink-capture-linux` stub (PipeWire portal API surface)
//! - [ ] PipeWire capture implementation (`ashpd::desktop::ScreenCast`)
//! - [ ] GStreamer H.264 encode pipeline (`appsrc → vaapih264enc → appsink`)
//! - [ ] Signaling client (TCP TLS `hello` handshake as sender role)
//! - [ ] UDP video sender (DLNK-framed packets)
//! - [ ] egui settings UI (receiver IP, PIN, display count, resolution, fps)

use anyhow::Result;
use tracing::{error, info};
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

    // TODO Phase 5B: launch egui settings window
    // TODO Phase 5B: start capture + encode + send pipeline

    info!("Linux sender skeleton — capture pipeline not yet implemented.");
    info!("See linux-sender/README.md for development roadmap.");

    // Placeholder: exit cleanly so CI passes
    Ok(())
}
