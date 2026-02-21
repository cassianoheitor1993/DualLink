//! egui settings UI for the DualLink Windows Sender.
//!
//! Layout mirrors the Linux sender UI:
//!
//! ```
//! ┌────────────────────────────────────────────────────────┐
//! │  DualLink Windows Sender                               │
//! ├────────────────────────────────────────────────────────┤
//! │  Receiver IP  [192.168.1.100_______]  PIN  [123456__]  │
//! │  Discovered   [— select —___________]                  │
//! │  Displays [1▼]  Resolution [1920×1080___▼]  FPS [60▼]  │
//! │  Bitrate  [8000] kbps                                  │
//! ├────────────────────────────────────────────────────────┤
//! │  [▶ Start Streaming]          [■ Stop]                 │
//! ├────────────────────────────────────────────────────────┤
//! │  Display 0  ● Streaming  47.2 fps  12340 frames        │
//! └────────────────────────────────────────────────────────┘
//! ```

use std::collections::HashMap;

use eframe::egui::{self, Color32, RichText};
use tokio::runtime::Handle;
use tokio::sync::mpsc;

use crate::pipeline::{PipelineConfig, PipelineState, PipelineStatus, WinSenderPipeline};

// ── Discovered receiver (via mDNS) ────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct DiscoveredReceiver {
    pub name:     String,
    pub host:     String,
    pub port:     u16,
    pub displays: u8,
}

// ── WinSenderApp ──────────────────────────────────────────────────────────────

pub struct WinSenderApp {
    // ── Config ──
    host:           String,
    pairing_pin:    String,
    display_count:  usize,
    width:          u32,
    height:         u32,
    fps:            u32,
    bitrate_kbps:   u32,
    resolution_idx: usize,

    // ── Discovery ──
    discovered:     Vec<DiscoveredReceiver>,
    discovery_rx:   Option<mpsc::Receiver<DiscoveredReceiver>>,
    selected_peer:  Option<usize>,

    // ── Runtime ──
    running:   bool,
    pipelines: Vec<WinSenderPipeline>,
    status_rx: mpsc::Receiver<PipelineStatus>,
    status_tx: mpsc::Sender<PipelineStatus>,
    status:    HashMap<u8, PipelineStatus>,
    rt_handle: Handle,
}

impl WinSenderApp {
    pub fn new(rt_handle: Handle, _cc: &eframe::CreationContext<'_>) -> Self {
        let (status_tx, status_rx) = mpsc::channel::<PipelineStatus>(64);
        Self {
            host:           "192.168.1.100".to_owned(),
            pairing_pin:    "000000".to_owned(),
            display_count:  1,
            width:          1920,
            height:         1080,
            fps:            60,
            bitrate_kbps:   8000,
            resolution_idx: 2, // 1920×1080
            discovered:     Vec::new(),
            discovery_rx:   None,
            selected_peer:  None,
            running:        false,
            pipelines:      Vec::new(),
            status_rx,
            status_tx,
            status:         HashMap::new(),
            rt_handle,
        }
    }

    // ── mDNS browse ───────────────────────────────────────────────────────

    fn start_discovery(&mut self) {
        let (tx, rx) = mpsc::channel::<DiscoveredReceiver>(32);
        self.discovery_rx = Some(rx);
        self.discovered.clear();

        // Spawn async task that browses for _duallink._tcp.local.
        let _guard = self.rt_handle.enter();
        tokio::spawn(async move {
            browse_receivers(tx).await;
        });
    }

    fn poll_discovery(&mut self) {
        if let Some(rx) = &mut self.discovery_rx {
            while let Ok(peer) = rx.try_recv() {
                // Deduplicate by host
                if !self.discovered.iter().any(|p| p.host == peer.host) {
                    self.discovered.push(peer);
                }
            }
        }
    }

    // ── Pipeline lifecycle ────────────────────────────────────────────────

