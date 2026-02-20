use serde::{Deserialize, Serialize};

// MARK: - Resolution

/// Resolução de display suportada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl Resolution {
    pub const FHD: Self = Self { width: 1920, height: 1080 };
    pub const QHD: Self = Self { width: 2560, height: 1440 };
    pub const UHD: Self = Self { width: 3840, height: 2160 };

    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn aspect_ratio(&self) -> f64 {
        self.width as f64 / self.height as f64
    }

    pub fn total_pixels(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

impl std::fmt::Display for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}×{}", self.width, self.height)
    }
}

// MARK: - ConnectionMode

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionMode {
    Wifi,
    Usb,
}

impl std::fmt::Display for ConnectionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wifi => write!(f, "Wi-Fi"),
            Self::Usb => write!(f, "USB"),
        }
    }
}

// MARK: - VideoCodec

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    H264,
    H265,
}

// MARK: - PeerInfo

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: String,
    pub name: String,
    pub address: String,
    pub port: u16,
}

impl PeerInfo {
    pub fn new(id: impl Into<String>, name: impl Into<String>, address: impl Into<String>, port: u16) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            address: address.into(),
            port,
        }
    }

    pub fn socket_addr(&self) -> String {
        format!("{}:{}", self.address, self.port)
    }
}

// MARK: - SessionInfo

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub peer: PeerInfo,
    pub config: crate::StreamConfig,
    pub connection_mode: ConnectionMode,
}

// MARK: - ConnectionState

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Idle,
    Discovering,
    Connecting { peer: PeerInfo, attempt: u32 },
    Streaming { session: SessionInfo },
    Reconnecting { peer: PeerInfo, attempt: u32 },
    Error { reason: String },
}

impl ConnectionState {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Streaming { .. })
    }
}

impl PartialEq for SessionInfo {
    fn eq(&self, other: &Self) -> bool {
        self.session_id == other.session_id
    }
}

// MARK: - DecodedFrame

/// Frame de vídeo decodificado pronto para rendering.
pub struct DecodedFrame {
    /// Dados do frame (formato depende do decoder — tipicamente NV12/RGBA).
    pub data: bytes::Bytes,
    pub width: u32,
    pub height: u32,
    pub timestamp_us: u64,
    pub format: PixelFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Nv12,
    Rgba,
    Bgra,
}

// MARK: - EncodedFrame

/// Frame H.264/H.265 encodado recebido via WebRTC/USB.
#[derive(Debug, Clone)]
pub struct EncodedFrame {
    pub data: bytes::Bytes,
    pub timestamp_us: u64,
    pub is_keyframe: bool,
    pub codec: VideoCodec,
}
