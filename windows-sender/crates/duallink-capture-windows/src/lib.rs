//! duallink-capture-windows — Windows.Graphics.Capture (WGC) implementation.
//!
//! Captures a display using the WGC API available on Windows 10 1803+.
//! On non-Windows targets a stub is compiled for CI compatibility.
//!
//! # Windows pipeline
//!
//! ```text
//! EnumDisplayMonitors → HMONITOR[display_index]
//!   │  IGraphicsCaptureItemInterop::CreateForMonitor
//!   ▼
//! GraphicsCaptureItem
//!   │  Direct3D11CaptureFramePool::CreateFreeThreaded (BGRA8, 2 buffers)
//!   ▼
//! GraphicsCaptureSession::StartCapture()
//!   │  FrameArrived callback
//!   ▼
//! ID3D11Texture2D (GPU) → CopyResource → staging texture → Map
//!   │
//!   ▼
//! Vec<u8> BGRA8 → tokio mpsc channel → ScreenCapturer::next_frame()
//! ```

/// Configuration for a single display capture stream.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    pub display_index: u8,
    pub width:  u32,
    pub height: u32,
    pub fps:    u32,
}

/// A raw captured video frame (BGRA8, CPU-side).
#[derive(Debug)]
pub struct CapturedFrame {
    pub data:   Vec<u8>,
    pub pts_ms: u64,
    pub width:  u32,
    pub height: u32,
}

// ── Platform split ─────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod wgc;
#[cfg(target_os = "windows")]
pub use wgc::ScreenCapturer;

#[cfg(not(target_os = "windows"))]
mod stub;
#[cfg(not(target_os = "windows"))]
pub use stub::ScreenCapturer;
