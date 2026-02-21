use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tracing::{info, warn};

use duallink_core::{detect_usb_ethernet, EncodedFrame, StreamConfig};
use duallink_decoder::DecoderFactory;
use duallink_discovery::{DualLinkAdvertiser, detect_local_ip};
use duallink_transport::{DualLinkReceiver, DisplayChannels, InputSender, SignalingEvent, SIGNALING_PORT};

use crate::state::{Phase, SharedState};

const SERVICE_NAME: &str = "duallink-receiver.service";

// ── Port release helpers ───────────────────────────────────────────────────────

/// Stop the systemd user service.  Works even when launched from a GUI session
/// (GNOME sets XDG_RUNTIME_DIR and the D-Bus socket in the environment).
fn stop_systemd_service() {
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "stop", SERVICE_NAME])
        .status();
}

/// Kill whatever process is currently holding UDP:7878 or TCP:7879.
/// Uses `fuser` (util-linux) which doesn't need D-Bus.
fn fuser_kill_ports() {
    // UDP 7878
    let _ = std::process::Command::new("fuser")
        .args(["-k", "7878/udp"])
        .status();
    // TCP 7879
    let _ = std::process::Command::new("fuser")
        .args(["-k", "7879/tcp"])
        .status();
}

/// True if anything is currently listening on TCP:7879 (fast path check).
fn port_is_busy() -> bool {
    std::net::TcpListener::bind("0.0.0.0:7879").is_err()
}

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

    // ── Step 0b: unconditionally release ports before binding ─────────────
    if tokio::task::spawn_blocking(port_is_busy).await.unwrap_or(false) {
        {
            let mut s = state.lock().unwrap();
            s.push_log(format!("Port 7879 busy — stopping {} and killing port holders…", SERVICE_NAME));
        }
        ctx.request_repaint();

        tokio::task::spawn_blocking(|| {
            stop_systemd_service();
            fuser_kill_ports();
        }).await.ok();

        // Wait up to 1.5 s in 150 ms steps for the port to free
        for _ in 0..10 {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            let still_busy = tokio::task::spawn_blocking(port_is_busy).await.unwrap_or(true);
            if !still_busy {
                break;
            }
        }

        {
            let mut s = state.lock().unwrap();
            s.push_log("Ports released — binding…".to_string());
        }
        ctx.request_repaint();
    }

    // ── Step 1: bind ports, generate PIN / TLS key, start all displays ────
    let display_count: u8 = std::env::var("DUALLINK_DISPLAY_COUNT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
        .max(1)
        .min(8);

    let (recv, mut channels, input_sender, startup) =
        match DualLinkReceiver::start_all(display_count).await {
            Ok(v) => v,
            Err(e) => {
                let msg = e.to_string();
                let hint = if msg.contains("Address already in use") {
                    format!(
                        "[ERROR] Port still in use after auto-stop. Run manually:\n\
                         systemctl --user stop {SERVICE_NAME}\n\
                         sudo fuser -k 7878/udp 7879/tcp\n\
                         Then reopen the GUI."
                    )
                } else {
                    format!("[ERROR] Failed to start receiver: {}", msg)
                };
                let mut s = state.lock().unwrap();
                s.phase = Phase::Error(msg);
                s.push_log(hint);
                ctx.request_repaint();
                return;
            }
        };
    // Keep `recv` alive for the lifetime of the process so background tasks
    // are not dropped.
    let _recv = recv;

    // ── Step 2: detect LAN IP and advertise via mDNS ─────────────────────
    let local_ip = detect_local_ip();
    let lan_ip_str = local_ip.to_string();

    let _advertiser = DualLinkAdvertiser::register(
        "DualLink Receiver",
        display_count,
        SIGNALING_PORT,
        local_ip,
        &startup.tls_fingerprint,
    )
    .map_err(|e| warn!("mDNS advertising unavailable: {e}"))
    .ok();

    {
        let mut s = state.lock().unwrap();
        s.pairing_pin     = startup.pairing_pin.clone();
        s.tls_fingerprint = startup.tls_fingerprint.clone();
        s.phase           = Phase::WaitingForClient;
        s.lan_ip          = lan_ip_str.clone();
        s.mdns_active     = _advertiser.is_some();
        s.display_count   = display_count;
        s.push_log(format!("Pairing PIN : {}", startup.pairing_pin));
        s.push_log(format!(
            "TLS fingerprint: {}…",
            &startup.tls_fingerprint[..startup.tls_fingerprint.len().min(32)]
        ));
        s.push_log(format!("LAN IP : {}  (mDNS: {})", lan_ip_str, if _advertiser.is_some() { "active" } else { "unavailable" }));
        s.push_log(format!("Display streams: {}", display_count));
        s.push_log("Ready — waiting for macOS DualLink client…");
    }
    ctx.request_repaint();

    // ── Step 3: spawn GUI-less loops for displays 1+ ─────────────────────
    // Display 0 is handled below (integrated with GUI state); displays 1+
    // run the same session-reconnect pattern but without GUI state updates.
    let extra_channels: Vec<DisplayChannels> = channels.drain(1..).collect();
    for ch in extra_channels {
        let is = input_sender.clone();
        tokio::spawn(async move {
            run_background_display(ch, is).await;
        });
    }

    // ── Step 4: display-0 session loop (GUI-integrated) ──────────────────
    let ch0 = match channels.into_iter().next() {
        Some(ch) => ch,
        None => {
            let mut s = state.lock().unwrap();
            s.phase = Phase::Error("No display channels returned".into());
            ctx.request_repaint();
            return;
        }
    };

    let DisplayChannels { mut frame_rx, mut event_rx, .. } = ch0;

    // Pending config forwarded from a mid-session ConfigUpdated (hot-reload).
    let mut pending_config: Option<StreamConfig> = None;

    'reconnect: loop {
        // ── 4a: wait for a client to connect (unless hot-reload) ─────────
        let (config, device_name, client_addr) = if let Some(cfg) = pending_config.take() {
            // Hot-reload: re-init decoder with new resolution in flying session
            let s = state.lock().unwrap();
            info!("Display[0] Hot-reload: new config {:?}", cfg);
            if let Phase::Streaming { ref peer_name, ref peer_addr } = s.phase {
                let pn = peer_name.clone();
                let pa = peer_addr.clone();
                drop(s);
                let addr = pa.parse().unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());
                (cfg, pn, addr)
            } else {
                drop(s);
                (cfg, "[hot-reload]".into(), "0.0.0.0:0".parse().unwrap())
            }
        } else {
            loop {
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

        // ── 4b: spawn decode+display thread ──────────────────────────────
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
        let push_errors = Arc::new(AtomicU64::new(0));
        let pe2 = Arc::clone(&push_errors);

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
                let kf    = frame.is_keyframe;
                match decoder.push_frame(frame) {
                    Ok(()) => {
                        pe2.fetch_add(0, Ordering::Relaxed); // no-op to keep pe2 alive
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
                        let errs = pe2.fetch_add(1, Ordering::Relaxed) + 1;
                        if errs <= 10 || errs % 120 == 0 {
                            let mut s = state2.lock().unwrap();
                            s.push_log(format!("[WARN] Decode error #{} ({} bytes kf={}): {}", errs, bytes, kf, e));
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

        // ── 4c: receive + forward frame loop ─────────────────────────────
        let session_exit_reason = loop {
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
                        break "decode_thread_gone";
                    }
                }

                event = event_rx.recv() => {
                    match event {
                        Some(SignalingEvent::SessionStopped { session_id }) => {
                            info!("Session {} stopped by sender", session_id);
                            break "session_stopped";
                        }
                        Some(SignalingEvent::ClientDisconnected) | None => {
                            warn!("Client disconnected");
                            break "client_disconnected";
                        }
                        Some(SignalingEvent::ConfigUpdated { config: new_cfg }) => {
                            let cur_w = config.resolution.width;
                            let cur_h = config.resolution.height;
                            if new_cfg.resolution.width != cur_w || new_cfg.resolution.height != cur_h {
                                let mut s = state.lock().unwrap();
                                s.push_log(format!(
                                    "Resolution change {}×{} → {}×{}: hot-reloading decoder",
                                    cur_w, cur_h,
                                    new_cfg.resolution.width, new_cfg.resolution.height
                                ));
                                drop(s);
                                ctx.request_repaint();
                                pending_config = Some(new_cfg);
                                break "config_updated";
                            } else {
                                let mut s = state.lock().unwrap();
                                s.push_log(format!(
                                    "Config update: {}×{} @ {} fps",
                                    new_cfg.resolution.width,
                                    new_cfg.resolution.height,
                                    new_cfg.target_fps
                                ));
                            }
                        }
                        _ => {}
                    }
                }
            }
        };

        // Drop sender → decode thread will drain and exit
        drop(decode_tx);
        let _ = decode_handle.await;

        info!("Display[0] session exit: {}", session_exit_reason);

        if session_exit_reason == "channels_closed" {
            break 'reconnect;
        }

        // If hot-reload: pending_config is already set; skip the reset below.
        if session_exit_reason != "config_updated" {
            // ── 4d: reset for next session ────────────────────────────────
            {
                let mut s = state.lock().unwrap();
                s.phase = Phase::WaitingForClient;
                s.reset_stats();
                let pin = s.pairing_pin.clone();
                s.push_log("Client disconnected — waiting for new connection…");
                s.push_log(format!("Pairing PIN still valid: {}", pin));
            }
            ctx.request_repaint();

            // Brief pause so the OS has time to clean up the prior TCP conn
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }
}

// ── Background display loop (no GUI state) ────────────────────────────────────

/// Handles one extra display (index ≥ 1) without touching the GUI state.
async fn run_background_display(ch: DisplayChannels, input_sender: InputSender) {
    let DisplayChannels { display_index, mut frame_rx, mut event_rx } = ch;
    let mut pending_config: Option<StreamConfig> = None;

    'reconnect: loop {
        // Wait for SessionStarted or use hot-reload config
        let config = if let Some(cfg) = pending_config.take() {
            cfg
        } else {
            loop {
                match event_rx.recv().await {
                    Some(SignalingEvent::SessionStarted { config, .. }) => break config,
                    Some(SignalingEvent::ClientDisconnected) => {
                        warn!("Display[{}] disconnected before hello", display_index);
                    }
                    None => {
                        info!("Display[{}] channel closed — exiting", display_index);
                        break 'reconnect;
                    }
                    _ => {}
                }
            }
        };

        let width  = config.resolution.width;
        let height = config.resolution.height;
        let (decode_tx, mut decode_rx) = tokio::sync::mpsc::channel::<EncodedFrame>(64);
        let is2 = input_sender.clone();

        let handle = tokio::task::spawn_blocking(move || {
            if let Ok(dec) = DecoderFactory::best_available_with_display(width, height) {
                while let Some(frame) = decode_rx.blocking_recv() {
                    let _ = dec.push_frame(frame);
                    for ev in dec.poll_input_events() {
                        let _ = is2.try_send(ev);
                    }
                }
            }
        });

        let exit_reason = loop {
            tokio::select! {
                Some(frame) = frame_rx.recv() => {
                    if decode_tx.send(frame).await.is_err() { break "decode_gone"; }
                }
                Some(evt) = event_rx.recv() => {
                    match evt {
                        SignalingEvent::SessionStopped { .. } => break "stopped",
                        SignalingEvent::ClientDisconnected => break "disconnected",
                        SignalingEvent::ConfigUpdated { config: new_cfg } => {
                            let cur_w = config.resolution.width;
                            let cur_h = config.resolution.height;
                            if new_cfg.resolution.width != cur_w || new_cfg.resolution.height != cur_h {
                                pending_config = Some(new_cfg);
                                break "config_updated";
                            }
                        }
                        _ => {}
                    }
                }
                else => break "closed",
            }
        };

        drop(decode_tx);
        let _ = handle.await;

        if exit_reason == "closed" { break 'reconnect; }
        if exit_reason != "config_updated" {
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }
}
