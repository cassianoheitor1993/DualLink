//! duallink-decoder — Sprint 1.4
//!
//! H.264 hardware-accelerated decoding via GStreamer.
//!
//! # Decoder priority (GT-2001 — Legion 5 Pro probe results 2026-02-20)
//! 1. `vaapih264dec`  — AMD Radeon 680M VA-API  (5.1ms avg) ← PRIMARY
//! 2. `vaapidecodebin` — VA-API auto-select      (5.5ms avg)
//! 3. `nvh264dec`     — NVIDIA NVDEC             (6.0ms avg)
//! 4. `avdec_h264`    — Software libavcodec      (16.8ms avg) ← last resort
//!
//! # Pipeline
//! ```text
//! appsrc → h264parse → [decoder] → videoconvert → video/x-raw,format=BGRA → appsink
//! ```
//! `h264parse` converts VideoToolbox AVCC (length-prefixed) → AnnexB automatically.

use bytes::Bytes;
use duallink_core::{errors::DecoderError, DecodedFrame, EncodedFrame, PixelFormat};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::{AppSink, AppSrc};
use tracing::{info, warn};

/// Decoder candidates in priority order (GT-2001).
static DECODER_PRIORITY: &[(&str, &str)] = &[
    ("vaapih264dec",   "AMD/Intel VA-API H.264 (primary — GT-2001)"),
    ("vaapidecodebin", "VA-API auto-select"),
    ("nvh264dec",      "NVIDIA NVDEC H.264"),
    ("avdec_h264",     "Software libavcodec (last resort)"),
];

// ── Probe ─────────────────────────────────────────────────────────────────────

/// Returns the name of the highest-priority available GStreamer H.264 decoder.
pub fn probe_best_decoder() -> Option<&'static str> {
    if gst::init().is_err() { return None; }
    for (element, label) in DECODER_PRIORITY {
        if gst::ElementFactory::find(element).is_some() {
            info!("Selected decoder: {} ({})", element, label);
            return Some(element);
        }
        warn!("Decoder '{}' not found, trying next", element);
    }
    None
}

// ── GStreamerDecoder ───────────────────────────────────────────────────────────

/// Synchronous H.264 decoder backed by a GStreamer pipeline.
///
/// **Must be called from `tokio::task::spawn_blocking`** — GStreamer's
/// `try_pull_sample` is blocking.
pub struct GStreamerDecoder {
    pipeline: gst::Pipeline,
    appsrc:   AppSrc,
    appsink:  AppSink,
    element:  &'static str,
    width:    u32,
    height:   u32,
}

impl GStreamerDecoder {
    /// Build and start the pipeline. Requires `gst::init()` to have been called.
    pub fn new(element: &'static str, width: u32, height: u32) -> Result<Self, DecoderError> {
        let pipeline_str = format!(
            "appsrc name=src format=time is-live=true \
             ! h264parse \
             ! {element} \
             ! videoconvert \
             ! video/x-raw,format=BGRA,width={width},height={height} \
             ! appsink name=sink sync=false max-buffers=4 drop=true"
        );

        let pipeline = gst::parse::launch(&pipeline_str)
            .map_err(|e| DecoderError::GStreamerPipeline(e.to_string()))?
            .downcast::<gst::Pipeline>()
            .map_err(|_| DecoderError::GStreamerPipeline("Not a pipeline".into()))?;

        let appsrc = pipeline
            .by_name("src")
            .and_then(|element| element.downcast::<AppSrc>().ok())
            .ok_or_else(|| DecoderError::GStreamerPipeline("No appsrc".into()))?;

        let appsink = pipeline
            .by_name("sink")
            .and_then(|element| element.downcast::<AppSink>().ok())
            .ok_or_else(|| DecoderError::GStreamerPipeline("No appsink".into()))?;

        // Let h264parse auto-detect whether input is AVCC or AnnexB
        let src_caps = gst::Caps::builder("video/x-h264")
            .field("alignment", "au")
            .build();
        appsrc.set_caps(Some(&src_caps));

        pipeline
            .set_state(gst::State::Playing)
            .map_err(|_| DecoderError::GStreamerPipeline("Failed to start pipeline".into()))?;

        info!("GStreamerDecoder({}) ready {}x{}", element, width, height);
        Ok(Self { pipeline, appsrc, appsink, element, width, height })
    }

    /// Decode one encoded frame synchronously. Returns raw BGRA pixels.
    pub fn decode_frame(&self, frame: EncodedFrame) -> Result<DecodedFrame, DecoderError> {
        // Allocate GStreamer buffer and copy NAL data
        let mut gst_buf = gst::Buffer::with_size(frame.data.len())
            .map_err(|_| DecoderError::DecodeFailed { reason: "alloc failed".into() })?;
        {
            let br = gst_buf.get_mut().unwrap();
            br.set_pts(gst::ClockTime::from_useconds(frame.timestamp_us));
            let mut map = br.map_writable()
                .map_err(|_| DecoderError::DecodeFailed { reason: "map failed".into() })?;
            map.copy_from_slice(&frame.data);
        }

        self.appsrc.push_buffer(gst_buf)
            .map_err(|_| DecoderError::DecodeFailed { reason: "appsrc push failed".into() })?;

        // Pull decoded sample (100ms timeout)
        let sample = self.appsink
            .try_pull_sample(gst::ClockTime::from_mseconds(100))
            .ok_or_else(|| DecoderError::DecodeFailed { reason: "appsink timeout".into() })?;

        let buffer = sample.buffer_owned()
            .ok_or_else(|| DecoderError::DecodeFailed { reason: "no buffer in sample".into() })?;
        let map = buffer.map_readable()
            .map_err(|_| DecoderError::DecodeFailed { reason: "read map failed".into() })?;

        let pts = if let Some(timestamp) = buffer.pts() {
            timestamp.useconds()
        } else {
            frame.timestamp_us
        };
        let data = Bytes::copy_from_slice(map.as_slice());

        Ok(DecodedFrame { data, width: self.width, height: self.height, timestamp_us: pts, format: PixelFormat::Bgra })
    }

    pub fn element_name(&self) -> &str { self.element }
    pub fn is_hardware_accelerated(&self) -> bool { self.element != "avdec_h264" }
}

impl Drop for GStreamerDecoder {
    fn drop(&mut self) { let _ = self.pipeline.set_state(gst::State::Null); }
}

// ── DecoderFactory ─────────────────────────────────────────────────────────────

pub struct DecoderFactory;

impl DecoderFactory {
    /// Probe and initialise the best available decoder for the given resolution.
    pub fn best_available(width: u32, height: u32) -> Result<GStreamerDecoder, DecoderError> {
        gst::init().map_err(|e| DecoderError::GStreamerPipeline(e.to_string()))?;
        let element = probe_best_decoder().ok_or(DecoderError::HardwareUnavailable)?;
        GStreamerDecoder::new(element, width, height)
    }
}
