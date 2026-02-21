use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::Result;
use duallink_core::{EncodedFrame, StreamConfig, detect_usb_ethernet};
use duallink_decoder::DecoderFactory;
use duallink_discovery::{DualLinkAdvertiser, detect_local_ip};
use duallink_transport::{DualLinkReceiver, DisplayChannels, InputSender, SignalingEvent, SIGNALING_PORT};
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Main receiver loop — Phase 5B (multi-display + cross-platform receiver)
///
/// # Display count
/// Set `DUALLINK_DISPLAY_COUNT` to control how many virtual displays to expose
/// (default 1, max 8).  Each display binds an independent UDP/TCP port pair:
///   - Display 0: UDP 7878 / TCP 7879
///   - Display 1: UDP 7880 / TCP 7881
///   - Display n: UDP 7878+2n / TCP 7879+2n
///
/// # Flow (per display)
/// 1. Bind UDP + TCP ports via `DualLinkReceiver::start_all`
/// 2. Wait for `hello` handshake → obtain `StreamConfig`
/// 3. Initialise the best available GStreamer display decoder
/// 4. Receive → decode → display loop
/// 5. Forward captured input events back to the Mac sender
pub async fn run() -> Result<()> {
    // ── Read display count from environment ────────────────────────────────
    let display_count: u8 = std::env::var("DUALLINK_DISPLAY_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
        .max(1)
        .min(8);

    // ── Detect USB Ethernet for low-latency transport ──────────────────────
    if let Some(usb) = detect_usb_ethernet() {
        info!(
            "USB Ethernet detected: {} → {} (peer: {})",
            usb.interface_name, usb.local_ip, usb.peer_ip
        );
        info!("Mac can connect via USB at {} for ~1ms latency", usb.local_ip);
    } else {
        info!("No USB Ethernet detected — using Wi-Fi transport");
    }

    info!(
        "Starting {} display stream(s) — binding transport ports...",
        display_count
    );

    let (_recv, channels, input_sender, startup) =
        DualLinkReceiver::start_all(display_count).await?;

    // ── Advertise via mDNS so senders can auto-discover this receiver ──────
    let local_ip = detect_local_ip();
    let _advertiser = DualLinkAdvertiser::register(
        "DualLink Receiver",
        display_count,
        SIGNALING_PORT,
        local_ip,
        &startup.tls_fingerprint,
    )
    .map_err(|e| warn!("mDNS advertising unavailable: {e}"))
    .ok();

    info!(
        "Waiting for DualLink client to connect on {} port pair(s).",
        channels.len()
    );
    info!("Pairing PIN: {}  |  TLS fingerprint: {}…", startup.pairing_pin, &startup.tls_fingerprint[..16.min(startup.tls_fingerprint.len())]);
    info!("Enter {}  in the DualLink sender app.", local_ip);

    // ── Spawn one task per display ─────────────────────────────────────────
    let mut handles = Vec::with_capacity(channels.len());
    for ch in channels {
        let is = input_sender.clone();
        let handle = tokio::spawn(async move {
            let idx = ch.display_index;
            if let Err(e) = run_display(ch, is).await {
                warn!("Display[{idx}] exited with error: {:#}", e);
            }
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.await;
    }

    info!("All display streams exited.");
    Ok(())
}

// ── Per-display loop ───────────────────────────────────────────────────────────

/// Runs a single display's receive → decode → display loop.
///
/// After each session ends (sender disconnects or stops) the function loops
/// back to wait for the **next** connection on the same bound ports, so the
/// receiver never needs a restart between sessions.
async fn run_display(
    ch: DisplayChannels,
    input_sender: InputSender,
) -> Result<()> {
    let DisplayChannels { display_index, mut frame_rx, mut event_rx } = ch;

    let mut session_count: u32 = 0;

    // Pending config forwarded from a mid-session ConfigUpdated event (hot-reload).
    // When set, the next 'reconnect iteration uses it instead of waiting for a new hello.
    let mut pending_config: Option<StreamConfig> = None;

    // ── Reconnect loop: one iteration per sender session ──────────────────
    'reconnect: loop {
        if session_count == 0 {
            info!(
                "Display[{}] Waiting for sender to connect...",
                display_index
            );
        } else if pending_config.is_some() {
            // Hot-reload path: session is still alive, just reinitialise the decoder.
            info!(
                "Display[{}] Session {} — hot-reloading decoder (resolution change)",
                display_index, session_count
            );
        } else {
            info!(
                "Display[{}] Session {} ended — ready for next connection",
                display_index, session_count
            );
            // Brief pause so the OS has time to clean up the prior TCP conn
            tokio::time::sleep(Duration::from_millis(300)).await;
        }

        // ── Obtain StreamConfig: pending hot-reload config or wait for hello ──
        let config = if let Some(cfg) = pending_config.take() {
            // Hot-reload: re-initialise decoder with the new resolution from ConfigUpdated,
            // without waiting for a new ClientHello (the TCP session is still alive).
            info!(
                "Display[{}] Hot-reloading decoder with updated config: {:?}",
                display_index, cfg
            );
            cfg
        } else {
            // Normal path: wait for the sender's hello handshake.
            let cfg = loop {
                match event_rx.recv().await {
                    Some(SignalingEvent::SessionStarted {
                        session_id,
                        device_name,
                        config,
                        client_addr,
                    }) => {
                        session_count += 1;
                        info!(
                            "Display[{}] Session #{} started: id={} from='{}' addr={} config={:?}",
                            display_index, session_count, session_id,
                            device_name, client_addr, config
                        );
                        break config;
                    }
                    Some(SignalingEvent::ClientDisconnected) => {
                        warn!(
                            "Display[{}] Client disconnected before hello — waiting again",
                            display_index
                        );
                    }
                    Some(other) => {
                        tracing::debug!("Display[{}] Pre-session event: {:?}", display_index, other);
                    }
                    None => {
                        // Channel closed permanently — no more connections possible
                        info!(
                            "Display[{}] Signaling channel closed (total sessions: {}). Exiting.",
                            display_index, session_count
                        );
                        break 'reconnect;
                    }
                }
            };
            cfg
        };

        // ── Initialise display decoder (new instance per session) ─────────
        let width  = config.resolution.width;
        let height = config.resolution.height;

        let display_decoder = match tokio::task::spawn_blocking(move || {
            DecoderFactory::best_available_with_display(width, height)
        })
        .await
        {
            Ok(Ok(dec)) => dec,
            Ok(Err(e)) => {
                warn!(
                    "Display[{}] Decoder init failed: {} — skipping session",
                    display_index, e
                );
                continue 'reconnect;
            }
            Err(e) => {
                warn!("Display[{}] Spawn-blocking panicked: {}", display_index, e);
                continue 'reconnect;
            }
        };

        let hw   = display_decoder.is_hardware_accelerated();
        let elem = display_decoder.element_name().to_string();
        info!(
            "Display[{}] Decoder ready: {} hw={} — video window should appear",
            display_index, elem, hw
        );

        // ── Dedicated blocking thread for decode + display + input ─────────
        let (decode_tx, mut decode_rx) = mpsc::channel::<EncodedFrame>(64);
        let push_errors = Arc::new(AtomicU64::new(0));
        let pe   = Arc::clone(&push_errors);
        let idx  = display_index;
        let is2  = input_sender.clone();

        let decode_handle = tokio::task::spawn_blocking(move || {
            while let Some(frame) = decode_rx.blocking_recv() {
                let sz = frame.data.len();
                let kf = frame.is_keyframe;
                match display_decoder.push_frame(frame) {
                    Ok(()) => {
                        let n = display_decoder.frames_pushed();
                        if n == 1 {
                            info!("Display[{idx}] First frame decoded and displayed!");
                        }
                        if n % 300 == 0 {
                            info!("Display[{idx}] Displayed {} frames", n);
                        }
                    }
                    Err(e) => {
                        let errs = pe.fetch_add(1, Ordering::Relaxed) + 1;
                        if errs <= 10 || errs % 100 == 0 {
                            warn!(
                                "Display[{idx}] push error #{} ({} bytes keyframe={}): {}",
                                errs, sz, kf, e
                            );
                        }
                    }
                }
                // Forward input events captured from the GStreamer window
                for event in display_decoder.poll_input_events() {
                    let _ = is2.try_send(event);
                }
            }
            info!("Display[{idx}] decode+display thread exiting");
        });

        // ── Main async receive → decode loop ───────────────────────────────
        info!(
            "Display[{}] Streaming — receiving and displaying frames...",
            display_index
        );
        let mut frames_received: u64 = 0;

        let session_exit_reason = loop {
            tokio::select! {
                // Incoming encoded frame
                Some(frame) = frame_rx.recv() => {
                    frames_received += 1;
                    if frames_received <= 5 {
                        tracing::debug!(
                            "Display[{}] Frame #{}: {} bytes keyframe={}",
                            display_index, frames_received, frame.data.len(), frame.is_keyframe
                        );
                    }
                    if frames_received % 300 == 0 {
                        let errs = push_errors.load(Ordering::Relaxed);
                        info!(
                            "Display[{}] Stats: received={} errors={}",
                            display_index, frames_received, errs
                        );
                    }
                    if decode_tx.send(frame).await.is_err() {
                        warn!("Display[{}] Decode thread gone — stopping session", display_index);
                        break "decode_thread_gone";
                    }
                }

                // Signaling events mid-session
                Some(event) = event_rx.recv() => {
                    match event {
                        SignalingEvent::SessionStopped { session_id } => {
                            info!(
                                "Display[{}] Session {} stopped by sender",
                                display_index, session_id
                            );
                            break "session_stopped";
                        }
                        SignalingEvent::ClientDisconnected => {
                            warn!("Display[{}] Sender disconnected unexpectedly", display_index);
                            break "client_disconnected";
                        }
                        SignalingEvent::ConfigUpdated { config: new_cfg } => {
                            info!("Display[{}] Config update received: {:?}", display_index, new_cfg);
                            let cur_w = config.resolution.width;
                            let cur_h = config.resolution.height;
                            if new_cfg.resolution.width != cur_w || new_cfg.resolution.height != cur_h {
                                info!(
                                    "Display[{}] Resolution change {}×{} → {}×{}: hot-reloading decoder",
                                    display_index,
                                    cur_w, cur_h,
                                    new_cfg.resolution.width, new_cfg.resolution.height
                                );
                                pending_config = Some(new_cfg);
                                break "config_updated";
                            }
                            // Same resolution — no decoder restart needed
                        }
                        _ => {}
                    }
                }

                else => break "channels_closed",
            }
        };

        // Signal decode thread to stop and wait for it
        drop(decode_tx);
        let _ = decode_handle.await;

        let total_errs = push_errors.load(Ordering::Relaxed);
        info!(
            "Display[{}] Session #{} complete ({}). received={} errors={}",
            display_index, session_count, session_exit_reason,
            frames_received, total_errs
        );

        // "channels_closed" means the transport layer shut down permanently
        if session_exit_reason == "channels_closed" {
            break 'reconnect;
        }

        // "config_updated": pending_config already set above — loop back to re-init decoder.
        // All other reasons: loop back and wait for the next sender connection.
    }

    info!("Display[{}] Receiver loop exited.", display_index);
    Ok(())
}

