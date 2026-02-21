//! duallink-capture-windows — Screen capture for DualLink Windows sender.
//!
//! Uses `Windows.Graphics.Capture` (WGC) API available on Windows 10 1803+.
//! WGC supports both monitor capture and window capture via a system-provided
//! picker.
//!
//! # Planned architecture
//!
//! ```text
//! Windows.Graphics.Capture.GraphicsCaptureSession
//!   │  (IDirect3D11CaptureFramePool callback)
//!   ▼
//! ID3D11Texture2D (BGRA8, GPU)
//!   │  (CopyResource to staging texture + Map)
//!   ▼
//! Vec<u8>  (raw BGRA pixels pushed into GStreamer appsrc)
//!   │
//!   ▼
//! GStreamer encode pipeline (mfh264enc / nvh264enc / x264enc)
//! ```
//!
//! # Phase 5B status
//!
//! - [x] Dependency declarations (`windows` crate with WGC features)
//! - [ ] `GraphicsCaptureSession` setup and `FramePool` callback
//! - [ ] D3D11 texture → CPU staging readback
//! - [ ] Pixel format conversion (BGRA8 → NV12 for encoder)
//! - [ ] Virtual display integration (IddCx / parsec-vdd)

use anyhow::Result;

/// Configuration for a single display capture stream.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    pub display_index: u8,
    pub width:  u32,
    pub height: u32,
    pub fps:    u32,
}

/// A raw captured video frame (CPU-side copy of the D3D11 staging texture).
#[derive(Debug)]
pub struct CapturedFrame {
    pub data:   Vec<u8>,
    pub pts_ms: u64,
    pub width:  u32,
    pub height: u32,
}

/// Windows screen capturer.
///
/// TODO (Phase 5B): implement using `Windows.Graphics.Capture.GraphicsCaptureSession`.
pub struct ScreenCapturer {
    config: CaptureConfig,
}

impl ScreenCapturer {
    /// Open a WGC capture session.
    ///
    /// On non-Windows platforms this is a no-op stub for CI compatibility.
    pub async fn open(config: CaptureConfig) -> Result<Self> {
        tracing::info!(
            "ScreenCapturer::open display={} {}x{} @{}fps (WGC stub)",
            config.display_index, config.width, config.height, config.fps
        );
        // TODO Phase 5B (Windows):
        //   let item = GraphicsCaptureItem::TryCreateFromDisplayId(display_id)?;
        //   let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        //       &device, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2,
        //       SizeInt32 { Width: w, Height: h })?;
        //   let session = frame_pool.CreateCaptureSession(&item)?;
        //   session.StartCapture()?;
        Ok(Self { config })
    }

    /// Poll for the next captured frame.
    ///
    /// TODO Phase 5B: wire to WGC FramePool; currently always returns None.
    pub async fn next_frame(&mut self) -> Option<CapturedFrame> {
        tracing::warn!(
            "ScreenCapturer::next_frame stub — no frames (display={})",
            self.config.display_index
        );
        None
    }
}
