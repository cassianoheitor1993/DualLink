//! DualLink Windows Sender — Phase 5E.
//!
//! Turns a Windows machine into a DualLink **sender**, mirroring or extending
//! its screen to any DualLink receiver (Linux, macOS, or another Windows machine).
//!
//! # Modes
//!
//! | Mode | How | Env vars |
//! |------|-----|---------|
//! | **GUI** (default) | `.\duallink-sender.exe` | — |
//! | **Headless** | `DUALLINK_NO_UI=1 .\duallink-sender.exe` | `DUALLINK_HOST`, `DUALLINK_PIN`, etc. |
//!
//! # Phase 5E status
//!
//! - [x] WGC capture (Windows.Graphics.Capture via `windows` crate)
//! - [x] GStreamer H.264 encode (mfh264enc / nvh264enc / x264enc priority)
//! - [x] egui settings UI with mDNS receiver discovery
//! - [x] `WinSenderPipeline` — per-display capture → encode → UDP-send task
//! - [ ] SendInput input injection (Phase 5F)
//! - [ ] Virtual display via IddCx / parsec-vdd (Phase 5F)

mod encoder;
mod pipeline;
mod ui;

use anyhow::Result;
use tracing::info;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .init();

    info!("DualLink Windows Sender v{}", env!("CARGO_PKG_VERSION"));

    // Initialise GStreamer once before any pipeline is created
    gstreamer::init()?;

    // Build a multi-threaded tokio runtime
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let no_ui = std::env::var("DUALLINK_NO_UI").as_deref() == Ok("1");

    if no_ui {
        rt.block_on(headless_main())
    } else {
        let handle = rt.handle().clone();
        let _rt_guard = rt.enter();

        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("DualLink Windows Sender")
                .with_inner_size([520.0, 360.0])
                .with_min_inner_size([400.0, 300.0]),
            ..Default::default()
        };

        eframe::run_native(
            "DualLink Windows Sender",
            native_options,
            Box::new(move |cc| Ok(Box::new(ui::WinSenderApp::new(handle, cc)))),
        )
        .map_err(|e| anyhow::anyhow!("eframe error: {}", e))
    }
}

// ── Headless pipeline loop ─────────────────────────────────────────────────────

async fn headless_main() -> Result<()> {
    use std::env;
    use pipeline::{PipelineConfig, PipelineState, WinSenderPipeline};
    use tokio::sync::mpsc;

    let host  = env::var("DUALLINK_HOST").unwrap_or_else(|_| "192.168.1.100".to_owned());
    let pin   = env::var("DUALLINK_PIN").unwrap_or_else(|_| "000000".to_owned());
    let n: u8 = env::var("DUALLINK_DISPLAY_COUNT").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let w: u32 = env::var("DUALLINK_WIDTH").ok().and_then(|v| v.parse().ok()).unwrap_or(1920);
    let h: u32 = env::var("DUALLINK_HEIGHT").ok().and_then(|v| v.parse().ok()).unwrap_or(1080);
    let fps: u32 = env::var("DUALLINK_FPS").ok().and_then(|v| v.parse().ok()).unwrap_or(60);
    let kbps: u32 = env::var("DUALLINK_KBPS").ok().and_then(|v| v.parse().ok()).unwrap_or(8000);

    info!("Headless: {} display(s) → {} — {}×{} @{}fps {}kbps", n, host, w, h, fps, kbps);

    let (status_tx, mut status_rx) = mpsc::channel::<pipeline::PipelineStatus>(64);
    let mut pipelines = Vec::new();

    for i in 0..n {
        let cfg = PipelineConfig { host: host.clone(), pairing_pin: pin.clone(),
            display_index: i, width: w, height: h, fps, bitrate_kbps: kbps };
        pipelines.push(WinSenderPipeline::spawn(cfg, status_tx.clone()));
    }

    let mut stopped = 0usize;
    while let Some(s) = status_rx.recv().await {
        match &s.state {
            PipelineState::Streaming => info!("Display[{}] streaming {:.1}fps", s.display_index, s.fps),
            PipelineState::Stopped | PipelineState::Failed(_) => {
                stopped += 1;
                if stopped >= n as usize { break; }
            }
            _ => {}
        }
    }

    info!("All pipelines exited.");
    Ok(())
}
