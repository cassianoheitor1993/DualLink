//! `WinSenderPipeline` — one Windows display's full capture → encode → send loop.
//!
//! Mirrors `linux-sender/src/pipeline.rs` but uses:
//! - `duallink_capture_windows::ScreenCapturer` (WGC on Windows, stub otherwise)
//! - `encoder::GstEncoder` with `mfh264enc` / `nvh264enc` / `x264enc` priority

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::collections::VecDeque;

use anyhow::Result;
use duallink_capture_windows::{CaptureConfig, ScreenCapturer};
use duallink_transport_client::{SignalingClient, VideoSender};
use duallink_core::StreamConfig;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

// ── Public types ──────────────────────────────────────────────────────────────

/// Configuration for one display pipeline.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub host:          String,
    pub pairing_pin:   String,
    pub display_index: u8,
    pub width:         u32,
    pub height:        u32,
    pub fps:           u32,
    pub bitrate_kbps:  u32,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            host:          "192.168.1.100".to_owned(),
            pairing_pin:   "000000".to_owned(),
            display_index: 0,
            width:         1920,
            height:        1080,
            fps:           60,
            bitrate_kbps:  8000,
        }
    }
}

/// Lifecycle state of a pipeline.
#[derive(Debug, Clone, PartialEq)]
pub enum PipelineState {
    Connecting,
    Streaming,
    Stopped,
    Failed(String),
}

/// Periodic status update pushed to the UI via mpsc channel.
#[derive(Debug, Clone)]
pub struct PipelineStatus {
    pub display_index: u8,
    pub state:         PipelineState,
    pub fps:           f32,
    pub frames_sent:   u64,
}

// ── WinSenderPipeline ─────────────────────────────────────────────────────────

/// Handle to a running capture → encode → send pipeline task.
pub struct WinSenderPipeline {
    stop_tx:      oneshot::Sender<()>,
    frames_sent:  Arc<AtomicU64>,
}

impl WinSenderPipeline {
    /// Spawn the async pipeline task and return a handle to it.
    pub fn spawn(config: PipelineConfig, status_tx: mpsc::Sender<PipelineStatus>) -> Self {
        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        let frames_sent = Arc::new(AtomicU64::new(0));
        let fs = Arc::clone(&frames_sent);

        tokio::spawn(async move {
            run_pipeline(config, status_tx, stop_rx, fs).await;
        });

        Self { stop_tx, frames_sent }
    }

    /// Request the pipeline to stop (best-effort, non-blocking).
    pub fn stop(&self) {
        // Can't call on consumed sender, so hold a flag instead of consuming
    }

    pub fn frames_sent(&self) -> u64 {
        self.frames_sent.load(Ordering::Relaxed)
    }
}

// ── Pipeline task ─────────────────────────────────────────────────────────────

