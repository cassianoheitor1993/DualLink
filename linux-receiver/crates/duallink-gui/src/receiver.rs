use std::sync::Arc;

use tracing::{info, warn};

use duallink_core::{detect_usb_ethernet, EncodedFrame};
use duallink_decoder::DecoderFactory;
use duallink_transport::{DualLinkReceiver, SignalingEvent};

use crate::state::{Phase, SharedState};

// ── Entry point (called from the tokio runtime thread) ─────────────────────────

/// Runs the entire receiver lifecycle.  Never returns under normal operation;
/// exits only when the channel pair is closed (process is shutting down) or on
/// a fatal start-up error.
pub async fn run(state: SharedState, ctx: egui::Context) {
    // ── Step 0: transport detection ───────────────────────────────────────
    {
        let mut s = state.lock().unwrap();
        match detect_usb_ethernet() {
            Some(usb) => {
                s.transport = format!("USB ({})", usb.local_ip);
                s.push_log(format!(
                    "USB Ethernet detected: {} → {} (peer {})",
                    usb.interface_name, usb.local_ip, usb.peer_ip
                ));
            }
            None => {
                s.transport = "Wi-Fi".into();
                s.push_log("No USB Ethernet interface found — using Wi-Fi transport");
            }
        }
        s.push_log("Binding UDP:7878 (video) + TCP:7879 (signaling)…");
    }
    ctx.request_repaint();

    // ── Step 1: bind ports and generate PIN / TLS key ─────────────────────
    let (recv, mut frame_rx, mut event_rx, input_sender, startup) =
        match DualLinkReceiver::start().await {
            Ok(v) => v,
            Err(e) => {
                let mut s = state.lock().unwrap();
                let msg = format!("Failed to start receiver: {}", e);
                s.phase = Phase::Error(msg.clone());
                s.push_log(format!("[ERROR] {}", msg));
                ctx.request_repaint();
                return;
            }
        };
    // Keep `recv` alive for the lifetime of the process so background tasks
    // are not dropped.
    let _recv = recv;

    {
        let mut s = state.lock().unwrap();
        s.pairing_pin     = startup.pairing_pin.clone();
        s.tls_fingerprint = startup.tls_fingerprint.clone();
        s.phase           = Phase::WaitingForClient;
        s.push_log(format!("Pairing PIN : {}", startup.pairing_pin));
        s.push_log(format!(
            "TLS fingerprint: {}…",
            &startup.tls_fingerprint[..startup.tls_fingerprint.len().min(32)]
        ));
        s.push_log("Ready — waiting for macOS DualLink client…");
    }
    ctx.request_repaint();

    // ── Step 2: session loop (handles reconnects) ─────────────────────────
    loop {
        // ── 2a: wait for a client to connect ─────────────────────────────
        let (config, device_name, client_addr) = loop {
            match event_rx.recv().await {
                Some(SignalingEvent::SessionStarted {
                    device_name,
                    config,
                    client_addr,
                    ..
                }) => {
                    break (config, device_name, client_addr);
                }
                Some(SignalingEvent::ClientDisconnected) => {
                    let mut s = state.lock().unwrap();
                    s.push_log("Client disconnected before completing pairing");
                    ctx.request_repaint();
                }
                None => return, // All senders dropped → process shutting down
                _ => {}
            }
        };

        {
            let mut s = state.lock().unwrap();
            s.phase = Phase::Connected {
                peer_name: device_name.clone(),
                peer_addr: client_addr.to_string(),
            };
            s.frames_received = 0;
            s.push_log(format!(
                "Client '{}' connected from {}",
                device_name, client_addr
            ));
        }
        ctx.request_repaint();

        // ── 2b: spawn decode+display thread ──────────────────────────────
        //
        // GStreamer MUST be initialised and used on a single OS thread
        // (it creates a display window + message loop).  We use
        // spawn_blocking so Tokio does not timeslice us off.
        let width  = config.resolution.width;
        let height = config.resolution.height;
        let (decode_tx, mut decode_rx) =
            tokio::sync::mpsc::channel::<EncodedFrame>(64);

        let state2     = Arc::clone(&state);
        let ctx2       = ctx.clone();
        let input_fwd  = input_sender.clone();

        let decode_handle = tokio::task::spawn_blocking(move || {
            // Create decoder (and start GStreamer pipeline / video window).
            let decoder = match DecoderFactory::best_available_with_display(width, height) {
                Ok(d) => d,
                Err(e) => {
                    let mut s = state2.lock().unwrap();
                    s.push_log(format!("[ERROR] Decoder init: {}", e));
                    ctx2.request_repaint();
                    return;
                }
            };

            {
                let mut s = state2.lock().unwrap();
                s.push_log(format!(
                    "Decoder: {} (hw={})",
                    decoder.element_name(),
                    decoder.is_hardware_accelerated()
                ));
            }
            ctx2.request_repaint();

            // Frame loop
            while let Some(frame) = decode_rx.blocking_recv() {
                let bytes = frame.data.len();
                match decoder.push_frame(frame) {
                    Ok(()) => {
                        let mut s = state2.lock().unwrap();
                        // Promote phase to Streaming on first successfully decoded frame
                        if let Phase::Connected { peer_name, peer_addr } = s.phase.clone() {
                            s.phase = Phase::Streaming { peer_name, peer_addr };
                        }
                        s.tick_frame(bytes);
                        let fd = s.frames_decoded;
                        drop(s);
                        // Repaint the GUI roughly every 30 decoded frames (~2× per second at 60 fps)
                        if fd % 30 == 0 {
                            ctx2.request_repaint();
                        }
                    }
                    Err(e) => {
                        let count = {
                            let s = state2.lock().unwrap();
                            s.frames_decoded
                        };
                        if count < 20 || count % 120 == 0 {
                            let mut s = state2.lock().unwrap();
                            s.push_log(format!("[WARN] Decode error: {}", e));
                        }
                    }
                }

                // Forward any mouse/keyboard events captured inside the video window
                for event in decoder.poll_input_events() {
                    let _ = input_fwd.try_send(event);
                }
            }

            info!("Decode thread exiting");
        });

        // ── 2c: receive + forward frame loop ─────────────────────────────
        loop {
            tokio::select! {
                frame = frame_rx.recv() => {
                    let Some(frame) = frame else {
                        // frame_rx closed → process shutting down
                        drop(decode_tx);
                        let _ = decode_handle.await;
                        return;
                    };
                    {
                        let mut s = state.lock().unwrap();
                        s.frames_received += 1;
                    }
                    if decode_tx.send(frame).await.is_err() {
                        warn!("Decode thread gone — stopping session");
                        break;
                    }
                }

                event = event_rx.recv() => {
                    match event {
                        Some(SignalingEvent::SessionStopped { session_id }) => {
                            info!("Session {} stopped by sender", session_id);
                            break;
                        }
                        Some(SignalingEvent::ClientDisconnected) | None => {
                            warn!("Client disconnected");
                            break;
                        }
                        Some(SignalingEvent::ConfigUpdated { config }) => {
                            let mut s = state.lock().unwrap();
                            s.push_log(format!(
                                "Config update: {}×{} @ {} fps",
                                config.resolution.width,
                                config.resolution.height,
                                config.target_fps
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }

        // Drop sender → decode thread will drain and exit
        drop(decode_tx);
        let _ = decode_handle.await;

        // ── 2d: reset for next session ────────────────────────────────────
        {
            let mut s = state.lock().unwrap();
            s.phase = Phase::WaitingForClient;
            s.reset_stats();
            let pin = s.pairing_pin.clone();
            s.push_log("Client disconnected — waiting for new connection…");
            s.push_log(format!("Pairing PIN still valid: {}", pin));
        }
        ctx.request_repaint();
    }
}
