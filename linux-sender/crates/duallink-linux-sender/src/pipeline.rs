//! `SenderPipeline` — one display's full capture → encode → UDP-send loop.
//!
//! Each display stream is an independent `SenderPipeline`:
//!
//! ```text
//! PipeWire portal → GstEncoder → VideoSender (UDP:7878+2n)
//!                                SignalingClient (TLS:7879+2n)
//! ```
//!
//! Create N pipelines for N display streams (multi-monitor sender).
//!
//! # Status channel
//!
//! [`SenderPipeline::spawn`] returns a [`PipelineStatus`] receiver that the
//! egui UI polls with [`try_recv`](tokio::sync::mpsc::Receiver::try_recv) to
//! get live FPS, frame count, and connection state.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use duallink_capture_linux::{CaptureConfig, ScreenCapturer};
use duallink_core::StreamConfig;
use duallink_transport_client::{SignalingClient, VideoSender};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::encoder::GstEncoder;

// ── Configuration ─────────────────────────────────────────────────────────────

/// Configuration for a single display sender pipeline.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    // Network
    pub host:          String,
    pub pairing_pin:   String,
    pub display_index: u8,
    // Video
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

// ── Status ─────────────────────────────────────────────────────────────────────

/// Live status update sent by the pipeline task to the UI.
#[derive(Debug, Clone)]
pub struct PipelineStatus {
    pub display_index: u8,
    pub state:         PipelineState,
    /// Instantaneous frames per second.
    pub fps:           f32,
    /// Total frames sent since pipeline start.
    pub frames_sent:   u64,
}

/// State of a sender pipeline.
#[derive(Debug, Clone, PartialEq)]
pub enum PipelineState {
    Connecting,
    Streaming,
    /// Stopped cleanly.
    Stopped,
    /// Failed with an error message.
    Failed(String),
}

// ── SenderPipeline ────────────────────────────────────────────────────────────

/// Handle to a running sender pipeline task.
pub struct SenderPipeline {
    pub display_index: u8,
    /// Send a `()` to request graceful shutdown.
    pub stop_tx: mpsc::Sender<()>,
    /// Frames sent counter (shared with pipeline task).
    pub frames_sent: Arc<AtomicU64>,
}

impl SenderPipeline {
    /// Spawn a capture → encode → send pipeline for one display.
    ///
    /// Returns the pipeline handle and a status-update channel that the UI
    /// can poll. The pipeline runs until the remote session ends or
    /// `stop_tx.send(())` is called.
    pub fn spawn(
        config: PipelineConfig,
        status_tx: mpsc::Sender<PipelineStatus>,
    ) -> Self {
        let (stop_tx, stop_rx) = mpsc::channel::<()>(1);
        let frames_sent = Arc::new(AtomicU64::new(0));
        let fs = Arc::clone(&frames_sent);
        let display_index = config.display_index;

        tokio::spawn(run_pipeline(config, stop_rx, status_tx, fs));

        Self { display_index, stop_tx, frames_sent }
    }

    /// Request graceful stop (non-blocking).
    pub fn stop(&self) {
        let _ = self.stop_tx.try_send(());
    }

    /// Total frames sent so far.
    pub fn frames_sent(&self) -> u64 {
        self.frames_sent.load(Ordering::Relaxed)
    }
}

// ── Pipeline task ─────────────────────────────────────────────────────────────

