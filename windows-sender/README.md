# DualLink Windows Sender

Turns a Windows machine into a DualLink **sender** — shares its screen wirelessly
or via USB-C with any DualLink receiver (Linux, Windows, or macOS).

> **Phase 5F complete.**  WGC screen capture, GStreamer H.264 encode, egui settings
> UI with mDNS auto-discovery, and SendInput-based input injection are all implemented.
> Virtual display via IddCx/parsec-vdd is planned for Phase 5G.

---

## Architecture

```
Windows.Graphics.Capture  →  GStreamer H.264 encode  →  UDP:7878+2n  →  Receiver
(WGC, per-monitor)            (mfh264enc / nvh264enc    (DLNK frames)
                               / x264enc fallback)
                               TCP:7879+2n TLS signaling
                               ↑
               mDNS discovery (_duallink._tcp.local.)
               egui settings UI
               SendInput injection (receiver → local mouse/keyboard)
```

---

## Modes

| Mode | Command | Notes |
|------|---------|-------|
| **GUI** (default) | `.\duallink-sender.exe` | Launches egui settings window |
| **Headless** | `DUALLINK_NO_UI=1 .\duallink-sender.exe` | Env-var configured, no window |

---

## Prerequisites

### 1. Rust toolchain (MSVC target)

```powershell
winget install Rustlang.Rustup
rustup target add x86_64-pc-windows-msvc
```

### 2. GStreamer runtime + development files

Download the **MSVC** builds from:
<https://gstreamer.freedesktop.org/download/>

Install in order:
1. `gstreamer-1.0-msvc-x86_64-<ver>.msi`
2. `gstreamer-1.0-devel-msvc-x86_64-<ver>.msi`

Add environment variables:

```powershell
$env:PATH += ";C:\gstreamer\1.0\msvc_x86_64\bin"
$env:PKG_CONFIG_PATH = "C:\gstreamer\1.0\msvc_x86_64\lib\pkgconfig"
```

### 3. Visual Studio Build Tools

Required for Rust MSVC target and GStreamer native libs.
<https://visualstudio.microsoft.com/visual-cpp-build-tools/>

---

## Build

```powershell
cd windows-sender
$env:PKG_CONFIG_PATH = "C:\gstreamer\1.0\msvc_x86_64\lib\pkgconfig"
cargo build --release -p duallink-windows-sender
```

---

## Run

### GUI mode (default)

```powershell
.\target\release\duallink-sender.exe
```

The settings UI lets you:
- Browse auto-discovered receivers via mDNS (no manual IP entry needed)
- Enter receiver IP + pairing PIN manually as fallback
- Choose resolution, FPS, bitrate, and display index
- Start / stop the capture pipeline

### Headless mode

```powershell
$env:DUALLINK_NO_UI   = "1"
$env:DUALLINK_HOST    = "192.168.1.100"  # receiver LAN IP
$env:DUALLINK_PIN     = "123456"         # 6-digit PIN shown by receiver
$env:DUALLINK_DISPLAY = "0"              # zero-based display index
$env:DUALLINK_WIDTH   = "1920"
$env:DUALLINK_HEIGHT  = "1080"
$env:DUALLINK_FPS     = "60"
$env:DUALLINK_KBPS    = "8000"
.\target\release\duallink-sender.exe
```

---

## mDNS Discovery

The sender browses for `_duallink._tcp.local.` services on start.  Any running
DualLink receiver on the same subnet will appear automatically in the UI.  The
TXT record carries the receiver's LAN IP, port, display count, and a short TLS
fingerprint for TOFU verification.

---

## SendInput Injection

Mouse and keyboard events captured by the receiver's video window are forwarded
back to the Windows sender via the signaling TCP connection and replayed with
`SendInput`.  Virtual key codes are translated using a built-in VK map covering
all common keys and modifiers.

---

## GStreamer Encoder Priority

| Priority | Element | Requires |
|----------|---------|---------|
| 1 | `mfh264enc` | Media Foundation (Windows 8+ — built-in) |
| 2 | `nvh264enc` | NVIDIA GPU (NVENC + GStreamer NVCODEC plugin) |
| 3 | `x264enc` | Software fallback |

---

## Firewall

Allow outbound UDP/TCP for each display (default ports for display 0):

```powershell
netsh advfirewall firewall add rule name="DualLink Video 0"     protocol=UDP dir=out remoteport=7878 action=allow
netsh advfirewall firewall add rule name="DualLink Signaling 0" protocol=TCP dir=out remoteport=7879 action=allow
```

---

## Implementation Status

| Feature | Phase | Status |
|---------|-------|--------|
| WGC screen capture | 5E | ✅ |
| GStreamer H.264 encode (`mfh264enc` / `nvh264enc` / `x264enc`) | 5E | ✅ |
| `WinSenderPipeline` (capture → encode → UDP send) | 5E | ✅ |
| `Arc<Notify>` pipeline stop (clean shutdown) | 5F | ✅ |
| egui settings UI | 5E | ✅ |
| mDNS receiver discovery | 5E | ✅ |
| SendInput input injection (VK map) | 5F | ✅ |
| Virtual display via IddCx / parsec-vdd | 5G | 🔲 |
| Multi-display sender (N parallel pipelines) | 5G | 🔲 |
