//! duallink-capture-linux — Screen capture for DualLink Linux sender.
//!
//! # Capture backends
//!
//! | Backend | Protocol | Status |
//! |---------|---------|--------|
//! | PipeWire (ashpd + GStreamer) | Wayland + X11 via portal | Phase 5C ✓ |
//! | X11 XShm | X11 only | Planned Phase 6 |
//!
//! # Usage
//!
//! ```rust,no_run
//! # async fn example() -> anyhow::Result<()> {
//! use duallink_capture_linux::{CaptureConfig, ScreenCapturer};
//! let cfg = CaptureConfig { display_index: 0, width: 1920, height: 1080, fps: 60 };
//! let mut capturer = ScreenCapturer::open(cfg).await?;
//! while let Some(frame) = capturer.next_frame().await {
//!     // frame.data: Vec<u8> BGRx raw pixels (4 bytes/px, X byte unused)
//!     // frame.pts_ms: presentation timestamp (ms)
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Architecture
//!
//! ```text
//! ashpd portal ──► PipeWire node_id + remote_fd
//!                          │
//!                          ▼
//!            pipewiresrc(fd=X, path=Y)
//!                          │
//!                    videoconvert
//!                          │
//!               video/x-raw,format=BGRx
//!                          │
//!                       appsink  ─────► tokio channel ──► next_frame()
//! ```

#![allow(unused_variables, dead_code)]

use anyhow::Result;
use tracing::warn;

// ── Public types ──────────────────────────────────────────────────────────────

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

impl Default for CaptureConfig {
    fn default() -> Self {
        Self { display_index: 0, width: 1920, height: 1080, fps: 60 }
    }
}

/// A raw captured video frame.
#[derive(Debug)]
pub struct CapturedFrame {
    /// Pixel data — BGRx (4 bytes per pixel, X byte unused on Linux).
    pub data:   Vec<u8>,
    /// Presentation timestamp in milliseconds.
    pub pts_ms: u64,
    /// Pixel format.
    pub format: PixelFormat,
    /// Frame width in pixels.
    pub width:  u32,
    /// Frame height in pixels.
    pub height: u32,
}

/// Pixel format of a captured frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// 4 bytes per pixel: Blue, Green, Red, unused.
    Bgrx,
    /// Planar YUV 4:2:0.
    Nv12,
}

// ── ScreenCapturer ────────────────────────────────────────────────────────────

/// Screen capturer handle.  Open with [`ScreenCapturer::open`].
pub struct ScreenCapturer {
    config: CaptureConfig,
    #[cfg(target_os = "linux")]
    inner: linux::LinuxCapturer,
}

impl ScreenCapturer {
    /// Open a PipeWire screen-capture session.
    ///
    /// On Wayland this shows an XDG portal permission dialog.
    /// Requires `xdg-desktop-portal` + a backend (`-wlr`, `-gnome`, `-kde`) running.
    pub async fn open(config: CaptureConfig) -> Result<Self> {
        #[cfg(target_os = "linux")]
        {
            let inner = linux::LinuxCapturer::open(config.clone()).await?;
            return Ok(Self { config, inner });
        }
        #[cfg(not(target_os = "linux"))]
        {
            warn!("ScreenCapturer::open — non-Linux platform, stub capturer");
            Ok(Self { config })
        }
    }

    /// Await the next captured frame.  Returns `None` when the session ends.
    pub async fn next_frame(&mut self) -> Option<CapturedFrame> {
        #[cfg(target_os = "linux")]
        return self.inner.next_frame().await;
        #[cfg(not(target_os = "linux"))]
        {
            warn!("ScreenCapturer::next_frame — stub, no frames produced");
            None
        }
    }

    /// Active configuration.
    pub fn config(&self) -> &CaptureConfig {
        &self.config
    }
}

// ── Linux implementation (PipeWire portal + GStreamer) ────────────────────────

#[cfg(target_os = "linux")]
mod linux {
    use super::{CaptureConfig, CapturedFrame, PixelFormat};

    use std::os::unix::io::IntoRawFd;

    use anyhow::Context;
    use ashpd::desktop::screencast::{CaptureType, Persist, ScreenCast, SourceType};
    use ashpd::WindowIdentifier;
    use gstreamer::prelude::*;
    use gstreamer_app::{AppSink, AppSinkCallbacks};
    use tokio::sync::mpsc;
    use tracing::{debug, info, error};

    // ── Public handle ─────────────────────────────────────────────────────────

    pub(super) struct LinuxCapturer {
        frame_rx:     mpsc::Receiver<CapturedFrame>,
        _pipeline:    gstreamer::Pipeline,
        _bus_watcher: tokio::task::JoinHandle<()>,
    }

