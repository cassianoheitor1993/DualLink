use serde::{Deserialize, Serialize};
use crate::types::{Resolution, VideoCodec};

/// Configuração de stream de vídeo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamConfig {
    pub resolution: Resolution,
    pub target_fps: u32,
    pub max_bitrate_bps: u64,
    pub codec: VideoCodec,
    pub low_latency_mode: bool,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            resolution: Resolution::FHD,
            target_fps: 30,
            max_bitrate_bps: 8_000_000,
            codec: VideoCodec::H264,
            low_latency_mode: true,
        }
    }
}

impl StreamConfig {
    /// Configuração de alta performance para 60fps.
    pub fn high_performance() -> Self {
        Self {
            resolution: Resolution::QHD,
            target_fps: 60,
            max_bitrate_bps: 20_000_000,
            codec: VideoCodec::H264,
            low_latency_mode: true,
        }
    }

    /// Retorna o intervalo entre frames em microsegundos.
    pub fn frame_interval_us(&self) -> u64 {
        1_000_000 / self.target_fps as u64
    }
}
