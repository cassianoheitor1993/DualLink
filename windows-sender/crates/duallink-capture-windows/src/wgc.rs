//! Windows.Graphics.Capture (WGC) screen capture implementation.
//!
//! Requires Windows 10 1803+ (build 17134) and the `windows` crate with WGC features.
//!
//! # Threading model
//!
//! WGC `FrameArrived` callbacks arrive on a thread-pool thread.  We push frames
//! into a `tokio::sync::mpsc` channel and `next_frame()` awaits them.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use tokio::sync::mpsc;
use windows::{
    core::*,
    Foundation::TypedEventHandler,
    Graphics::{
        Capture::{
            Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
        },
        DirectX::DirectXPixelFormat,
        SizeInt32,
    },
    Win32::{
        Foundation::{BOOL, LPARAM},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::{
                D3D11CreateDevice, ID3D11Device, ID3D11Texture2D,
                D3D11_BIND_FLAG, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
            },
            Dxgi::IDXGIDevice,
            Gdi::{EnumDisplayMonitors, HMONITOR, HDC},
        },
        System::WinRT::{
            Direct3D11::CreateDirect3D11DeviceFromDXGIDevice,
            Graphics::Capture::IGraphicsCaptureItemInterop,
            RoInitialize, RO_INIT_MULTITHREADED,
        },
    },
};

use super::{CaptureConfig, CapturedFrame};

// ── ScreenCapturer ─────────────────────────────────────────────────────────────

pub struct ScreenCapturer {
    config:    CaptureConfig,
    frame_rx:  mpsc::Receiver<CapturedFrame>,
    // Keep alive: session + pool are dropped when capturer is dropped
    _session:  GraphicsCaptureSession,
    _pool:     Direct3D11CaptureFramePool,
}

