//! egui settings UI for the DualLink Linux Sender.
//!
//! Provides a small settings window that replaces env-var-only configuration.
//!
//! # Layout
//!
//! ```
//! ┌─────────────────────────────────────────────────────┐
//! │  🖥  DualLink Linux Sender                          │
//! ├─────────────────────────────────────────────────────┤
//! │  Host  [192.168.1.100________]  PIN  [123456__]     │
//! │  Displays  [1 ▼]  Resolution  [1920x1080 ▼]  FPS [60]│
//! │  Bitrate  [8000] kbps                               │
//! ├─────────────────────────────────────────────────────┤
//! │  [   Start Streaming   ]  [  Stop  ]               │
//! ├─────────────────────────────────────────────────────┤
//! │  Display 0  ● Streaming  47.2 fps  12340 frames     │
//! │  Display 1  ○ Stopped                               │
//! └─────────────────────────────────────────────────────┘
//! ```

use std::collections::HashMap;

use eframe::egui::{self, Color32, RichText};
use tokio::sync::mpsc;
use tokio::runtime::Handle;

use crate::pipeline::{PipelineConfig, PipelineState, PipelineStatus, SenderPipeline};

// ── SenderApp ─────────────────────────────────────────────────────────────────

/// egui application for the Linux sender.
pub struct SenderApp {
    // ── Configuration fields ──
    host:          String,
    pairing_pin:   String,
    display_count: usize,
    width:         u32,
    height:        u32,
    fps:           u32,
    bitrate_kbps:  u32,
    /// Index into RESOLUTIONS table.
    resolution_idx: usize,

    // ── Runtime state ──
    running: bool,
    /// Pipeline handles — one per active display.
    pipelines: Vec<crate::pipeline::SenderPipeline>,
    /// Channel for receiving status updates from pipelines.
    status_rx:    mpsc::Receiver<PipelineStatus>,
    /// Sender used to create new status channels when pipelines are (re)spawned.
    status_tx_template: mpsc::Sender<PipelineStatus>,
    /// Latest status per display index.
    status: HashMap<u8, PipelineStatus>,

    // ── tokio handle for spawning tasks ──
    rt_handle: Handle,
}

impl SenderApp {
    /// Create a new sender app with a tokio runtime handle.
    pub fn new(rt_handle: Handle, cc: &eframe::CreationContext<'_>) -> Self {
        let (status_tx, status_rx) = mpsc::channel::<PipelineStatus>(64);
        Self {
            host:          "192.168.1.100".to_owned(),
            pairing_pin:   "000000".to_owned(),
            display_count: 1,
            width:         1920,
            height:        1080,
            fps:           60,
            bitrate_kbps:  8000,
            resolution_idx: 2, // 1920×1080
            running: false,
            pipelines: Vec::new(),
            status_rx,
            status_tx_template: status_tx,
            status: HashMap::new(),
            rt_handle,
        }
    }

    fn start(&mut self) {
        if self.running {
            return;
        }
        self.running = true;
        self.status.clear();

        // Spawn N pipelines
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
            let status_tx = self.status_tx_template.clone();
            // Enter the tokio runtime context so tokio::spawn works from eframe's main thread.
            let _guard = self.rt_handle.enter();
            let pl = SenderPipeline::spawn(cfg, status_tx);
            self.pipelines.push(pl);
        }
    }

    fn stop(&mut self) {
        for pl in &self.pipelines {
            pl.stop();
        }
        self.pipelines.clear();
        self.running = false;
    }

    fn poll_status(&mut self) {
        while let Ok(s) = self.status_rx.try_recv() {
            // If all displays are Stopped or Failed, mark as not running
            self.status.insert(s.display_index, s);
        }
        if self.running {
            let all_done = self
                .status
                .values()
                .all(|s| matches!(s.state, PipelineState::Stopped | PipelineState::Failed(_)));
            if all_done && self.display_count as usize == self.status.len() {
                self.running = false;
                self.pipelines.clear();
            }
        }
    }
}

