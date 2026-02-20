use async_trait::async_trait;
use duallink_core::DecodedFrame;
use thiserror::Error;

// MARK: - Renderer trait

/// Interface comum para renderizadores fullscreen.
///
/// Implementações planejadas:
/// - `WaylandRenderer` — via wgpu com Wayland surface
/// - `X11Renderer` — via wgpu com X11 surface
/// - `GstRenderer` — via GStreamer video sink (alternativa simples)
#[async_trait]
pub trait Renderer: Send + Sync {
    /// Inicializa o renderer e abre janela fullscreen.
    async fn initialize(&mut self, width: u32, height: u32) -> Result<(), RendererError>;

    /// Apresenta um frame decodificado na tela.
    async fn present(&mut self, frame: DecodedFrame) -> Result<(), RendererError>;

    /// Redimensiona o viewport.
    async fn resize(&mut self, width: u32, height: u32) -> Result<(), RendererError>;

    /// Fecha o renderer e libera recursos.
    async fn shutdown(&mut self);
}

// MARK: - RendererError

#[derive(Error, Debug)]
pub enum RendererError {
    #[error("Failed to initialize renderer: {0}")]
    InitializationFailed(String),

    #[error("Failed to present frame: {0}")]
    PresentFailed(String),

    #[error("Display system unavailable (Wayland/X11 not found)")]
    DisplaySystemUnavailable,
}

// MARK: - PlaceholderRenderer

/// Placeholder — será substituído pela implementação real na Fase 1 (Sprint 1.2.4).
pub struct PlaceholderRenderer;

#[async_trait]
impl Renderer for PlaceholderRenderer {
    async fn initialize(&mut self, _width: u32, _height: u32) -> Result<(), RendererError> {
        Err(RendererError::InitializationFailed("PlaceholderRenderer — not implemented".into()))
    }
    async fn present(&mut self, _frame: DecodedFrame) -> Result<(), RendererError> {
        Ok(())
    }
    async fn resize(&mut self, _width: u32, _height: u32) -> Result<(), RendererError> {
        Ok(())
    }
    async fn shutdown(&mut self) {}
}
