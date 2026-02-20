mod gui_app;
mod receiver;
mod state;

use std::sync::{Arc, Mutex};

use state::GuiState;

fn main() -> eframe::Result<()> {
    // ── Logging ───────────────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .compact()
        .init();

    // ── Shared state ──────────────────────────────────────────────────────
    let shared_state: state::SharedState = Arc::new(Mutex::new(GuiState::default()));

    // ── Window options ────────────────────────────────────────────────────
    let window_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("DualLink Receiver")
            .with_inner_size([560.0, 720.0])
            .with_min_inner_size([420.0, 500.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "DualLink Receiver",
        window_options,
        Box::new(|cc| {
            // Clone state for the background task
            let state_bg = Arc::clone(&shared_state);
            let ctx_bg   = cc.egui_ctx.clone();

            // Spawn a dedicated OS thread running a tokio multi-thread runtime.
            // This keeps the async receiver entirely off the egui/glow main thread.
            std::thread::Builder::new()
                .name("duallink-receiver".into())
                .spawn(move || {
                    let rt = tokio::runtime::Builder::new_multi_thread()
                        .worker_threads(4)
                        .enable_all()
                        .build()
                        .expect("Failed to build tokio runtime");

                    rt.block_on(receiver::run(state_bg, ctx_bg));
                })
                .expect("Failed to spawn receiver thread");

            Ok(Box::new(gui_app::DualLinkApp::new(cc, shared_state)))
        }),
    )
}
