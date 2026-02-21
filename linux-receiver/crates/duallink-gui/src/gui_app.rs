use egui::{
    Align, Color32, FontFamily, FontId, Frame, Layout, Margin, RichText,
    ScrollArea, Stroke, Vec2,
};

use crate::state::{Phase, SharedState};

// ── Colours ───────────────────────────────────────────────────────────────────

const BG_PANEL:  Color32 = Color32::from_rgb(28,  30,  36);
const BG_INSET:  Color32 = Color32::from_rgb(20,  22,  28);
const BG_CARD:   Color32 = Color32::from_rgb(36,  38,  46);
const ACCENT:    Color32 = Color32::from_rgb(99, 144, 255);
const TEXT_DIM:  Color32 = Color32::from_rgb(130, 135, 148);
const TEXT_NORM: Color32 = Color32::from_rgb(210, 215, 230);

// ── App struct ────────────────────────────────────────────────────────────────

pub struct DualLinkApp {
    state:              SharedState,
    show_fingerprint:   bool,
    auto_scroll_logs:   bool,
    copied_pin_frames:  u8,  // countdown for "Copied!" flash
}

impl DualLinkApp {
    pub fn new(cc: &eframe::CreationContext<'_>, state: SharedState) -> Self {
        // Apply dark visuals with custom colours
        let mut visuals = egui::Visuals::dark();
        visuals.window_fill             = BG_PANEL;
        visuals.panel_fill              = BG_PANEL;
        visuals.extreme_bg_color        = BG_INSET;
        visuals.faint_bg_color          = BG_CARD;
        visuals.widgets.inactive.bg_fill  = BG_CARD;
        visuals.widgets.hovered.bg_fill   = Color32::from_rgb(50, 53, 65);
        visuals.widgets.active.bg_fill    = Color32::from_rgb(65, 68, 82);
        cc.egui_ctx.set_visuals(visuals);

        // Slightly larger default font
        let mut style = (*cc.egui_ctx.style()).clone();
        style.text_styles.insert(
            egui::TextStyle::Body,
            FontId::new(14.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            FontId::new(13.5, FontFamily::Proportional),
        );
        cc.egui_ctx.set_style(style);

        Self {
            state,
            show_fingerprint:  false,
            auto_scroll_logs:  true,
            copied_pin_frames: 0,
        }
    }
}

// ── eframe::App implementation ────────────────────────────────────────────────

impl eframe::App for DualLinkApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Decrement "Copied!" flash countdown
        if self.copied_pin_frames > 0 {
            self.copied_pin_frames -= 1;
            ctx.request_repaint();
        }

        // Snapshot state to avoid holding the lock across rendering
        let snap = {
            let s = self.state.lock().unwrap();
            StateSnapshot {
                phase:           s.phase.clone(),
                pairing_pin:     s.pairing_pin.clone(),
                tls_fingerprint: s.tls_fingerprint.clone(),
                fps:             s.fps,
                frames_received: s.frames_received,
                frames_decoded:  s.frames_decoded,
                bitrate_mbps:    s.bitrate_mbps,
                transport:       s.transport.clone(),
                logs:            s.logs.iter().cloned().collect::<Vec<_>>(),
                lan_ip:          s.lan_ip.clone(),
                mdns_active:     s.mdns_active,
                display_count:   s.display_count,
            }
        };

        egui::CentralPanel::default()
            .frame(Frame::none().fill(BG_PANEL))
            .show(ctx, |ui| {
                ui.set_min_size(Vec2::new(540.0, 640.0));

                // ── Header ────────────────────────────────────────────────
                render_header(ui, &snap);
                ui.add_space(10.0);

                // ── Status card ───────────────────────────────────────────
                render_status_card(ui, &snap);
                ui.add_space(10.0);

                // ── PIN card (shown when not yet streaming) ───────────────
                let show_pin = !snap.pairing_pin.is_empty()
                    && !matches!(snap.phase, Phase::Error(_));
                if show_pin {
                    self.render_pin_card(ui, ctx, &snap);
                    ui.add_space(6.0);

                    // TLS fingerprint toggle
                    self.render_fingerprint_section(ui, &snap.tls_fingerprint);
                    ui.add_space(10.0);
                }

                // ── Streaming stats card ──────────────────────────────────
                if matches!(snap.phase, Phase::Streaming { .. }) {
                    render_stats_card(ui, &snap);
                    ui.add_space(10.0);
                }

                // ── Log panel ─────────────────────────────────────────────
                render_log_panel(ui, &snap.logs, &mut self.auto_scroll_logs);

                // ── Footer / quit button ──────────────────────────────────
                ui.add_space(8.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_sized(
                            [110.0, 30.0],
                            egui::Button::new(
                                RichText::new("Quit DualLink")
                                    .color(Color32::from_rgb(220, 80, 70)),
                            )
                            .fill(BG_CARD)
                            .stroke(Stroke::new(1.0, Color32::from_rgb(180, 60, 55))),
                        )
                        .clicked()
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
    }
}

// ── Rendering helpers ─────────────────────────────────────────────────────────

fn render_header(ui: &mut egui::Ui, snap: &StateSnapshot) {
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        // App name
        ui.label(
            RichText::new("DualLink")
                .font(FontId::new(26.0, FontFamily::Proportional))
                .strong()
                .color(Color32::WHITE),
        );
        ui.label(
            RichText::new("Receiver")
                .font(FontId::new(26.0, FontFamily::Proportional))
                .color(ACCENT),
        );

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(6.0);
            // Transport badge
            ui.label(
                RichText::new(&snap.transport)
                    .font(FontId::new(11.5, FontFamily::Proportional))
                    .color(TEXT_DIM),
            );
        });
    });

    // Thin accent separator
    let rect = ui.available_rect_before_wrap();
    let y    = ui.cursor().top();
    ui.painter().line_segment(
        [egui::pos2(rect.left() + 6.0, y), egui::pos2(rect.right() - 6.0, y)],
        Stroke::new(1.0, Color32::from_rgb(55, 58, 74)),
    );
    ui.add_space(4.0);
}

