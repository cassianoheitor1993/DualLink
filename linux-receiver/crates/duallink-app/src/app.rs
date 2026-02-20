use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use duallink_core::EncodedFrame;
use duallink_decoder::DecoderFactory;
use duallink_transport::{DualLinkReceiver, SignalingEvent};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Main receiver loop — Sprint 1.4
///
/// 1. Bind UDP:7878 (video) + TCP:7879 (signaling)
/// 2. Wait for hello handshake → get StreamConfig
/// 3. Initialise GStreamer decoder (vaapih264dec → nvh264dec → avdec_h264)
/// 4. Receive → decode → (renderer Sprint 2)
pub async fn run() -> Result<()> {
    info!("Binding transport (UDP:7878 video, TCP:7879 signaling)...");

    let (_recv, mut frame_rx, mut event_rx) = DualLinkReceiver::start().await?;

    info!("Waiting for macOS DualLink client to connect...");
    info!("Enter the IP of this machine in the DualLink mac app and press Start Mirroring.");

    // ── Wait for hello to get the session config ───────────────────────────
    let config = loop {
        match event_rx.recv().await {
            Some(SignalingEvent::SessionStarted { session_id, device_name, config, client_addr }) => {
                info!(
                    "Session started: id={} from='{}' addr={} config={:?}",
                    session_id, device_name, client_addr, config
                );
                break config;
            }
            Some(SignalingEvent::ClientDisconnected) => {
                warn!("Client disconnected before hello — waiting again");
            }
            Some(other) => {
                info!("Signaling event (pre-session): {:?}", other);
            }
            None => {
                anyhow::bail!("Signaling channel closed before session started");
            }
        }
    };

    // ── Initialise decoder ─────────────────────────────────────────────────
    let width  = config.resolution.width;
    let height = config.resolution.height;

    let decoder = tokio::task::spawn_blocking(move || {
        DecoderFactory::best_available(width, height)
    }).await?
    .map_err(|e| anyhow::anyhow!("Decoder init failed: {}", e))?;

    let hw = decoder.is_hardware_accelerated();
    let elem = decoder.element_name().to_string();
    info!("Decoder ready: {} hw={}", elem, hw);

    // ── Dedicated decode thread (GStreamer is blocking) ─────────────────────
    let (decode_tx, mut decode_rx) = mpsc::channel::<EncodedFrame>(64);
    let decoded_count = Arc::new(AtomicU64::new(0));
    let decode_errors = Arc::new(AtomicU64::new(0));
    let dc = Arc::clone(&decoded_count);
    let de = Arc::clone(&decode_errors);

    let decode_handle = tokio::task::spawn_blocking(move || {
        while let Some(frame) = decode_rx.blocking_recv() {
            let sz = frame.data.len();
            let kf = frame.is_keyframe;
            match decoder.decode_frame(frame) {
                Ok(_decoded) => {
                    let n = dc.fetch_add(1, Ordering::Relaxed) + 1;
                    if n == 1 {
                        info!("First frame decoded successfully!");
                    }
                    if n % 300 == 0 {
                        info!("Decoded {} frames so far", n);
                    }
                }
                Err(e) => {
                    let errs = de.fetch_add(1, Ordering::Relaxed) + 1;
                    if errs <= 10 || errs % 100 == 0 {
                        warn!("Decode error #{} ({} bytes, keyframe={}): {}", errs, sz, kf, e);
                    }
                }
            }
        }
        info!("Decode thread exiting");
    });

    // ── Main receive → decode loop ─────────────────────────────────────────
    info!("Streaming — receiving frames...");
    let mut frames_received: u64 = 0;
    loop {
        tokio::select! {
            // Incoming encoded frame
            Some(frame) = frame_rx.recv() => {
                frames_received += 1;
                if frames_received <= 5 {
                    debug!("Frame #{}: {} bytes keyframe={}", frames_received, frame.data.len(), frame.is_keyframe);
                }
                if frames_received % 300 == 0 {
                    let dec = decoded_count.load(Ordering::Relaxed);
                    let err = decode_errors.load(Ordering::Relaxed);
                    info!("Stats: received={} decoded={} errors={}", frames_received, dec, err);
                }
                // Send to blocking decode thread
                if decode_tx.send(frame).await.is_err() {
                    warn!("Decode thread gone — stopping");
                    break;
                }
            }

            // Signaling events mid-session
            Some(event) = event_rx.recv() => {
                match event {
                    SignalingEvent::SessionStopped { session_id } => {
                        info!("Session {} stopped by sender — exiting", session_id);
                        break;
                    }
                    SignalingEvent::ClientDisconnected => {
                        warn!("Sender disconnected — exiting");
                        break;
                    }
                    SignalingEvent::ConfigUpdated { config } => {
                        info!("Config update received: {:?}", config);
                        // TODO: Sprint 2 — reinitialise decoder/renderer on config change
                    }
                    _ => {}
                }
            }

            else => break,
        }
    }

    // Cleanup: drop sender so decode thread exits
    drop(decode_tx);
    let _ = decode_handle.await;

    let total_dec = decoded_count.load(Ordering::Relaxed);
    let total_err = decode_errors.load(Ordering::Relaxed);
    info!("Receiver exited. received={} decoded={} errors={}", frames_received, total_dec, total_err);
    Ok(())
}
