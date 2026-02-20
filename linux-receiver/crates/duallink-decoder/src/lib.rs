use async_trait::async_trait;
use duallink_core::{DecodedFrame, DecoderError, EncodedFrame};

// MARK: - VideoDecoder trait

/// Interface comum para decoders de vídeo.
///
/// Implementações:
/// - `NvDecDecoder` — NVDEC via GStreamer (NVIDIA, Fase 1)
/// - `VaapiDecoder` — VAAPI via GStreamer (Intel/AMD, Fase 1)
/// - `SoftwareDecoder` — FFmpeg software (fallback)
#[async_trait]
pub trait VideoDecoder: Send + Sync {
    /// Inicializa o decoder para o codec especificado.
    async fn initialize(&mut self, config: &duallink_core::StreamConfig) -> Result<(), DecoderError>;

    /// Decodifica um frame encodado.
    async fn decode(&mut self, frame: EncodedFrame) -> Result<DecodedFrame, DecoderError>;

    /// Libera recursos do decoder.
    async fn shutdown(&mut self);

    /// True se usar aceleração de hardware.
    fn is_hardware_accelerated(&self) -> bool;

    /// Nome do decoder (para logs e diagnóstico).
    fn name(&self) -> &str;
}

// MARK: - DecoderFactory

/// Seleciona o melhor decoder disponível no sistema.
pub struct DecoderFactory;

impl DecoderFactory {
    /// Retorna o decoder de maior performance disponível.
    ///
    /// Ordem de preferência:
    /// 1. NVDEC (NVIDIA GPU)
    /// 2. VAAPI (Intel/AMD GPU)
    /// 3. Software (CPU — fallback)
    pub fn best_available() -> Box<dyn VideoDecoder> {
        // TODO: Sprint 0.3.1 — implementar detecção e criação dos decoders
        // Por enquanto, retornar placeholder
        Box::new(PlaceholderDecoder)
    }
}

// MARK: - PlaceholderDecoder (Sprint 0.3 — substituir por GStreamer)

struct PlaceholderDecoder;

#[async_trait]
impl VideoDecoder for PlaceholderDecoder {
    async fn initialize(&mut self, _config: &duallink_core::StreamConfig) -> Result<(), DecoderError> {
        Err(DecoderError::NotInitialized)
    }

    async fn decode(&mut self, _frame: EncodedFrame) -> Result<DecodedFrame, DecoderError> {
        Err(DecoderError::NotInitialized)
    }

    async fn shutdown(&mut self) {}

    fn is_hardware_accelerated(&self) -> bool { false }
    fn name(&self) -> &str { "PlaceholderDecoder" }
}
