# DualLink Windows Sender

Turns a Windows machine into a DualLink **sender** — shares its screen wirelessly
or via USB-C with any DualLink receiver (Linux, Windows, or macOS).

> **Phase 5B skeleton** — screen capture pipeline not yet implemented.
> See the status table in `src/main.rs` for current progress.

---

## Architecture

```
Windows.Graphics.Capture  →  GStreamer H.264 encode  →  UDP:7878  →  Receiver
(WGC, hardware-accelerated)   (mfh264enc / nvh264enc)   (DLNK frames)
                               TCP:7879 TLS signaling
```

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

### 4. Virtual display driver (for Extend mode)

Install [parsec-vdd](https://github.com/nicehash/parsec-vdd) or an equivalent
IddCx virtual display driver to create headless virtual monitors.

---

## Build

```powershell
cd windows-sender
$env:PKG_CONFIG_PATH = "C:\gstreamer\1.0\msvc_x86_64\lib\pkgconfig"
cargo build --release -p duallink-windows-sender
```

---

## Run (when implemented)

```powershell
$env:DUALLINK_RECEIVER_IP = "192.168.1.100"
$env:DUALLINK_PAIRING_PIN = "123456"
.\target\release\duallink-sender.exe
```

---

## GStreamer encoder priority (Windows)

| Priority | Element | Requires |
|----------|---------|---------|
| 1 | `mfh264enc` | Media Foundation (Windows 8+ — built-in) |
| 2 | `nvh264enc` | NVIDIA GPU (NVENC + GStreamer NVCODEC plugin) |
| 3 | `x264enc` | Software fallback |

---

## Firewall (sender)

Allow outbound UDP/TCP on the display ports (usually open by default):

```powershell
# Display 0
netsh advfirewall firewall add rule name="DualLink Sender 0" protocol=UDP dir=out remoteport=7878 action=allow
netsh advfirewall firewall add rule name="DualLink Signaling 0" protocol=TCP dir=out remoteport=7879 action=allow
```

---

## Phase 5B TODO

- [ ] `GraphicsCaptureSession` + `FramePool` callback implementation
- [ ] D3D11 texture → CPU staging readback
- [ ] GStreamer encode: `appsrc → videoconvert → mfh264enc → appsink`
- [ ] TCP TLS signaling client (`hello` → pairing PIN → config handshake)
- [ ] UDP DLNK-framed packet sender
- [ ] egui settings UI (receiver IP, PIN, display count, resolution, codec, fps)
- [ ] Virtual display driver integration (parsec-vdd / IddCx)
- [ ] Multi-display sender (N parallel capture + encode + send pipelines)
