use duallink_core::PeerInfo;
use mdns_sd::{ServiceBrowser, ServiceDaemon, ServiceEvent};
use tracing::{debug, info, warn};

pub const SERVICE_TYPE: &str = "_duallink._tcp.local.";
pub const DEFAULT_PORT: u16 = 8443;

/// Descobre dispositivos DualLink na rede local via mDNS.
pub struct DiscoveryService {
    daemon: Option<ServiceDaemon>,
}

impl DiscoveryService {
    pub fn new() -> Self {
        Self { daemon: None }
    }

    /// Inicia a busca por peers DualLink.
    /// Retorna um channel receptor que emitirá `PeerInfo` conforme peers forem encontrados.
    pub fn start_browsing(&mut self) -> Result<tokio::sync::mpsc::Receiver<PeerInfo>, DiscoveryError> {
        let daemon = ServiceDaemon::new().map_err(|e| DiscoveryError::DaemonFailed(e.to_string()))?;
        let receiver = daemon
            .browse(SERVICE_TYPE)
            .map_err(|e| DiscoveryError::BrowseFailed(e.to_string()))?;

        let (tx, rx) = tokio::sync::mpsc::channel(16);

        tokio::spawn(async move {
            while let Ok(event) = receiver.recv_async().await {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        info!("[Discovery] Found peer: {}", info.get_fullname());
                        let addresses: Vec<_> = info.get_addresses().iter().collect();
                        if let Some(addr) = addresses.first() {
                            let peer = PeerInfo::new(
                                info.get_fullname(),
                                info.get_hostname().trim_end_matches('.'),
                                addr.to_string(),
                                info.get_port(),
                            );
                            let _ = tx.send(peer).await;
                        }
                    }
                    ServiceEvent::ServiceRemoved(_, fullname) => {
                        debug!("[Discovery] Peer gone: {}", fullname);
                    }
                    _ => {}
                }
            }
        });

        self.daemon = Some(daemon);
        Ok(rx)
    }

    pub fn stop(&mut self) {
        if let Some(daemon) = self.daemon.take() {
            let _ = daemon.shutdown();
        }
    }
}

impl Default for DiscoveryService {
    fn default() -> Self {
        Self::new()
    }
}

// MARK: - DiscoveryError

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("mDNS daemon failed to start: {0}")]
    DaemonFailed(String),

    #[error("Failed to browse service: {0}")]
    BrowseFailed(String),
}