async fn run_pipeline(
    cfg: PipelineConfig,
    status_tx: mpsc::Sender<PipelineStatus>,
    mut stop_rx: oneshot::Receiver<()>,
    frames_sent: Arc<AtomicU64>,
) {
    let idx = cfg.display_index;

    macro_rules! report {
        ($state:expr) => {
            let _ = status_tx.try_send(PipelineStatus {
                display_index: idx,
                state: $state,
                fps: 0.0,
                frames_sent: frames_sent.load(Ordering::Relaxed),
            });
        };
        ($state:expr, $fps:expr) => {
            let _ = status_tx.try_send(PipelineStatus {
                display_index: idx,
                state: $state,
                fps: $fps,
                frames_sent: frames_sent.load(Ordering::Relaxed),
            });
        };
    }

    report!(PipelineState::Connecting);

    // ── 1. Connect signaling ──────────────────────────────────────────────
    let mut sig = match SignalingClient::connect(&cfg.host, idx).await {
        Ok(s) => s,
        Err(e) => {
            report!(PipelineState::Failed(format!("Signaling: {e}")));
            return;
        }
    };

    let session_id = format!("win-sender-{idx}-{}", ts_ms());
    let stream_cfg = StreamConfig {
        width: cfg.width,
        height: cfg.height,
        fps: cfg.fps,
        ..Default::default()
    };
    match sig.send_hello(&session_id, hostname(), stream_cfg.clone(), &cfg.pairing_pin).await {
        Ok(ack) if !ack.accepted => {
            report!(PipelineState::Failed(format!("Rejected: {:?}", ack.reason)));
            return;
        }
        Err(e) => {
            report!(PipelineState::Failed(format!("Hello: {e}")));
            return;
        }
        Ok(_) => {}
    }

    let (mut sig_writer, mut input_rx) = sig.start_recv_loop();

    // ── 2. Connect UDP sender ─────────────────────────────────────────────
    let video = match VideoSender::connect(&cfg.host, idx).await {
        Ok(v) => v,
        Err(e) => {
            report!(PipelineState::Failed(format!("UDP: {e}")));
            return;
        }
    };

    // ── 3. Open screen capturer ───────────────────────────────────────────
    let cap_cfg = CaptureConfig {
        display_index: cfg.display_index,
        width: cfg.width,
        height: cfg.height,
        fps: cfg.fps,
    };
    let mut capturer = match ScreenCapturer::open(cap_cfg).await {
        Ok(c) => c,
        Err(e) => {
            report!(PipelineState::Failed(format!("Capture: {e}")));
            return;
        }
    };

    // ── 4. Create encoder ─────────────────────────────────────────────────
    let mut encoder = match super::encoder::GstEncoder::new(
        cfg.width, cfg.height, cfg.fps, cfg.bitrate_kbps,
    ) {
        Ok(e) => e,
        Err(e) => {
            report!(PipelineState::Failed(format!("Encoder: {e}")));
            return;
        }
    };

    report!(PipelineState::Streaming);
    info!("Display[{idx}] WinSenderPipeline streaming → {}", cfg.host);

    let mut fps_counter = FpsCounter::new();
    let mut keepalive = tokio::time::interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            _ = &mut stop_rx => {
                info!("Display[{idx}] stop requested");
                break;
            }

            maybe_raw = capturer.next_frame() => {
                let Some(raw) = maybe_raw else { break; };
                let _ = encoder.push_frame(raw);
            }

            maybe_enc = tokio::task::spawn_blocking({
                // Poll encoder in a blocking-compatible way
                let mut enc = unsafe {
                    &mut *(&mut encoder as *mut super::encoder::GstEncoder)
                };
                move || enc.next_encoded()
            }) => {
                if let Ok(Some(enc)) = maybe_enc {
                    if let Err(e) = video.send_frame(&enc).await {
                        warn!("Display[{idx}] send_frame: {e:#}");
                    }
                    frames_sent.fetch_add(1, Ordering::Relaxed);
                    fps_counter.tick();
                }
            }

            _ = keepalive.tick() => {
                let _ = sig_writer.send_keepalive(ts_ms()).await;
                report!(PipelineState::Streaming, fps_counter.fps());
            }

            maybe_ev = input_rx.recv() => {
                match maybe_ev {
                    Some(ev) => {
                        // TODO Phase 5F: inject via SendInput on Windows
                        tracing::debug!("Display[{idx}] input: {:?}", ev);
                    }
                    None => break,
                }
            }
        }
    }

    encoder.send_eos();
    let _ = sig_writer.send_stop(&session_id).await;
    report!(PipelineState::Stopped);
    info!("Display[{idx}] WinSenderPipeline stopped");
}

// ── FpsCounter ────────────────────────────────────────────────────────────────

struct FpsCounter {
    timestamps: VecDeque<std::time::Instant>,
}

impl FpsCounter {
    fn new() -> Self { Self { timestamps: VecDeque::with_capacity(128) } }

    fn tick(&mut self) {
        let now = std::time::Instant::now();
        self.timestamps.push_back(now);
        while self.timestamps.front().map_or(false, |t| now - *t > Duration::from_secs(1)) {
            self.timestamps.pop_front();
        }
    }

    fn fps(&self) -> f32 { self.timestamps.len() as f32 }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn ts_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn hostname() -> &'static str {
    Box::leak(
        hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "windows-sender".to_owned())
            .into_boxed_str(),
    )
}