impl eframe::App for SenderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll status updates every frame
        self.poll_status();
        // Request a repaint so the UI stays fresh even without user interaction
        ctx.request_repaint_after(std::time::Duration::from_millis(500));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);

            // ── Title ─────────────────────────────────────────────────────
            ui.heading("DualLink Linux Sender");
            ui.separator();

            // ── Connection settings ───────────────────────────────────────
            let enabled = !self.running;
            ui.add_enabled_ui(enabled, |ui| {
                egui::Grid::new("settings_grid")
                    .num_columns(4)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        // Row 1: Host + PIN
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

                        // Row 2: Display count + Resolution
                        ui.label("Displays:");
                        egui::ComboBox::from_id_source("display_count")
                            .selected_text(format!("{}", self.display_count))
                            .width(60.0)
                            .show_ui(ui, |ui| {
                                for n in 1..=4usize {
                                    ui.selectable_value(
                                        &mut self.display_count,
                                        n,
                                        format!("{n}"),
                                    );
                                }
                            });

                        ui.label("Resolution:");
                        egui::ComboBox::from_id_source("resolution")
                            .selected_text(format!("{}×{}", self.width, self.height))
                            .width(120.0)
                            .show_ui(ui, |ui| {
                                const RESOLUTIONS: &[(u32, u32, &str)] = &[
                                    (3840, 2160, "3840×2160 (4K)"),
                                    (2560, 1440, "2560×1440 (2K)"),
                                    (1920, 1080, "1920×1080 (FHD)"),
                                    (1280, 720,  "1280×720  (HD)"),
                                ];
                                for (idx, (w, h, label)) in RESOLUTIONS.iter().enumerate() {
                                    if ui.selectable_label(self.resolution_idx == idx, *label).clicked() {
                                        self.resolution_idx = idx;
                                        self.width = *w;
                                        self.height = *h;
                                    }
                                }
                            });
                        ui.end_row();

                        // Row 3: FPS + Bitrate
                        ui.label("FPS:");
                        egui::ComboBox::from_id_source("fps")
                            .selected_text(format!("{}", self.fps))
                            .width(60.0)
                            .show_ui(ui, |ui| {
                                for f in &[24u32, 30, 60] {
                                    ui.selectable_value(&mut self.fps, *f, format!("{f}"));
                                }
                            });

                        ui.label("Bitrate:");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.bitrate_kbps)
                                    .range(500..=50000)
                                    .speed(100.0),
                            );
                            ui.label("kbps");
                        });
                        ui.end_row();
                    });
            });

            ui.separator();

            // ── Action buttons ────────────────────────────────────────────
            ui.horizontal(|ui| {
                if !self.running {
                    if ui
                        .add_sized(
                            [150.0, 32.0],
                            egui::Button::new(
                                if self.display_count == 1 {
                                    "▶  Start Streaming"
                                } else {
                                    "▶  Start All Displays"
                                },
                            ),
                        )
                        .clicked()
                    {
                        self.start();
                    }
                } else {
                    if ui
                        .add_sized([120.0, 32.0], egui::Button::new("■  Stop"))
                        .clicked()
                    {
                        self.stop();
                    }
                }
            });

            ui.separator();

            // ── Per-display status ────────────────────────────────────────
            ui.label(RichText::new("Display Status").strong());

            if !self.running && self.status.is_empty() {
                ui.label(
                    RichText::new("Not connected — configure and click Start Streaming.")
                        .color(Color32::GRAY),
                );
            }

            for i in 0..self.display_count as u8 {
                let status = self.status.get(&i);
                ui.horizontal(|ui| {
                    match status {
                        None => {
                            ui.label(format!("Display {i}"));
                            ui.label(RichText::new("⊘ Idle").color(Color32::GRAY));
                        }
                        Some(s) => {
                            ui.label(format!("Display {i}"));
                            match &s.state {
                                PipelineState::Connecting => {
                                    ui.label(
                                        RichText::new("⟳ Connecting…")
                                            .color(Color32::YELLOW),
                                    );
                                }
                                PipelineState::Streaming => {
                                    ui.label(
                                        RichText::new("● Streaming")
                                            .color(Color32::GREEN),
                                    );
                                    ui.label(format!("{:.1} fps", s.fps));
                                    ui.label(
                                        RichText::new(format!("{} frames", s.frames_sent))
                                            .color(Color32::GRAY),
                                    );
                                }
                                PipelineState::Stopped => {
                                    ui.label(
                                        RichText::new("○ Stopped").color(Color32::GRAY),
                                    );
                                }
                                PipelineState::Failed(msg) => {
                                    ui.label(
                                        RichText::new(format!("✗ {msg}"))
                                            .color(Color32::RED),
                                    );
                                }
                            }
                        }
                    }
                });
            }

            // ── Footer ────────────────────────────────────────────────────
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.small(concat!("DualLink v", env!("CARGO_PKG_VERSION")));
            });
        });
    }
}
