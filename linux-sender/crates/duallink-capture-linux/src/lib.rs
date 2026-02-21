//! duallink-capture-linux — Screen capture for DualLink Linux sender.
//!
//! # Capture backends
//!
//! | Backend | Protocol | Status |
//! |---------|---------|--------|
//! | PipeWire (ashpd) | Wayland + X11 via portal | Phase 5B skeleton |
//! | X11 XShm | X11 only | Planned Phase 5C |
//!
//! # Usage (future)
//!
//! ```rust,no_run
//! # async fn example() -> anyhow::Result<()> {
//! use duallink_capture_linux::{CaptureConfig, ScreenCapturer};
//! let cfg = CaptureConfig { display_index: 0, width: 1920, height: 1080, fps: 60 };
//! let mut capturer = ScreenCapturer::open(cfg).await?;
//! while let Some(frame) = capturer.next_frame().await {
//!     // frame.data: Vec<u8> (BGRA or NV12 depending on backend)
//! }
//! # Ok(())
//! # }
//! ```

use anyhow::Result;

/// Configuration for a single display capture stream.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Zero-based display index (corresponds to DualLink display_index).
    pub display_index: u8,
    pub width:  u32,
    pub height: u32,
    /// Target capture frame rate.
    pub fps: u32,
}

/// A raw captured video frame.
#[derive(Debug)]
pub struct CapturedFrame {
    /// Pixel data — format depends on the backend (BGRA or NV12).
    pub data:   Vec<u8>,
    /// Presentation timestamp in milliseconds.
    pub pts_ms: u64,
    /// Frame format.
    pub format: PixelFormat,
}

/// Pixel format of a captured frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Bgra,
    Nv12,
}

/// Screen capturer handle.
///
/// TODO (Phase 5B): implement PipeWire capture via `ashpd::desktop::ScreenCast`.
pub struct ScreenCapturer {
    config: CaptureConfig,
}

impl ScreenCapturer {
    /// Open a screen capture session for the given configuration.
    ///
    /// Currently returns a stub that emits no frames — full implementation
    /// uses `ashpd::desktop::ScreenCast` portal for Wayland/PipeWire.
    pub async fn open(config: CaptureConfig) -> Result<Self> {
        tracing::info!(
            "ScreenCapturer::open display={} {}x{} @{}fps (stub — PipeWire not yet wired)",
            config.display_index, config.width, config.height, config.fps
        );
        // TODO Phase 5B:
        //   let session = ashpd::desktop::ScreenCast::new().await?;
        //   let streams = session.initiate_take_screenshot(...).await?;
        Ok(Self { config })
    }

    /// Poll for the next captured frame.
    ///
    /// Returns `None` when the capture session ends.
    ///
    /// TODO Phase 5B: wire to PipeWire stream; currently always returns None.
    pub async fn next_frame(&mut self) -> Option<CapturedFrame> {
        // Stub: real implementation blocks on the PipeWire fd
        tracing::warn!(
            "ScreenCapturer::next_frame stub — no frames will be produced (display={})",
            self.config.display_index
        );
        None
    }
}
