use serde::{Deserialize, Serialize};
use crate::types::{Resolution, VideoCodec};

/// Configuração de stream de vídeo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamConfig {
    pub resolution: Resolution,
    #[serde(alias = "targetFPS")]
    pub target_fps: u32,
    #[serde(alias = "maxBitrateBps")]
    pub max_bitrate_bps: u64,
    pub codec: VideoCodec,
    #[serde(alias = "lowLatencyMode")]
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

#[cfg(test)]
mod tests {
    use super::StreamConfig;

    #[test]
    fn deserializes_camel_case_fields() {
        let json = r#"{
            "resolution": {"width": 1920, "height": 1080},
            "targetFPS": 60,
            "maxBitrateBps": 12000000,
            "codec": "h264",
            "lowLatencyMode": true
        }"#;

        let cfg: StreamConfig = serde_json::from_str(json).expect("valid camelCase config");
        assert_eq!(cfg.target_fps, 60);
        assert_eq!(cfg.max_bitrate_bps, 12_000_000);
        assert!(cfg.low_latency_mode);
    }

    #[test]
    fn deserializes_snake_case_fields() {
        let json = r#"{
            "resolution": {"width": 1920, "height": 1080},
            "target_fps": 30,
            "max_bitrate_bps": 8000000,
            "codec": "h264",
            "low_latency_mode": false
        }"#;

        let cfg: StreamConfig = serde_json::from_str(json).expect("valid snake_case config");
        assert_eq!(cfg.target_fps, 30);
        assert_eq!(cfg.max_bitrate_bps, 8_000_000);
        assert!(!cfg.low_latency_mode);
    }
}