async fn run_pipeline(
    config: PipelineConfig,
    mut stop_rx: mpsc::Receiver<()>,
    status_tx: mpsc::Sender<PipelineStatus>,
    frames_sent: Arc<AtomicU64>,
) {
    let idx = config.display_index;

    macro_rules! send_status {
        ($state:expr, $fps:expr) => {
            let _ = status_tx.try_send(PipelineStatus {
                display_index: idx,
                state: $state,
                fps: $fps,
                frames_sent: frames_sent.load(Ordering::Relaxed),
            });
        };
    }

    send_status!(PipelineState::Connecting, 0.0);

    // ── 1. Connect signaling ──────────────────────────────────────────────
    let mut sig = match SignalingClient::connect(&config.host, idx).await {
        Ok(s) => s,
        Err(e) => {
            warn!("Display[{}] signaling connect failed: {:#}", idx, e);
            send_status!(PipelineState::Failed(format!("Connect: {e:#}")), 0.0);
            return;
        }
    };

    let session_id = format!("linux-sender-d{}-{}", idx, ts_ms());
    let stream_config = StreamConfig {
        width: config.width,
        height: config.height,
        fps: config.fps,
        ..Default::default()
    };

    let ack = match sig.send_hello(&session_id, &hostname(), stream_config, &config.pairing_pin).await {
        Ok(a) => a,
        Err(e) => {
            warn!("Display[{}] send_hello failed: {:#}", idx, e);
            send_status!(PipelineState::Failed(format!("Handshake: {e:#}")), 0.0);
            return;
        }
    };

    if !ack.accepted {
        let reason = ack.reason.unwrap_or_else(|| "unknown".to_owned());
        warn!("Display[{}] rejected: {}", idx, reason);
        send_status!(PipelineState::Failed(format!("Rejected: {reason}")), 0.0);
        return;
    }
    info!("Display[{}] session accepted (id={})", idx, session_id);

    let (mut sig_writer, mut input_rx) = sig.start_recv_loop();

    // ── 2. Connect UDP video sender ───────────────────────────────────────
    let video = match VideoSender::connect(&config.host, idx).await {
        Ok(v) => v,
        Err(e) => {
            send_status!(PipelineState::Failed(format!("UDP: {e:#}")), 0.0);
            return;
        }
    };

    // ── 3. Open screen capture ────────────────────────────────────────────
    let cap_cfg = CaptureConfig {
        display_index: idx,
        width:  config.width,
        height: config.height,
        fps:    config.fps,
    };
    let mut capturer = match ScreenCapturer::open(cap_cfg).await {
        Ok(c) => c,
        Err(e) => {
            send_status!(PipelineState::Failed(format!("Capture: {e:#}")), 0.0);
            return;
        }
    };

    // ── 4. Create GStreamer encoder ───────────────────────────────────────
    let mut encoder = match GstEncoder::new(config.width, config.height, config.fps, config.bitrate_kbps) {
        Ok(e) => e,
        Err(e) => {
            send_status!(PipelineState::Failed(format!("Encoder: {e:#}")), 0.0);
            return;
        }
    };

    send_status!(PipelineState::Streaming, 0.0);
    info!("Display[{}] streaming to {} ...", idx, config.host);

    // ── 5. Main loop ──────────────────────────────────────────────────────
    let mut keepalive_ticker = tokio::time::interval(Duration::from_secs(1));
    let mut fps_counter = FpsCounter::new();

    loop {
        tokio::select! {
            // Stop requested by UI
            _ = stop_rx.recv() => {
                info!("Display[{}] stop requested", idx);
                break;
            }

            // Capture raw frame
            maybe_raw = capturer.next_frame() => {
                let Some(raw) = maybe_raw else {
                    info!("Display[{}] capture EOS", idx);
                    break;
                };
                if let Err(e) = encoder.push_frame(raw) {
                    warn!("Display[{}] push_frame: {:#}", idx, e);
                }
            }

            // Pull encoded frame and send
            maybe_enc = encoder.next_encoded() => {
                let Some(enc) = maybe_enc else {
                    info!("Display[{}] encoder EOS", idx);
                    break;
                };
                match video.send_frame(&enc).await {
                    Ok(_) => {
                        frames_sent.fetch_add(1, Ordering::Relaxed);
                        fps_counter.tick();
                    }
                    Err(e) => {
                        warn!("Display[{}] send_frame: {:#}", idx, e);
                    }
                }
            }

            // 1-Hz keepalive + FPS status update
            _ = keepalive_ticker.tick() => {
                let fps = fps_counter.fps();
                send_status!(PipelineState::Streaming, fps);
                if let Err(e) = sig_writer.send_keepalive(ts_ms()).await {
                    warn!("Display[{}] keepalive: {:#}", idx, e);
                    break;
                }
            }

            // Input events from receiver
            maybe_ev = input_rx.recv() => {
                match maybe_ev {
                    Some(ev) => {
                        // Forwarded to uinput injector if available — see input_inject.rs
                        #[cfg(target_os = "linux")]
                        crate::input_inject::inject_global(ev).await;
                        #[cfg(not(target_os = "linux"))]
                        tracing::debug!("Display[{}] input event (stub): {:?}", idx, ev);
                    }
                    None => {
                        info!("Display[{}] signaling closed", idx);
                        break;
                    }
                }
            }
        }
    }

    // ── Cleanup ───────────────────────────────────────────────────────────
    encoder.send_eos();
    let _ = sig_writer.send_stop(&session_id).await;
    send_status!(PipelineState::Stopped, 0.0);
    info!("Display[{}] pipeline stopped", idx);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn ts_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn hostname() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "linux-sender".to_owned())
}

/// Rolling ~1 second FPS counter.
struct FpsCounter {
    count:      u32,
    window_start: std::time::Instant,
    last_fps:   f32,
}

impl FpsCounter {
    fn new() -> Self {
        Self { count: 0, window_start: std::time::Instant::now(), last_fps: 0.0 }
    }

    fn tick(&mut self) {
        self.count += 1;
    }

    /// Returns the FPS over the last ~1 second window; resets the counter.
    fn fps(&mut self) -> f32 {
        let elapsed = self.window_start.elapsed().as_secs_f32();
        if elapsed >= 0.5 {
            self.last_fps = self.count as f32 / elapsed;
            self.count = 0;
            self.window_start = std::time::Instant::now();
        }
        self.last_fps
    }
}
