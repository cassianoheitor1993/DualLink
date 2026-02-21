# DualLink Windows Receiver

Receives screen streams from a macOS DualLink sender and displays them on a Windows machine.
Uses the same Rust codebase as the Linux receiver — all platform-specific code is guarded
with `#[cfg(target_os = "...")]`.

---

## Prerequisites

### 1. Rust toolchain (MSVC target)

```powershell
winget install Rustlang.Rustup
rustup target add x86_64-pc-windows-msvc
```

### 2. GStreamer runtime + development files

Download the **MSVC** builds (not MinGW) from:
<https://gstreamer.freedesktop.org/download/>

Install in order:
1. `gstreamer-1.0-msvc-x86_64-<ver>.msi` — runtime
2. `gstreamer-1.0-devel-msvc-x86_64-<ver>.msi` — headers + pkg-config files

After installation, add GStreamer to `PATH` and point `PKG_CONFIG_PATH`:

```powershell
$env:PATH += ";C:\gstreamer\1.0\msvc_x86_64\bin"
$env:PKG_CONFIG_PATH = "C:\gstreamer\1.0\msvc_x86_64\lib\pkgconfig"
```

Add `pkg-config` if not already on the machine:

```powershell
winget install bloodrock.pkg-config
```

### 3. Visual Studio Build Tools (C/C++ compiler)

Required by GStreamer's native libs. Install from:
<https://visualstudio.microsoft.com/visual-cpp-build-tools/>

Select: **Desktop development with C++** workload.

---

## Build

```powershell
# From the linux-receiver/ directory (all crates are shared)
cd ..\linux-receiver
$env:PKG_CONFIG_PATH = "C:\gstreamer\1.0\msvc_x86_64\lib\pkgconfig"
cargo build --release -p duallink-app
```

The binary lands at `linux-receiver\target\release\duallink-app.exe`.

---

## Run

```powershell
# Single display (default)
.\duallink-app.exe

# Two displays
$env:DUALLINK_DISPLAY_COUNT = "2"
.\duallink-app.exe
```

The receiver prints a 6-digit **Pairing PIN** on startup.  Enter it in the macOS
DualLink sender together with this machine's IP address.

---

## GStreamer decoder pipeline (Windows)

Priority order (defined in `duallink-decoder/src/lib.rs`):

| Priority | Element | Requires |
|----------|---------|---------|
| 1 | `d3d11h264dec` | Windows 10 1703+ (D3D11 video acceleration) |
| 2 | `mfh264dec` | Media Foundation (built-in since Win8) |
| 3 | `nvh264dec` | NVIDIA GPU + GStreamer NVCODEC plugin |
| 4 | `avdec_h264` | Software fallback (always available) |

The receiver selects the first element that initialises successfully.

---

## Firewall

Allow inbound UDP/TCP on the display ports:

```powershell
# Display 0 (default)
netsh advfirewall firewall add rule name="DualLink Display 0" protocol=UDP dir=in localport=7878 action=allow
netsh advfirewall firewall add rule name="DualLink Signaling 0" protocol=TCP dir=in localport=7879 action=allow

# Display 1 (if DUALLINK_DISPLAY_COUNT=2)
netsh advfirewall firewall add rule name="DualLink Display 1" protocol=UDP dir=in localport=7880 action=allow
netsh advfirewall firewall add rule name="DualLink Signaling 1" protocol=TCP dir=in localport=7881 action=allow
```

---

## Known Limitations (Phase 5B)

- USB-C transport (CDC-NCM gadget) is Linux-only; Windows receiver uses Wi-Fi only.
- `d3d11h264dec` requires GStreamer built against the MSVC D3D11 plugin — verify
  by running `gst-inspect-1.0 d3d11h264dec`.
- H.265 / HEVC hardware decoding (`d3d11h265dec`) is planned for Phase 5C.
