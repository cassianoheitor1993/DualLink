//! GStreamer H.264 encode pipeline for the Linux sender.
//!
//! # Encoder priority (highest to lowest)
//!
//! | Encoder       | Backend    | Notes |
//! |---------------|------------|-------|
//! | `vaapih264enc` | VA-API HW | Intel / AMD iGPU |
//! | `nvh264enc`   | NVENC HW   | NVIDIA GPU |
//! | `x264enc`     | Software   | CPU fallback, always available |
//!
//! # Pipeline
//!
//! ```text
//! appsrc (BGRx)
//!   → videoconvert
//!   → video/x-raw,format=I420   (intermediate conversion)
//!   → <best-encoder>
//!   → video/x-h264,stream-format=byte-stream,alignment=au
//!   → h264parse
//!   → appsink (H.264 AU byte-stream)
//! ```

use anyhow::Context;
use bytes::Bytes;
use duallink_capture_linux::CapturedFrame;
use duallink_core::{EncodedFrame, VideoCodec};
use gstreamer::prelude::*;
use gstreamer_app::{AppSink, AppSinkCallbacks, AppSrc, AppSrcCallbacks};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

// ── Encoder selection ─────────────────────────────────────────────────────────

/// Return the GStreamer element name of the best available H.264 encoder,
/// plus a GStreamer property string to insert after the element name.
fn select_encoder() -> (&'static str, &'static str) {
    let candidates: &[(&str, &str)] = &[
        ("vaapih264enc",  "rate-control=cbr quality-level=6"),
        ("nvh264enc",     "preset=low-latency-hq rc-mode=cbr"),
        ("x264enc",       "tune=zerolatency speed-preset=veryfast key-int-max=30"),
    ];
    for (name, props) in candidates {
        if gstreamer::ElementFactory::find(name).is_some() {
            info!("H.264 encoder selected: {}", name);
            return (name, props);
        }
    }
    // x264enc should always be available if gst-plugins-ugly is installed.
    warn!("No preferred H.264 encoder found; falling back to x264enc");
    ("x264enc", "tune=zerolatency")
}

// ── GstEncoder ────────────────────────────────────────────────────────────────

/// Encodes raw BGRx frames to H.264 using GStreamer.
///
/// Push frames with [`GstEncoder::push_frame`] and pull encoded output with
/// [`GstEncoder::next_encoded`].
pub struct GstEncoder {
    appsrc:     AppSrc,
    encoded_rx: mpsc::Receiver<EncodedFrame>,
    _pipeline:  gstreamer::Pipeline,
}

impl GstEncoder {
    /// Create and start a GStreamer encode pipeline.
    ///
    /// Must be called after `gstreamer::init()`.
    pub fn new(
        width: u32,
        height: u32,
        fps: u32,
        bitrate_kbps: u32,
    ) -> anyhow::Result<Self> {
        let (enc_name, enc_props) = select_encoder();

        let desc = format!(
            "appsrc name=src is-live=true format=time \
                 caps=\"video/x-raw,format=BGRx,width={width},height={height},\
                        framerate={fps}/1,colorimetry=bt709\" \
             ! videoconvert \
             ! {enc_name} {enc_props} bitrate={bitrate_kbps} \
             ! video/x-h264,stream-format=byte-stream,alignment=au \
             ! h264parse \
             ! appsink name=sink max-buffers=4 drop=false sync=false emit-signals=false"
        );
        debug!("Encoder pipeline: {}", desc);

        let pipeline = gstreamer::parse::launch(&desc)
            .context("Parsing encoder pipeline")?
            .downcast::<gstreamer::Pipeline>()
            .map_err(|_| anyhow::anyhow!("Expected a Pipeline"))?;

        let appsrc: AppSrc = pipeline
            .by_name("src")
            .context("Finding appsrc 'src'")?
            .downcast::<AppSrc>()
            .map_err(|_| anyhow::anyhow!("Expected AppSrc"))?;

        let appsink: AppSink = pipeline
            .by_name("sink")
            .context("Finding appsink 'sink'")?
            .downcast::<AppSink>()
            .map_err(|_| anyhow::anyhow!("Expected AppSink"))?;

        let (encoded_tx, encoded_rx) = mpsc::channel::<EncodedFrame>(16);

        appsink.set_callbacks(
            AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gstreamer::FlowError::Eos)?;
                    let buffer = sample.buffer().ok_or(gstreamer::FlowError::Error)?;

                    let pts_us = buffer
                        .pts()
                        .map(|t| t.useconds())
                        .unwrap_or(0);
                    let is_keyframe = !buffer
                        .flags()
                        .contains(gstreamer::BufferFlags::DELTA_UNIT);

                    let map = buffer
                        .map_readable()
                        .map_err(|_| gstreamer::FlowError::Error)?;
                    let data = Bytes::copy_from_slice(map.as_slice());

                    let frame = EncodedFrame {
                        data,
                        timestamp_us: pts_us,
                        is_keyframe,
                        codec: VideoCodec::H264,
                    };

                    if encoded_tx.blocking_send(frame).is_err() {
                        return Err(gstreamer::FlowError::Flushing);
                    }
                    Ok(gstreamer::FlowSuccess::Ok)
                })
                .build(),
        );

        pipeline
            .set_state(gstreamer::State::Playing)
            .context("Starting encoder pipeline")?;

        Ok(Self { appsrc, encoded_rx, _pipeline: pipeline })
    }

    /// Push a BGRx raw frame into the encode pipeline.
    ///
    /// Non-blocking — returns `Err` only if the pipeline has terminated.
    pub fn push_frame(&self, frame: CapturedFrame) -> anyhow::Result<()> {
        let mut buf = gstreamer::Buffer::with_size(frame.data.len())
            .context("Allocating GStreamer buffer")?;
        {
            let buf_mut = buf.get_mut().unwrap();
            buf_mut.set_pts(gstreamer::ClockTime::from_mseconds(frame.pts_ms));
            let mut map = buf_mut
                .map_writable()
                .map_err(|_| anyhow::anyhow!("Failed to map buffer"))?;
            map.copy_from_slice(&frame.data);
        }

        self.appsrc
            .push_buffer(buf)
            .map_err(|e| anyhow::anyhow!("appsrc push_buffer: {:?}", e))?;

        Ok(())
    }

    /// Await the next encoded H.264 access unit.
    ///
    /// Returns `None` when the pipeline ends.
    pub async fn next_encoded(&mut self) -> Option<EncodedFrame> {
        self.encoded_rx.recv().await
    }

    /// Send EOS to the pipeline and wait for it to drain.
    pub fn send_eos(&self) {
        let _ = self.appsrc.end_of_stream();
    }
}
