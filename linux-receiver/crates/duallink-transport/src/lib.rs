use async_trait::async_trait;
use bytes::Bytes;
use duallink_core::{ConnectionMode, TransportError};
use tracing::{debug, info};

// MARK: - Transport trait

/// Interface abstrata para camada de transporte.
///
/// Permite trocar o transporte (Wi-Fi ↔ USB) sem alterar o pipeline de vídeo.
///
/// Implementações:
/// - `WebRtcTransport` — Wi-Fi via WebRTC (Fase 1)
/// - `UsbTransport` — USB-C via bulk transfer (Fase 3)
#[async_trait]
pub trait Transport: Send + Sync {
    /// Envia dados para o peer.
    async fn send(&self, data: Bytes) -> Result<(), TransportError>;

    /// Recebe dados do peer.
    async fn recv(&self) -> Result<Bytes, TransportError>;

    /// Fecha a conexão limpa.
    async fn close(&self) -> Result<(), TransportError>;

    /// Modo de conexão atual.
    fn mode(&self) -> ConnectionMode;

    /// Latência estimada em milissegundos (se disponível).
    fn estimated_latency_ms(&self) -> Option<u32>;
}

// MARK: - ConnectionInfo

#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub mode: ConnectionMode,
    pub remote_addr: String,
    pub estimated_latency_ms: Option<u32>,
    pub bandwidth_mbps: Option<f64>,
}