fn render_status_card(ui: &mut egui::Ui, snap: &StateSnapshot) {
    card(ui, |ui| {
        ui.horizontal(|ui| {
            // Coloured status dot
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
            ui.painter()
                .circle_filled(rect.center(), 5.0, snap.phase.color());

            ui.label(
                RichText::new(snap.phase.label())
                    .strong()
                    .color(TEXT_NORM),
            );

            // Extra peer info
            if let Some(name) = snap.phase.peer_name() {
                ui.label(RichText::new("—").color(TEXT_DIM));
                ui.label(
                    RichText::new(name)
                        .color(Color32::WHITE)
                        .strong(),
                );
                if let Some(addr) = snap.phase.peer_addr() {
                    ui.label(
                        RichText::new(format!("({})", addr))
                            .color(TEXT_DIM)
                            .font(FontId::new(12.0, FontFamily::Proportional)),
                    );
                }
            }

            // Error detail
            if let Phase::Error(msg) = &snap.phase {
                ui.label(
                    RichText::new(format!(": {}", msg))
                        .color(Color32::from_rgb(220, 100, 100))
                        .font(FontId::new(12.0, FontFamily::Proportional)),
                );
            }
        });
    });
}

impl DualLinkApp {
    fn render_pin_card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, snap: &StateSnapshot) {
        let pin = &snap.pairing_pin;
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Pairing PIN")
                        .color(TEXT_DIM)
                        .font(FontId::new(12.0, FontFamily::Proportional)),
                );
            });
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                // Big monospace PIN display
                ui.label(
                    RichText::new(pin)
                        .font(FontId::new(38.0, FontFamily::Monospace))
                        .strong()
                        .color(ACCENT),
                );

                // Copy button
                ui.add_space(12.0);
                let btn_label = if self.copied_pin_frames > 0 {
                    "Copied!"
                } else {
                    "Copy"
                };
                let btn_color = if self.copied_pin_frames > 0 {
                    Color32::from_rgb(60, 200, 80)
                } else {
                    TEXT_DIM
                };
                if ui
                    .add_sized(
                        [60.0, 28.0],
                        egui::Button::new(
                            RichText::new(btn_label)
                                .color(btn_color)
                                .font(FontId::new(12.5, FontFamily::Proportional)),
                        )
                        .fill(BG_INSET)
                        .stroke(Stroke::new(1.0, Color32::from_rgb(60, 65, 80))),
                    )
                    .clicked()
                {
                    ctx.copy_text(pin.to_string());
                    self.copied_pin_frames = 90; // ~1.5 s at 60 fps
                }
            });

            ui.add_space(2.0);
            ui.label(
                RichText::new("Enter this PIN in the macOS DualLink app to authorise the connection.")
                    .color(TEXT_DIM)
                    .font(FontId::new(12.0, FontFamily::Proportional)),
            );

            // LAN IP row — shown once detect_local_ip() has resolved
            if !snap.lan_ip.is_empty() {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let mdns_badge = if snap.mdns_active {
                        RichText::new("mDNS ✓")
                            .color(Color32::from_rgb(60, 200, 80))
                            .font(FontId::new(11.5, FontFamily::Proportional))
                    } else {
                        RichText::new("mDNS ✗")
                            .color(Color32::from_rgb(180, 100, 50))
                            .font(FontId::new(11.5, FontFamily::Proportional))
                    };
                    ui.label(mdns_badge);
                    ui.label(
                        RichText::new(format!("Connect from: {}  •  {} display{}",
                            snap.lan_ip,
                            snap.display_count,
                            if snap.display_count == 1 { "" } else { "s" }))
                            .color(TEXT_DIM)
                            .font(FontId::new(12.0, FontFamily::Proportional)),
                    );
                });
            }
        });
    }

    fn render_fingerprint_section(&mut self, ui: &mut egui::Ui, fp: &str) {
        if fp.is_empty() {
            return;
        }
        let header = RichText::new("▸ TLS certificate fingerprint")
            .font(FontId::new(12.0, FontFamily::Proportional))
            .color(TEXT_DIM);
        let header_open = RichText::new("▾ TLS certificate fingerprint")
            .font(FontId::new(12.0, FontFamily::Proportional))
            .color(TEXT_DIM);

        let toggle_label = if self.show_fingerprint { header_open } else { header };
        if ui.add(egui::Label::new(toggle_label).sense(egui::Sense::click())).clicked() {
            self.show_fingerprint = !self.show_fingerprint;
        }

        if self.show_fingerprint {
            ui.add_space(4.0);
            card(ui, |ui| {
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    ui.label(
                        RichText::new(fp)
                            .font(FontId::new(11.5, FontFamily::Monospace))
                            .color(Color32::from_rgb(120, 180, 120)),
                    );
                });
                ui.add_space(2.0);
                ui.label(
                    RichText::new("The macOS client accepts this certificate on first connect (TOFU).")
                        .font(FontId::new(11.5, FontFamily::Proportional))
                        .color(TEXT_DIM),
                );
            });
        }
    }
}

