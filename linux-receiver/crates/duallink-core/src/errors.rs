use thiserror::Error;

#[derive(Error, Debug)]
pub enum DualLinkError {
    #[error("Not implemented yet: {feature}")]
    NotImplemented { feature: String },

    #[error("Configuration invalid: {reason}")]
    ConfigurationInvalid { reason: String },

    #[error("Permission denied: {permission}")]
    PermissionDenied { permission: String },

    #[error("Connection failed: {reason}")]
    ConnectionFailed { reason: String },

    #[error("Stream error: {reason}")]
    StreamError { reason: String },

    #[error("Decoder error: {0}")]
    Decoder(#[from] DecoderError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Error, Debug)]
pub enum DecoderError {
    #[error("Hardware decoder unavailable, no software fallback configured")]
    HardwareUnavailable,

    #[error("GStreamer pipeline error: {0}")]
    GStreamerPipeline(String),

    #[error("Failed to decode frame: {reason}")]
    DecodeFailed { reason: String },

    #[error("Decoder not initialized")]
    NotInitialized,
}

#[derive(Error, Debug)]
pub enum TransportError {
    #[error("Connection closed by peer")]
    ConnectionClosed,

    #[error("Send failed: {reason}")]
    SendFailed { reason: String },

    #[error("Receive failed: {reason}")]
    ReceiveFailed { reason: String },

    #[error("Timeout after {ms}ms")]
    Timeout { ms: u64 },
}
