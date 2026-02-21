//! GStreamer H.264 encoder pipeline for the DualLink Windows sender.
//!
//! Encoder priority (first factory found wins):
//! 1. `mfh264enc`  — Windows Media Foundation (zero-license, hardware or software)
//! 2. `nvh264enc`  — NVIDIA NVENC (low latency, GPU)
//! 3. `x264enc`    — Software fallback
//!
//! Pipeline:
//! ```text
//! appsrc (BGRx)
//!   → videoconvert
//!   → video/x-raw,format=BGRx  (or NV12 for mfh264enc)
//!   → <encoder>
//!   → h264parse
//!   → appsink
//! ```

use anyhow::{Context, Result};
use duallink_capture_windows::CapturedFrame;
use duallink_core::EncodedFrame;
use gstreamer::{self as gst, prelude::*};
use gstreamer_app::{AppSink, AppSrc};

// ── Encoder selection ─────────────────────────────────────────────────────────

const ENCODER_CANDIDATES: &[&str] = &["mfh264enc", "nvh264enc", "x264enc"];

fn pick_encoder() -> &'static str {
    for name in ENCODER_CANDIDATES {
        if gst::ElementFactory::find(name).is_some() {
            tracing::info!("[GstEncoderWin] Using encoder: {}", name);
            return name;
        }
    }
    tracing::warn!("[GstEncoderWin] No hardware encoder found; defaulting to x264enc");
    "x264enc"
}

// ── GstEncoder ────────────────────────────────────────────────────────────────

/// GStreamer H.264 encode pipeline for the Windows sender.
pub struct GstEncoder {
    pipeline: gst::Pipeline,
    appsrc:   AppSrc,
    appsink:  AppSink,
    width:    u32,
    height:   u32,
    fps:      u32,
}

impl GstEncoder {
    /// Create and start a GStreamer encode pipeline.
    pub fn new(width: u32, height: u32, fps: u32, bitrate_kbps: u32) -> Result<Self> {
        let enc_name = pick_encoder();
        let bitrate_bps = bitrate_kbps * 1000;

        let pipeline_desc = if enc_name == "mfh264enc" {
            // mfh264enc accepts NV12 natively; convert from BGRx first
            format!(
                "appsrc name=src is-live=true format=time \
                 caps=video/x-raw,format=BGRx,width={width},height={height},framerate={fps}/1 \
                 ! videoconvert \
                 ! video/x-raw,format=NV12,width={width},height={height},framerate={fps}/1 \
                 ! mfh264enc bitrate={bitrate_kbps} quality-vs-speed=100 low-latency=true \
                 ! h264parse \
                 ! appsink name=sink sync=false emit-signals=true"
            )
        } else if enc_name == "nvh264enc" {
            format!(
                "appsrc name=src is-live=true format=time \
                 caps=video/x-raw,format=BGRx,width={width},height={height},framerate={fps}/1 \
                 ! videoconvert \
                 ! video/x-raw,format=NV12,width={width},height={height} \
                 ! nvh264enc bitrate={bitrate_bps} preset=low-latency-hq \
                 ! h264parse \
                 ! appsink name=sink sync=false emit-signals=true"
            )
        } else {
            // x264enc: software
            let x264_kbps = bitrate_kbps;
            format!(
                "appsrc name=src is-live=true format=time \
                 caps=video/x-raw,format=BGRx,width={width},height={height},framerate={fps}/1 \
                 ! videoconvert \
                 ! video/x-raw,format=I420,width={width},height={height} \
                 ! x264enc bitrate={x264_kbps} speed-preset=ultrafast \
                   tune=zerolatency key-int-max=60 \
                 ! h264parse \
                 ! appsink name=sink sync=false emit-signals=true"
            )
        };

        tracing::debug!("[GstEncoderWin] Pipeline: {}", pipeline_desc);

        let pipeline = gst::parse::launch(&pipeline_desc)
            .context("GStreamer pipeline parse")?
            .downcast::<gst::Pipeline>()
            .map_err(|_| anyhow::anyhow!("Pipeline downcast failed"))?;

        let appsrc = pipeline
            .by_name("src")
            .context("src element")?
            .downcast::<AppSrc>()
            .map_err(|_| anyhow::anyhow!("AppSrc downcast"))?;

        let appsink = pipeline
            .by_name("sink")
            .context("sink element")?
            .downcast::<AppSink>()
            .map_err(|_| anyhow::anyhow!("AppSink downcast"))?;

        pipeline.set_state(gst::State::Playing).context("Pipeline → Playing")?;
        tracing::info!(
            "[GstEncoderWin] Pipeline running: {}×{} @{}fps {}kbps ({})",
            width, height, fps, bitrate_kbps, enc_name
        );

        Ok(Self { pipeline, appsrc, appsink, width, height, fps })
    }

    /// Push a raw captured frame into the GStreamer appsrc.
    pub fn push_frame(&mut self, frame: CapturedFrame) -> Result<()> {
        use gstreamer::buffer::Buffer;
        use gstreamer::ClockTime;

        let mut buf = Buffer::with_size(frame.data.len())
            .context("Buffer::with_size")?;
        {
            let buf_mut = buf.get_mut().unwrap();
            buf_mut.set_pts(ClockTime::from_mseconds(frame.pts_ms));
            let mut map = buf_mut.map_writable().context("buffer map")?;
            map.as_mut_slice().copy_from_slice(&frame.data);
        }
        self.appsrc
            .push_buffer(buf)
            .map_err(|e| anyhow::anyhow!("push_buffer: {e}"))?;
        Ok(())
    }

    /// Pull the next encoded frame from the GStreamer appsink (blocks briefly).
    pub fn next_encoded(&mut self) -> Option<EncodedFrame> {
        use gstreamer::BufferFlags;

        let sample = self.appsink.try_pull_sample(gst::ClockTime::from_mseconds(50))?;
        let buf = sample.buffer()?;
        let map = buf.map_readable().ok()?;
        let is_keyframe = !buf.flags().contains(BufferFlags::DELTA_UNIT);
        let pts_ms = buf.pts().map(|t| t.mseconds()).unwrap_or(0);
        Some(EncodedFrame {
            data: map.as_slice().to_vec(),
            is_keyframe,
            pts_ms,
            display_index: 0,
        })
    }

    /// Send EOS to flush remaining encoded frames.
    pub fn send_eos(&mut self) {
        let _ = self.appsrc.end_of_stream();
    }
}

impl Drop for GstEncoder {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}