    fn start(&mut self) {
        if self.running { return; }
        self.running = true;
        self.status.clear();
        let _guard = self.rt_handle.enter();
        for i in 0..self.display_count as u8 {
            let cfg = PipelineConfig {
                host:          self.host.clone(),
                pairing_pin:   self.pairing_pin.clone(),
                display_index: i,
                width:         self.width,
                height:        self.height,
                fps:           self.fps,
                bitrate_kbps:  self.bitrate_kbps,
            };
            let pl = WinSenderPipeline::spawn(cfg, self.status_tx.clone());
            self.pipelines.push(pl);
        }
    }

    fn stop(&mut self) {
        for pl in &self.pipelines { pl.stop(); }
        self.pipelines.clear();
        self.running = false;
    }

    fn poll_status(&mut self) {
        while let Ok(s) = self.status_rx.try_recv() {
            self.status.insert(s.display_index, s);
        }
        if self.running {
            let done = self.status.values()
                .all(|s| matches!(s.state, PipelineState::Stopped | PipelineState::Failed(_)));
            if done && self.status.len() == self.display_count {
                self.running = false;
                self.pipelines.clear();
            }
        }
    }
}

impl eframe::App for WinSenderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_status();
        self.poll_discovery();
        ctx.request_repaint_after(std::time::Duration::from_millis(500));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
            ui.heading("DualLink Windows Sender");
            ui.separator();

            let locked = self.running;
            ui.add_enabled_ui(!locked, |ui| {
                egui::Grid::new("settings")
                    .num_columns(4)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        // Row 1: IP + PIN
                        ui.label("Receiver IP:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.host)
                                .hint_text("192.168.1.100")
                                .desired_width(160.0),
                        );
                        ui.label("PIN:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.pairing_pin)
                                .hint_text("000000")
                                .desired_width(70.0),
                        );
                        ui.end_row();

                        // Row 2: mDNS discovered receivers
                        ui.label("Discovered:");
                        let sel_label = self.selected_peer
                            .and_then(|i| self.discovered.get(i))
                            .map(|p| p.name.clone())
                            .unwrap_or_else(|| "— scan for receivers —".to_owned());
                        egui::ComboBox::from_id_source("discovered")
                            .selected_text(sel_label)
                            .width(200.0)
                            .show_ui(ui, |ui| {
                                for (i, peer) in self.discovered.iter().enumerate() {
                                    let label = format!("{} ({})", peer.name, peer.host);
                                    if ui.selectable_label(self.selected_peer == Some(i), &label).clicked() {
                                        self.selected_peer = Some(i);
                                        self.host = peer.host.clone();
                                    }
                                }
                            });
                        if ui.small_button("⟳ Scan").clicked() {
                            self.start_discovery();
                        }
                        ui.end_row();

                        // Row 3: Display count + Resolution
                        ui.label("Displays:");
                        egui::ComboBox::from_id_source("display_count")
                            .selected_text(format!("{}", self.display_count))
                            .width(50.0)
                            .show_ui(ui, |ui| {
                                for n in 1..=4usize {
                                    ui.selectable_value(&mut self.display_count, n, format!("{n}"));
                                }
                            });
                        ui.label("Resolution:");
                        egui::ComboBox::from_id_source("resolution")
                            .selected_text(format!("{}×{}", self.width, self.height))
                            .width(130.0)
                            .show_ui(ui, |ui| {
                                const RES: &[(u32, u32, &str)] = &[
                                    (3840, 2160, "3840×2160 (4K)"),
                                    (2560, 1440, "2560×1440 (2K)"),
                                    (1920, 1080, "1920×1080 (FHD)"),
                                    (1280, 720,  "1280×720  (HD)"),
                                ];
                                for (idx, (w, h, lbl)) in RES.iter().enumerate() {
                                    if ui.selectable_label(self.resolution_idx == idx, *lbl).clicked() {
                                        self.resolution_idx = idx;
                                        self.width = *w;
                                        self.height = *h;
                                    }
                                }
                            });
                        ui.end_row();

                        // Row 4: FPS + Bitrate
                        ui.label("FPS:");
                        egui::ComboBox::from_id_source("fps")
                            .selected_text(format!("{}", self.fps))
                            .width(55.0)
                            .show_ui(ui, |ui| {
                                for f in &[24u32, 30, 60] {
                                    ui.selectable_value(&mut self.fps, *f, format!("{f}"));
                                }
                            });
                        ui.label("Bitrate:");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.bitrate_kbps)
                                    .range(500..=50_000)
                                    .speed(100.0),
                            );
                            ui.label("kbps");
                        });
                        ui.end_row();
                    });
            });

            ui.separator();

            // ── Buttons ───────────────────────────────────────────────────
            ui.horizontal(|ui| {
                if !self.running {
                    if ui.add_sized([160.0, 32.0], egui::Button::new(
                        if self.display_count == 1 { "▶  Start Streaming" } else { "▶  Start All Displays" }
                    )).clicked() {
                        self.start();
                    }
                } else {
                    if ui.add_sized([120.0, 32.0], egui::Button::new("■  Stop")).clicked() {
                        self.stop();
                    }
                }
            });

            ui.separator();
            ui.label(RichText::new("Display Status").strong());

            if !self.running && self.status.is_empty() {
                ui.label(RichText::new("Configure above and click Start Streaming.").color(Color32::GRAY));
            }

            for i in 0..self.display_count as u8 {
                ui.horizontal(|ui| {
                    match self.status.get(&i) {
                        None => {
                            ui.label(format!("Display {i}"));
                            ui.label(RichText::new("⊘ Idle").color(Color32::GRAY));
                        }
                        Some(s) => {
                            ui.label(format!("Display {i}"));
                            match &s.state {
                                PipelineState::Connecting => {
                                    ui.label(RichText::new("⟳ Connecting…").color(Color32::YELLOW));
                                }
                                PipelineState::Streaming => {
                                    ui.label(RichText::new("● Streaming").color(Color32::GREEN));
                                    ui.label(format!("{:.1} fps", s.fps));
                                    ui.label(RichText::new(format!("{} frames", s.frames_sent)).color(Color32::GRAY));
                                }
                                PipelineState::Stopped => {
                                    ui.label(RichText::new("○ Stopped").color(Color32::GRAY));
                                }
                                PipelineState::Failed(msg) => {
                                    ui.label(RichText::new(format!("✗ {msg}")).color(Color32::RED));
                                }
                            }
                        }
                    }
                });
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.small(concat!("DualLink v", env!("CARGO_PKG_VERSION"), " (Windows)"));
            });
        });
    }
}

