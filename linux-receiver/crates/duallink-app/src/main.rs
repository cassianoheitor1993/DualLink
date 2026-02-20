use anyhow::Result;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

mod app;

#[tokio::main]
async fn main() -> Result<()> {
    // Inicializar logging
    // Usar RUST_LOG=debug para mais detalhes
    // Usar GST_DEBUG=3 para GStreamer debug
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .with_thread_ids(false)
        .init();

    info!("DualLink Receiver v{}", env!("CARGO_PKG_VERSION"));
    info!("Starting...");

    // Iniciar o app principal
    match app::run().await {
        Ok(()) => {
            info!("DualLink Receiver exited cleanly.");
            Ok(())
        }
        Err(e) => {
            error!("Fatal error: {:#}", e);
            Err(e)
        }
    }
}
