//! DualLink Linux Sender — Phase 5D.
//!
//! Turns a Linux machine into a DualLink **sender**, mirroring or extending its
//! screen to a DualLink **receiver** (another Linux machine, Windows, or macOS).
//!
//! # Modes
//!
//! | Mode | How to start | Key env vars |
//! |------|-------------|-------------|
//! | **GUI** (default) | `./duallink-sender` | — |
//! | **Headless** | `DUALLINK_NO_UI=1 ./duallink-sender` | `DUALLINK_HOST`, `DUALLINK_PIN`, etc. |
//!
//! # Phase 5D status
//!
//! - [x] egui settings UI (host, PIN, resolution, fps, bitrate, display count)
//! - [x] `SenderPipeline` — per-display capture → encode → UDP-send task
//! - [x] `input_inject` — uinput virtual mouse + keyboard (Linux receiver → local desktop)
//! - [x] Multi-display sender (N parallel `SenderPipeline` tasks)
//! - [ ] Absolute mouse positioning (ABS_X/Y tablet device)
//! - [ ] egui FPS graph overlay

mod encoder;
mod input_inject;
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

    info!("DualLink Linux Sender v{}", env!("CARGO_PKG_VERSION"));

    // Initialise uinput injector (no-op if /dev/uinput is not accessible)
    input_inject::init();

    // Initialise GStreamer once before any pipeline is created
    gstreamer::init()?;

    // Build a multi-threaded tokio runtime that runs concurrently with eframe.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let no_ui = std::env::var("DUALLINK_NO_UI").as_deref() == Ok("1");

    if no_ui {
        // ── Headless mode: read config from env vars, run without a window ──
        rt.block_on(headless_main())
    } else {
        // ── GUI mode: launch eframe window, pipelines run in the tokio rt ──
        let handle = rt.handle().clone();
        // Keep the runtime alive for the duration of the GUI.
        let _rt_guard = rt.enter();

        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("DualLink Linux Sender")
                .with_inner_size([480.0, 320.0])
                .with_min_inner_size([380.0, 280.0]),
            ..Default::default()
        };

        eframe::run_native(
            "DualLink Linux Sender",
            native_options,
            Box::new(move |cc| Ok(Box::new(ui::SenderApp::new(handle, cc)))),
        )
        .map_err(|e| anyhow::anyhow!("eframe error: {}", e))
    }
}

// ── Headless pipeline loop (env-var config) ────────────────────────────────────

async fn headless_main() -> Result<()> {
    use std::{env, time::{Duration, SystemTime, UNIX_EPOCH}};
    use pipeline::{PipelineConfig, PipelineState, SenderPipeline};
    use tokio::sync::mpsc;

    let host = env::var("DUALLINK_HOST").unwrap_or_else(|_| "192.168.1.100".to_owned());
    let pin  = env::var("DUALLINK_PIN").unwrap_or_else(|_| "000000".to_owned());
    let display_count: u8 = env::var("DUALLINK_DISPLAY_COUNT")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let width:  u32 = env::var("DUALLINK_WIDTH").ok().and_then(|v| v.parse().ok()).unwrap_or(1920);
    let height: u32 = env::var("DUALLINK_HEIGHT").ok().and_then(|v| v.parse().ok()).unwrap_or(1080);
    let fps:    u32 = env::var("DUALLINK_FPS").ok().and_then(|v| v.parse().ok()).unwrap_or(60);
    let kbps:   u32 = env::var("DUALLINK_KBPS").ok().and_then(|v| v.parse().ok()).unwrap_or(8000);

    info!(
        "Headless mode: {} display(s) → {} — {}×{} @{}fps {}kbps",
        display_count, host, width, height, fps, kbps
    );

    let (status_tx, mut status_rx) = mpsc::channel::<pipeline::PipelineStatus>(64);
    let mut pipelines = Vec::new();

    for i in 0..display_count {
        let cfg = PipelineConfig {
            host: host.clone(),
            pairing_pin: pin.clone(),
            display_index: i,
            width,
            height,
            fps,
            bitrate_kbps: kbps,
        };
        pipelines.push(SenderPipeline::spawn(cfg, status_tx.clone()));
    }

    // Wait until all pipelines finish
    let mut stopped = 0usize;
    while let Some(s) = status_rx.recv().await {
        match &s.state {
            PipelineState::Streaming => {
                info!(
                    "Display[{}] streaming — {:.1} fps {} frames",
                    s.display_index, s.fps, s.frames_sent
                );
            }
            PipelineState::Stopped => {
                info!("Display[{}] stopped", s.display_index);
                stopped += 1;
                if stopped >= display_count as usize {
                    break;
                }
            }
            PipelineState::Failed(e) => {
                tracing::error!("Display[{}] failed: {}", s.display_index, e);
                stopped += 1;
                if stopped >= display_count as usize {
                    break;
                }
            }
            _ => {}
        }
    }

    info!("All pipelines exited. Goodbye.");
    Ok(())
}


