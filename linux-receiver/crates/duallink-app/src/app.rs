use anyhow::Result;
use duallink_core::{ConnectionState, StreamConfig};
use duallink_discovery::DiscoveryService;
use tracing::{info, warn};

/// Loop principal do receiver.
///
/// Sequência:
/// 1. Iniciar discovery mDNS
/// 2. Aguardar peer (macOS sender)
/// 3. Estabelecer sessão WebRTC
/// 4. Inicializar decoder (NVDEC/VAAPI)
/// 5. Inicializar renderer fullscreen
/// 6. Loop: receber frame → decodificar → renderizar
pub async fn run() -> Result<()> {
    let config = StreamConfig::default();
    info!("Config: {:?}", config);

    // Fase 1 — discovery
    let mut discovery = DiscoveryService::new();
    let mut peer_rx = discovery.start_browsing()
        .map_err(|e| anyhow::anyhow!("Discovery failed: {}", e))?;

    info!("Searching for DualLink sender on local network...");
    info!("Make sure the macOS DualLink app is running.");

    // Aguardar primeiro peer
    let Some(peer) = peer_rx.recv().await else {
        warn!("Discovery ended without finding a peer.");
        return Ok(());
    };

    info!("Found peer: {} at {}", peer.name, peer.socket_addr());

    // TODO: Sprint 1.2.2 — conectar via WebRTC signaling
    // TODO: Sprint 1.2.3 — inicializar decoder
    // TODO: Sprint 1.2.4 — inicializar renderer
    // TODO: Sprint 1.2.7 — loop principal receive → decode → render

    discovery.stop();
    Ok(())
}
