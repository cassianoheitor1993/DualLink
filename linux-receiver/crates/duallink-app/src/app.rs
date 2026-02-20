use anyhow::Result;
use duallink_decoder::DecoderFactory;
use duallink_transport::{DualLinkReceiver, SignalingEvent};
use tracing::{info, warn};

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

    info!("Decoder ready: {} hw={}", decoder.element_name(), decoder.is_hardware_accelerated());

    // ── Main receive → decode loop ─────────────────────────────────────────
    info!("Streaming — receiving frames...");
    loop {
        tokio::select! {
            // Incoming encoded frame
            Some(frame) = frame_rx.recv() => {
                match decoder.decode_frame(frame) {
                    Ok(decoded) => {
                        // TODO: Sprint 2 — pass decoded frame to renderer
                        let _ = decoded;
                    }
                    Err(e) => {
                        warn!("Decode error: {}", e);
                    }
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

    info!("Receiver exited cleanly.");
    Ok(())
}
