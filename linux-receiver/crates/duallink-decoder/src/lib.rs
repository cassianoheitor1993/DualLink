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
use duallink_core::{errors::DecoderError, DecodedFrame, EncodedFrame, InputEvent, MouseButton, PixelFormat};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::{AppSink, AppSrc};
use tracing::{info, debug, warn};

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

        // Mac sends Annex-B (start-code prefixed) with SPS/PPS on keyframes
        let src_caps = gst::Caps::builder("video/x-h264")
            .field("stream-format", "byte-stream")
            .field("alignment", "au")
            .build();
        appsrc.set_caps(Some(&src_caps));

        pipeline
            .set_state(gst::State::Playing)
            .map_err(|_| DecoderError::GStreamerPipeline("Failed to start pipeline".into()))?;

        info!("GStreamerDecoder({}) ready {}x{}", element, width, height);
        Ok(Self { pipeline, appsrc, appsink, element, width, height })
    }

    /// Push one encoded frame into the pipeline. Returns None while pipeline fills.
    pub fn decode_frame(&self, frame: EncodedFrame) -> Result<DecodedFrame, DecoderError> {
        // Allocate GStreamer buffer and copy NAL data
        let data_len = frame.data.len();
        let mut gst_buf = gst::Buffer::with_size(data_len)
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

        // Pull decoded sample (500ms timeout — decoder pipeline needs a few frames to fill)
        let sample = self.appsink
            .try_pull_sample(gst::ClockTime::from_mseconds(500))
            .ok_or_else(|| DecoderError::DecodeFailed { reason: format!("appsink timeout (pushed {} bytes)", data_len) })?;

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

// ── GStreamerDisplayDecoder ────────────────────────────────────────────────────

/// Combined decode + display pipeline — Sprint 2.1
///
/// Uses `autovideosink` instead of `appsink` so GStreamer handles window
/// creation and rendering directly.  Zero extra CPU copies compared to
/// pulling from `appsink` and re-pushing to a separate display pipeline.
///
/// # Pipeline
/// ```text
/// appsrc → h264parse → [decoder] → autovideosink sync=false
/// ```
///
/// **Must be called from `tokio::task::spawn_blocking`** — GStreamer
/// creates the window / event loop on this thread.
pub struct GStreamerDisplayDecoder {
    pipeline: gst::Pipeline,
    appsrc:   AppSrc,
    element:  &'static str,
    #[allow(dead_code)]
    width:    u32,
    #[allow(dead_code)]
    height:   u32,
    frame_count: std::sync::atomic::AtomicU64,
}

impl GStreamerDisplayDecoder {
    /// Build and start the decode+display pipeline.
    pub fn new(element: &'static str, width: u32, height: u32) -> Result<Self, DecoderError> {
        // VA-API decoders output surfaces with alignment-padded heights
        // (e.g. 1088 instead of 1080).  `videoconvert` can't map those
        // surfaces properly → "info->height <= meta->height" assertion.
        //
        // Fix: for vaapi decoders, use `vaapipostproc` which operates
        // natively on VA surfaces.  For software decoders, use plain
        // `videoconvert`.
        let is_vaapi = element.starts_with("vaapi");
        let postproc = if is_vaapi {
            "vaapipostproc".to_string()
        } else {
            "videoconvert ! videoscale".to_string()
        };

        let pipeline_str = format!(
            "appsrc name=src format=time is-live=true \
             ! h264parse \
             ! {element} \
             ! {postproc} \
             ! autovideosink name=videosink sync=false"
        );

        let pipeline = gst::parse::launch(&pipeline_str)
            .map_err(|e| DecoderError::GStreamerPipeline(e.to_string()))?
            .downcast::<gst::Pipeline>()
            .map_err(|_| DecoderError::GStreamerPipeline("Not a pipeline".into()))?;

        let appsrc = pipeline
            .by_name("src")
            .and_then(|el| el.downcast::<AppSrc>().ok())
            .ok_or_else(|| DecoderError::GStreamerPipeline("No appsrc".into()))?;

        // Mac sends Annex-B (start-code prefixed) with SPS/PPS on keyframes
        let src_caps = gst::Caps::builder("video/x-h264")
            .field("stream-format", "byte-stream")
            .field("alignment", "au")
            .build();
        appsrc.set_caps(Some(&src_caps));

        pipeline
            .set_state(gst::State::Playing)
            .map_err(|_| DecoderError::GStreamerPipeline("Failed to start display pipeline".into()))?;

        info!("GStreamerDisplayDecoder({}) ready {}×{} — fullscreen display via autovideosink", element, width, height);

        Ok(Self {
            pipeline,
            appsrc,
            element,
            width,
            height,
            frame_count: std::sync::atomic::AtomicU64::new(0),
        })
    }

    /// Push one encoded frame into the pipeline. GStreamer decodes and displays it.
    pub fn push_frame(&self, frame: EncodedFrame) -> Result<(), DecoderError> {
        let data_len = frame.data.len();
        let mut gst_buf = gst::Buffer::with_size(data_len)
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

        let n = self.frame_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if n == 1 {
            info!("First frame pushed to display pipeline ({} bytes)", data_len);
        }

        Ok(())
    }

    /// Number of frames pushed so far.
    pub fn frames_pushed(&self) -> u64 {
        self.frame_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Poll for input (navigation) events from the GStreamer display window.
    ///
    /// Returns all pending mouse/keyboard events since the last call.
    /// Call this regularly from the decode thread (e.g. after each `push_frame`).
    pub fn poll_input_events(&self) -> Vec<InputEvent> {
        let mut events = Vec::new();
        let bus = match self.pipeline.bus() {
            Some(b) => b,
            None => return events,
        };

        // Drain all pending messages
        while let Some(msg) = bus.pop() {
            if let gst::MessageView::Element(elem) = msg.view() {
                if let Some(s) = elem.structure() {
                    if let Some(ev) = self.parse_navigation_event(s) {
                        events.push(ev);
                    }
                }
            }
        }
        events
    }

    /// Parse a GStreamer navigation structure into an InputEvent.
    ///
    /// Navigation structures have:
    /// - `event` field: "mouse-move", "mouse-button-press", "mouse-button-release",
    ///   "mouse-scroll", "key-press", "key-release"
    /// - `pointer_x`, `pointer_y`: absolute pixel coords (f64)
    /// - `button`: mouse button number (1=left, 2=middle, 3=right)
    /// - `key`: keyval string for keyboard events
    /// - `delta_x`, `delta_y`: scroll deltas
    fn parse_navigation_event(&self, s: &gst::StructureRef) -> Option<InputEvent> {
        let event_type = s.get::<&str>("event").ok()?;
        let w = self.width as f64;
        let h = self.height as f64;

        match event_type {
            "mouse-move" => {
                let px = s.get::<f64>("pointer_x").ok()?;
                let py = s.get::<f64>("pointer_y").ok()?;
                Some(InputEvent::MouseMove {
                    x: (px / w).clamp(0.0, 1.0),
                    y: (py / h).clamp(0.0, 1.0),
                })
            }
            "mouse-button-press" => {
                let px = s.get::<f64>("pointer_x").ok()?;
                let py = s.get::<f64>("pointer_y").ok()?;
                let btn = s.get::<i32>("button").unwrap_or(1);
                Some(InputEvent::MouseDown {
                    x: (px / w).clamp(0.0, 1.0),
                    y: (py / h).clamp(0.0, 1.0),
                    button: gst_button_to_mouse_button(btn),
                })
            }
            "mouse-button-release" => {
                let px = s.get::<f64>("pointer_x").ok()?;
                let py = s.get::<f64>("pointer_y").ok()?;
                let btn = s.get::<i32>("button").unwrap_or(1);
                Some(InputEvent::MouseUp {
                    x: (px / w).clamp(0.0, 1.0),
                    y: (py / h).clamp(0.0, 1.0),
                    button: gst_button_to_mouse_button(btn),
                })
            }
            "mouse-scroll" => {
                let px = s.get::<f64>("pointer_x").ok()?;
                let py = s.get::<f64>("pointer_y").ok()?;
                let dx = s.get::<f64>("delta_x").unwrap_or(0.0);
                let dy = s.get::<f64>("delta_y").unwrap_or(0.0);
                Some(InputEvent::MouseScroll {
                    x: (px / w).clamp(0.0, 1.0),
                    y: (py / h).clamp(0.0, 1.0),
                    delta_x: dx,
                    delta_y: dy,
                })
            }
            "key-press" => {
                let key = s.get::<&str>("key").ok()?;
                let keyval = x11_keyval_from_name(key);
                debug!("Key press: '{}' keyval={}", key, keyval);
                Some(InputEvent::KeyDown {
                    keycode: keyval,
                    text: if key.len() == 1 { Some(key.to_string()) } else { None },
                })
            }
            "key-release" => {
                let key = s.get::<&str>("key").ok()?;
                let keyval = x11_keyval_from_name(key);
                Some(InputEvent::KeyUp { keycode: keyval })
            }
            _ => None,
        }
    }

    pub fn element_name(&self) -> &str { self.element }
    pub fn is_hardware_accelerated(&self) -> bool { self.element != "avdec_h264" }
}

/// Map GStreamer button number (1-based) to MouseButton.
fn gst_button_to_mouse_button(btn: i32) -> MouseButton {
    match btn {
        1 => MouseButton::Left,
        2 => MouseButton::Middle,
        3 => MouseButton::Right,
        _ => MouseButton::Left,
    }
}

/// Map GStreamer/X11 key name to a keyval.
/// GStreamer sends X11 key names (e.g. "a", "Return", "Shift_L", "space").
/// We pass the raw X11 keyval so the Mac side can map it.
fn x11_keyval_from_name(name: &str) -> u32 {
    // Common special keys — full mapping via xkbcommon if needed later
    match name {
        "Return" | "KP_Enter" => 0xff0d,
        "Escape" => 0xff1b,
        "Tab" => 0xff09,
        "BackSpace" => 0xff08,
        "Delete" => 0xffff,
        "space" => 0x0020,
        "Shift_L" => 0xffe1,
        "Shift_R" => 0xffe2,
        "Control_L" => 0xffe3,
        "Control_R" => 0xffe4,
        "Alt_L" => 0xffe9,
        "Alt_R" => 0xffea,
        "Super_L" => 0xffeb,
        "Super_R" => 0xffec,
        "Left" => 0xff51,
        "Up" => 0xff52,
        "Right" => 0xff53,
        "Down" => 0xff54,
        "Home" => 0xff50,
        "End" => 0xff57,
        "Page_Up" => 0xff55,
        "Page_Down" => 0xff56,
        "F1" => 0xffbe,
        "F2" => 0xffbf,
        "F3" => 0xffc0,
        "F4" => 0xffc1,
        "F5" => 0xffc2,
        "F6" => 0xffc3,
        "F7" => 0xffc4,
        "F8" => 0xffc5,
        "F9" => 0xffc6,
        "F10" => 0xffc7,
        "F11" => 0xffc8,
        "F12" => 0xffc9,
        "Caps_Lock" => 0xffe5,
        _ => {
            // For single-char keys, use the Unicode codepoint
            let mut chars = name.chars();
            if let Some(c) = chars.next() {
                if chars.next().is_none() {
                    return c as u32;
                }
            }
            // Unknown — pass name hash as fallback
            0
        }
    }
}

impl Drop for GStreamerDisplayDecoder {
    fn drop(&mut self) {
        info!("Shutting down display pipeline ({})", self.element);
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

// ── DecoderFactory ─────────────────────────────────────────────────────────────

pub struct DecoderFactory;

impl DecoderFactory {
    /// Probe and initialise the best available decoder for the given resolution.
    /// Returns a decoder that produces `DecodedFrame` via `decode_frame()`.
    pub fn best_available(width: u32, height: u32) -> Result<GStreamerDecoder, DecoderError> {
        gst::init().map_err(|e| DecoderError::GStreamerPipeline(e.to_string()))?;
        let element = probe_best_decoder().ok_or(DecoderError::HardwareUnavailable)?;
        GStreamerDecoder::new(element, width, height)
    }

    /// Probe and initialise a combined decode+display pipeline.
    /// Frames are decoded AND displayed directly via `autovideosink`.
    pub fn best_available_with_display(width: u32, height: u32) -> Result<GStreamerDisplayDecoder, DecoderError> {
        gst::init().map_err(|e| DecoderError::GStreamerPipeline(e.to_string()))?;
        let element = probe_best_decoder().ok_or(DecoderError::HardwareUnavailable)?;
        GStreamerDisplayDecoder::new(element, width, height)
    }
}
