use async_trait::async_trait;
use duallink_core::DecodedFrame;
use thiserror::Error;

// MARK: - Renderer trait

/// Interface comum para renderizadores fullscreen.
///
/// Implementações:
/// - `GStreamerDisplayRenderer` — Sprint 2.1 — combined decode+display via
///   GStreamer `autovideosink` (see `duallink-decoder::GStreamerDisplayDecoder`)
/// - Future: `WgpuRenderer` — direct GPU rendering via wgpu (Sprint 3+)
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

// MARK: - GStreamer Display Renderer (Sprint 2.1)
//
// The actual display rendering is done by `GStreamerDisplayDecoder` in the
// decoder crate, which creates a combined decode+display GStreamer pipeline
// ending with `autovideosink`.  This avoids unnecessary CPU copies and
// leverages GStreamer's native windowing/compositing.
//
// Pipeline: appsrc → h264parse → vaapih264dec → autovideosink
//
// The `Renderer` trait with `DecodedFrame` input is preserved for future use
// cases (overlays, wgpu-based rendering, custom compositing).

// MARK: - PlaceholderRenderer

/// Placeholder for trait-based rendering (unused when using GStreamer display decoder).
pub struct PlaceholderRenderer;

#[async_trait]
impl Renderer for PlaceholderRenderer {
    async fn initialize(&mut self, _width: u32, _height: u32) -> Result<(), RendererError> {
        Err(RendererError::InitializationFailed("PlaceholderRenderer — use GStreamerDisplayDecoder instead".into()))
    }
    async fn present(&mut self, _frame: DecodedFrame) -> Result<(), RendererError> {
        Ok(())
    }
    async fn resize(&mut self, _width: u32, _height: u32) -> Result<(), RendererError> {
        Ok(())
    }
    async fn shutdown(&mut self) {}
}