fn render_stats_card(ui: &mut egui::Ui, snap: &StateSnapshot) {
    card(ui, |ui| {
        ui.label(
            RichText::new("Streaming stats")
                .color(TEXT_DIM)
                .font(FontId::new(12.0, FontFamily::Proportional)),
        );
        ui.add_space(6.0);

        ui.horizontal_wrapped(|ui| {
            stat_chip(ui, "FPS",      &format!("{:.1}", snap.fps));
            stat_chip(ui, "Decoded",  &snap.frames_decoded.to_string());
            stat_chip(ui, "Received", &snap.frames_received.to_string());
            stat_chip(ui, "Bitrate",  &format!("{:.1} Mbit/s", snap.bitrate_mbps));
            stat_chip(ui, "Displays", &snap.display_count.to_string());
        });
    });
}

fn render_log_panel(
    ui: &mut egui::Ui,
    logs: &[String],
    auto_scroll: &mut bool,
) {
    // Header row with auto-scroll toggle
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Log")
                .color(TEXT_DIM)
                .font(FontId::new(12.0, FontFamily::Proportional)),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.checkbox(auto_scroll, RichText::new("auto-scroll").color(TEXT_DIM).font(FontId::new(11.5, FontFamily::Proportional)));
        });
    });
    ui.add_space(3.0);

    let available = ui.available_size();
    let log_height = (available.y - 55.0).max(140.0);

    Frame::none()
        .fill(BG_INSET)
        .inner_margin(Margin::symmetric(8.0, 6.0))
        .stroke(Stroke::new(1.0, Color32::from_rgb(45, 48, 60)))
        .rounding(egui::Rounding::same(6.0))
        .show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt("log_scroll")
                .max_height(log_height)
                .auto_shrink([false, false])
                .stick_to_bottom(*auto_scroll)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    for line in logs {
                        let color = if line.starts_with("[ERROR]") {
                            Color32::from_rgb(220, 80, 70)
                        } else if line.starts_with("[WARN]") {
                            Color32::from_rgb(220, 165, 50)
                        } else {
                            Color32::from_rgb(160, 170, 185)
                        };
                        ui.label(
                            RichText::new(line)
                                .font(FontId::new(11.5, FontFamily::Monospace))
                                .color(color),
                        );
                    }
                });
        });
}

// ── Utilities ─────────────────────────────────────────────────────────────────

fn card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    Frame::none()
        .fill(BG_CARD)
        .inner_margin(Margin::symmetric(12.0, 10.0))
        .rounding(egui::Rounding::same(8.0))
        .stroke(Stroke::new(1.0, Color32::from_rgb(50, 53, 68)))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add_contents(ui);
        });
}

fn stat_chip(ui: &mut egui::Ui, label: &str, value: &str) {
    Frame::none()
        .fill(BG_INSET)
        .inner_margin(Margin::symmetric(10.0, 6.0))
        .rounding(egui::Rounding::same(6.0))
        .stroke(Stroke::new(1.0, Color32::from_rgb(50, 53, 68)))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(value)
                        .font(FontId::new(20.0, FontFamily::Monospace))
                        .strong()
                        .color(Color32::WHITE),
                );
                ui.add_space(1.0);
                ui.label(
                    RichText::new(label)
                        .font(FontId::new(11.0, FontFamily::Proportional))
                        .color(TEXT_DIM),
                );
            });
        });
    ui.add_space(6.0);
}

// ── Snapshot (to avoid holding lock during paint) ─────────────────────────────

struct StateSnapshot {
    phase:           Phase,
    pairing_pin:     String,
    tls_fingerprint: String,
    fps:             f64,
    frames_received: u64,
    frames_decoded:  u64,
    bitrate_mbps:    f64,
    transport:       String,
    logs:            Vec<String>,
    lan_ip:          String,
    mdns_active:     bool,
    display_count:   u8,
}

// Forward Phase methods onto the snapshot for ergonomics in the renderer
impl std::ops::Deref for StateSnapshot {
    type Target = Phase;
    fn deref(&self) -> &Phase { &self.phase }
}