// ── mDNS browser task ─────────────────────────────────────────────────────────

async fn browse_receivers(tx: mpsc::Sender<DiscoveredReceiver>) {
    use mdns_sd::{ServiceDaemon, ServiceEvent};

    let daemon = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("[mDNS] Failed to create daemon: {}", e);
            return;
        }
    };

    let receiver = match daemon.browse("_duallink._tcp.local.") {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("[mDNS] Browse failed: {}", e);
            return;
        }
    };

    // Browse for up to 3 seconds
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);

    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() { break; }

        match tokio::time::timeout(remaining, receiver.recv_async()).await {
            Ok(Ok(ServiceEvent::ServiceResolved(info))) => {
                let name = info.get_hostname().trim_end_matches('.').to_owned();
                let host = info.get_properties()
                    .get("host")
                    .map(|v| v.val_str().to_owned())
                    .unwrap_or_else(|| {
                        info.get_addresses().iter().next()
                            .map(|a| a.to_string())
                            .unwrap_or_else(|| name.clone())
                    });
                let port = info.get_properties()
                    .get("port")
                    .and_then(|v| v.val_str().parse().ok())
                    .unwrap_or(7879u16);
                let displays = info.get_properties()
                    .get("displays")
                    .and_then(|v| v.val_str().parse().ok())
                    .unwrap_or(1u8);
                let display_name = info.get_fullname()
                    .split('.')
                    .next()
                    .unwrap_or(&name)
                    .to_owned();

                tracing::info!("[mDNS] Found receiver: {} @ {}:{}", display_name, host, port);
                let _ = tx.send(DiscoveredReceiver { name: display_name, host, port, displays }).await;
            }
            Ok(Ok(_)) | Ok(Err(_)) => {}
            Err(_) => break, // timeout
        }
    }

    let _ = daemon.shutdown();
}