impl ScreenCapturer {
    /// Open a WGC capture session for the given display.
    pub async fn open(config: CaptureConfig) -> Result<Self> {
        // Initialise WinRT on this thread (no-op if already done)
        unsafe { let _ = RoInitialize(RO_INIT_MULTITHREADED); }

        let display_index = config.display_index as usize;

        // ── 1. Enumerate monitors ─────────────────────────────────────────
        let monitors = enumerate_monitors();
        if display_index >= monitors.len() {
            anyhow::bail!(
                "Display[{}] not found ({} monitors detected)",
                display_index,
                monitors.len()
            );
        }
        let hmonitor = monitors[display_index];
        tracing::info!(
            "Display[{}] WGC capturing HMONITOR {:?}",
            display_index, hmonitor
        );

        // ── 2. Create D3D11 device ─────────────────────────────────────────
        let mut d3d_device: Option<ID3D11Device> = None;
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d_device),
                None,
                None,
            )
            .context("D3D11CreateDevice")?;
        }
        let d3d_device = d3d_device.unwrap();

        // ── 3. Wrap D3D11 device as WinRT IDirect3DDevice ─────────────────
        let dxgi_device: IDXGIDevice = d3d_device.cast().context("cast IDXGIDevice")?;
        let winrt_device = unsafe {
            CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device)
                .context("CreateDirect3D11DeviceFromDXGIDevice")?
        };

        // ── 4. Create GraphicsCaptureItem from HMONITOR ───────────────────
        let interop: IGraphicsCaptureItemInterop =
            windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
                .context("IGraphicsCaptureItemInterop factory")?;
        let item: GraphicsCaptureItem =
            unsafe { interop.CreateForMonitor(hmonitor).context("CreateForMonitor")? };
        let item_size: SizeInt32 = item.Size().context("GraphicsCaptureItem::Size")?;

        tracing::info!(
            "Display[{}] WGC item size {}x{}",
            display_index, item_size.Width, item_size.Height
        );

        // ── 5. Create frame pool ──────────────────────────────────────────
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &winrt_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2, // buffer count
            item_size,
        )
        .context("CreateFreeThreaded frame pool")?;

        // ── 6. Create capture session ─────────────────────────────────────
        let session = pool.CreateCaptureSession(&item).context("CreateCaptureSession")?;
        // Disable the yellow capture border (Windows 11 22H2+; OK to ignore error)
        let _ = session.SetIsBorderRequired(false);

        // ── 7. Register FrameArrived callback ─────────────────────────────
        let (frame_tx, frame_rx) = mpsc::channel::<CapturedFrame>(8);
        let d3d_clone = d3d_device.clone();
        let w = item_size.Width as u32;
        let h = item_size.Height as u32;
        let pool_clone = pool.clone();

        pool.FrameArrived(&TypedEventHandler::new(
            move |pool_ref: &Option<Direct3D11CaptureFramePool>, _| {
                let pool_ref = match pool_ref {
                    Some(p) => p,
                    None => return Ok(()),
                };
                let frame = match pool_ref.TryGetNextFrame() {
                    Ok(f) => f,
                    Err(_) => return Ok(()),
                };
                let surface = frame.Surface()?;
                let texture: ID3D11Texture2D = surface.cast::<ID3D11Texture2D>()?;

                // Create a staging texture for CPU readback
                let staging = create_staging_texture(&d3d_clone, w, h)?;
                let mut ctx: Option<windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext> =
                    None;
                unsafe { d3d_clone.GetImmediateContext(&mut ctx) };
                let ctx = ctx.unwrap();
                unsafe { ctx.CopyResource(&staging, &texture) };

                // Map and copy pixels
                let mapped = unsafe {
                    ctx.Map(&staging, 0,
                        windows::Win32::Graphics::Direct3D11::D3D11_MAP_READ, 0)?
                };
                let row_pitch = mapped.RowPitch as usize;
                let mut data = Vec::with_capacity(w as usize * h as usize * 4);
                for row in 0..h as usize {
                    let row_start = row * row_pitch;
                    let src = unsafe {
                        std::slice::from_raw_parts(
                            (mapped.pData as *const u8).add(row_start),
                            w as usize * 4,
                        )
                    };
                    data.extend_from_slice(src);
                }
                unsafe { ctx.Unmap(&staging, 0) };

                let pts_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                let _ = frame_tx.try_send(CapturedFrame { data, pts_ms, width: w, height: h });
                Ok(())
            },
        ))
        .context("FrameArrived handler")?;

        // ── 8. Start capture ──────────────────────────────────────────────
        session.StartCapture().context("StartCapture")?;
        tracing::info!("Display[{}] WGC capture started", display_index);

        Ok(Self {
            config,
            frame_rx,
            _session: session,
            _pool: pool,
        })
    }

    /// Await the next captured frame (blocks until a frame arrives).
    pub async fn next_frame(&mut self) -> Option<CapturedFrame> {
        self.frame_rx.recv().await
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Create a CPU-readable staging texture matching (w×h, BGRA8).
fn create_staging_texture(device: &ID3D11Device, w: u32, h: u32) -> Result<ID3D11Texture2D> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width:     w,
        Height:    h,
        MipLevels: 1,
        ArraySize: 1,
        Format:    windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC {
            Count: 1, Quality: 0
        },
        Usage:     D3D11_USAGE_STAGING,
        BindFlags: D3D11_BIND_FLAG(0),
        CPUAccessFlags: D3D11_CPU_ACCESS_READ,
        MiscFlags: windows::Win32::Graphics::Direct3D11::D3D11_RESOURCE_MISC_FLAG(0),
    };
    let mut tex: Option<ID3D11Texture2D> = None;
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut tex))? };
    tex.context("CreateTexture2D staging")
}

/// Enumerate connected monitors, in the order Windows reports them.
fn enumerate_monitors() -> Vec<HMONITOR> {
    let mut list: Vec<HMONITOR> = Vec::new();

    unsafe extern "system" fn cb(
        hmon: HMONITOR,
        _: HDC,
        _: *mut windows::Win32::Foundation::RECT,
        data: LPARAM,
    ) -> BOOL {
        let list = data.0 as *mut Vec<HMONITOR>;
        unsafe { (*list).push(hmon) };
        BOOL(1)
    }

    unsafe {
        let _ = EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(cb),
            LPARAM(&mut list as *mut _ as isize),
        );
    }
    list
}
