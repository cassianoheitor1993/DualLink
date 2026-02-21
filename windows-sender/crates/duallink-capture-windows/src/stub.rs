//! Non-Windows stub for ScreenCapturer (CI + cross-compilation).

use anyhow::Result;
use super::{CaptureConfig, CapturedFrame};

#[allow(dead_code)]
pub struct ScreenCapturer {
    config: CaptureConfig,
}

impl ScreenCapturer {
    pub async fn open(config: CaptureConfig) -> Result<Self> {
        tracing::info!(
            "ScreenCapturer::open stub (non-Windows) display={} {}x{} @{}fps",
            config.display_index, config.width, config.height, config.fps
        );
        Ok(Self { config })
    }

    pub async fn next_frame(&mut self) -> Option<CapturedFrame> {
        // Stub — block forever so the capture loop stays alive without burning CPU
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        None
    }
}
