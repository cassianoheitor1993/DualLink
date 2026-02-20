use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use duallink_core::EncodedFrame;
use duallink_decoder::DecoderFactory;
use duallink_transport::{DualLinkReceiver, SignalingEvent};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Main receiver loop — Sprint 2.3 (display + input forwarding)
///
/// 1. Bind UDP:7878 (video) + TCP:7879 (signaling)
/// 2. Wait for hello handshake → get StreamConfig
/// 3. Initialise GStreamer display decoder (vaapih264dec → autovideosink)
/// 4. Receive → decode → display (single pipeline)
/// 5. Capture mouse/keyboard from GStreamer window → forward to Mac via TCP
pub async fn run() -> Result<()> {
    // ── Detect USB Ethernet (Phase 3) ──────────────────────────────────────
    if let Some(usb_info) = duallink_core::detect_usb_ethernet() {
        info!("🔌 USB Ethernet detected: {} → {} (peer: {})",
              usb_info.interface_name, usb_info.local_ip, usb_info.peer_ip);
        info!("   USB-C transport will be preferred (lower latency)");
    } else {
        info!("No USB Ethernet detected — using Wi-Fi transport");
    }

    info!("Binding transport (UDP:7878 video, TCP:7879 signaling)...");

    let (_recv, mut frame_rx, mut event_rx, input_sender) = DualLinkReceiver::start().await?;

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

    // ── Initialise display decoder ─────────────────────────────────────────
    let width  = config.resolution.width;
    let height = config.resolution.height;

    let display_decoder = tokio::task::spawn_blocking(move || {
        DecoderFactory::best_available_with_display(width, height)
    }).await?
    .map_err(|e| anyhow::anyhow!("Display decoder init failed: {}", e))?;

    let hw = display_decoder.is_hardware_accelerated();
    let elem = display_decoder.element_name().to_string();
    info!("Display decoder ready: {} hw={} — video window should appear", elem, hw);

    // ── Dedicated decode+display+input thread (GStreamer is blocking) ──────
    let (decode_tx, mut decode_rx) = mpsc::channel::<EncodedFrame>(64);
    let push_errors = Arc::new(AtomicU64::new(0));
    let pe = Arc::clone(&push_errors);

    let decode_handle = tokio::task::spawn_blocking(move || {
        while let Some(frame) = decode_rx.blocking_recv() {
            let sz = frame.data.len();
            let kf = frame.is_keyframe;
            match display_decoder.push_frame(frame) {
                Ok(()) => {
                    let n = display_decoder.frames_pushed();
                    if n == 1 {
                        info!("First frame decoded and displayed!");
                    }
                    if n % 300 == 0 {
                        info!("Displayed {} frames so far", n);
                    }
                }
                Err(e) => {
                    let errs = pe.fetch_add(1, Ordering::Relaxed) + 1;
                    if errs <= 10 || errs % 100 == 0 {
                        warn!("Display push error #{} ({} bytes, keyframe={}): {}", errs, sz, kf, e);
                    }
                }
            }

            // Poll and forward input events from the GStreamer window
            for event in display_decoder.poll_input_events() {
                if let Err(_) = input_sender.try_send(event) {
                    // Channel full or closed — drop event silently
                }
            }
        }
        info!("Decode+display thread exiting");
    });

    // ── Main receive → display loop ────────────────────────────────────────
    info!("Streaming — receiving and displaying frames...");
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
                    let err = push_errors.load(Ordering::Relaxed);
                    info!("Stats: received={} errors={}", frames_received, err);
                }
                // Send to blocking decode+display thread
                if decode_tx.send(frame).await.is_err() {
                    warn!("Decode+display thread gone — stopping");
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

    let total_err = push_errors.load(Ordering::Relaxed);
    info!("Receiver exited. received={} errors={}", frames_received, total_err);
    Ok(())
}