    impl LinuxCapturer {
        pub(super) async fn open(config: CaptureConfig) -> anyhow::Result<Self> {
            gstreamer::init().context("GStreamer init")?;

            let (node_id, fd_raw) = negotiate_portal(&config).await?;
            info!(
                "PipeWire portal ok: node_id={} fd={} (display={})",
                node_id, fd_raw, config.display_index
            );

            let (pipeline, frame_rx) = build_pipeline(&config, fd_raw, node_id)?;
            pipeline
                .set_state(gstreamer::State::Playing)
                .context("GStreamer set Playing")?;

            // Watch the bus for errors / EOS in a background task.
            let pipeline_weak = pipeline.downgrade();
            let bus_watcher = tokio::spawn(async move {
                let Some(pl) = pipeline_weak.upgrade() else { return };
                let bus = pl.bus().expect("pipeline bus");
                loop {
                    match bus.timed_pop(gstreamer::ClockTime::from_seconds(1)) {
                        Some(msg) => match msg.view() {
                            gstreamer::MessageView::Eos(_) => {
                                info!("GStreamer pipeline EOS");
                                break;
                            }
                            gstreamer::MessageView::Error(e) => {
                                error!("GStreamer error: {}", e.error());
                                break;
                            }
                            _ => {}
                        },
                        None => {} // poll timeout — keep looping
                    }
                }
                let _ = pl.set_state(gstreamer::State::Null);
            });

            Ok(Self { frame_rx, _pipeline: pipeline, _bus_watcher: bus_watcher })
        }

        pub(super) async fn next_frame(&mut self) -> Option<CapturedFrame> {
            self.frame_rx.recv().await
        }
    }

    // ── Portal negotiation ────────────────────────────────────────────────────

    /// Ask the XDG desktop portal for a PipeWire screen-cast stream.
    /// Returns `(node_id, raw_fd)`.
    async fn negotiate_portal(config: &CaptureConfig) -> anyhow::Result<(u32, i32)> {
        let proxy = ScreenCast::new().await.context("ScreenCast portal")?;

        let session = proxy
            .create_session()
            .await
            .context("create_session")?;

        proxy
            .select_sources(
                &session,
                CaptureType::SCREEN,
                SourceType::MONITOR,
                false,          // multiple
                None,           // cursor_mode
                Persist::DoNot,
            )
            .await
            .context("select_sources")?;

        let response = proxy
            .start(&session, &WindowIdentifier::default())
            .await
            .context("portal start")?
            .response()
            .context("portal denied")?;

        let streams: Vec<_> = response.streams().to_vec();
        if streams.is_empty() {
            anyhow::bail!("No PipeWire streams returned by portal");
        }

        let idx = config.display_index as usize;
        let stream = streams.get(idx).unwrap_or(&streams[0]);
        let node_id = stream.pipe_wire_node_id();

        let fd = proxy
            .open_pipe_wire_remote(&session)
            .await
            .context("open_pipe_wire_remote")?;
        let fd_raw = fd.into_raw_fd();

        Ok((node_id, fd_raw))
    }

    // ── GStreamer pipeline ────────────────────────────────────────────────────

    fn build_pipeline(
        config: &CaptureConfig,
        fd: i32,
        node_id: u32,
    ) -> anyhow::Result<(gstreamer::Pipeline, mpsc::Receiver<CapturedFrame>)> {
        let w   = config.width;
        let h   = config.height;
        let fps = config.fps;

        let desc = format!(
            "pipewiresrc fd={fd} path={node_id} do-timestamp=true \
             ! videoconvert \
             ! video/x-raw,format=BGRx,width={w},height={h},framerate={fps}/1 \
             ! appsink name=sink max-buffers=2 drop=true sync=false emit-signals=false"
        );
        debug!("GStreamer pipeline: {}", desc);

        let pipeline = gstreamer::parse::launch(&desc)
            .context("Parsing GStreamer pipeline")?
            .downcast::<gstreamer::Pipeline>()
            .map_err(|_| anyhow::anyhow!("Expected Pipeline element"))?;

        let appsink: AppSink = pipeline
            .by_name("sink")
            .context("Finding appsink 'sink'")?
            .downcast::<AppSink>()
            .map_err(|_| anyhow::anyhow!("Expected AppSink"))?;

        let (frame_tx, frame_rx) = mpsc::channel::<CapturedFrame>(8);

        appsink.set_callbacks(
            AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gstreamer::FlowError::Eos)?;
                    let buffer = sample.buffer().ok_or(gstreamer::FlowError::Error)?;
                    let pts_ms = buffer.pts().map(|t| t.mseconds()).unwrap_or(0);
                    let map    = buffer.map_readable().map_err(|_| gstreamer::FlowError::Error)?;
                    let data   = map.as_slice().to_vec();

                    let frame  = CapturedFrame {
                        data,
                        pts_ms,
                        format: PixelFormat::Bgrx,
                        width:  w,
                        height: h,
                    };

                    if frame_tx.blocking_send(frame).is_err() {
                        return Err(gstreamer::FlowError::Flushing);
                    }
                    Ok(gstreamer::FlowSuccess::Ok)
                })
                .build(),
        );

        Ok((pipeline, frame_rx))
    }
}
