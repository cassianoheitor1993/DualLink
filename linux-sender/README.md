# DualLink Linux Sender

Turns a Linux machine into a DualLink **sender** — share its screen wirelessly
or via USB-C with any DualLink receiver (Linux, Windows, or macOS running the
DualLink receiver app).

> **Phase 5F complete.**  PipeWire capture, GStreamer H.264 encoder, TLS signaling
> client, UDP sender, egui settings UI, mDNS receiver discovery, and uinput
> input injection are all implemented.

---

## Architecture

```
PipeWire portal (ashpd)
  └─► pipewiresrc  →  videoconvert  →  appsink   [duallink-capture-linux]
                                           │
                              appsrc  →  vaapih264enc  →  appsink  [encoder.rs]
                                                              │
                          SignalingClient (TLS:7879+2n) ◄─── │
                          VideoSender    (UDP:7878+2n) ──────►  Receiver
                          ← InputEvent   (TLS back-channel)  ◄─ uinput injection
```

---

## Modes

| Mode | Command | Notes |
|------|---------|-------|
| **GUI** (default) | `./duallink-sender` | egui settings window with mDNS discovery |
| **Headless** | `DUALLINK_NO_UI=1 ./duallink-sender` | Env-var configured, no window |

---

## Prerequisites

### Rust toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### GStreamer (Ubuntu/Debian)

```bash
sudo apt-get install -y \
  gstreamer1.0-tools \
  gstreamer1.0-plugins-base \
  gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad \
  gstreamer1.0-plugins-ugly \
  gstreamer1.0-libav \
  gstreamer1.0-vaapi \
  libgstreamer1.0-dev \
  libgstreamer-plugins-base1.0-dev \
  libgstreamer-plugins-bad1.0-dev
```

### PipeWire (Wayland capture, Ubuntu 22.04+)

PipeWire is installed by default on Ubuntu 22.04+.  The `ashpd` portal API
requires a running PipeWire session and XDG portal.

```bash
# Verify portal is available
dbus-send --session --dest=org.freedesktop.portal.Desktop \
  /org/freedesktop/portal/desktop \
  org.freedesktop.DBus.Properties.GetAll \
  string:org.freedesktop.portal.ScreenCast
```

---

## Build

```bash
cd linux-sender
cargo build --release -p duallink-linux-sender
```

---

## Run

### GUI mode (default)

```bash
./target/release/duallink-sender
```

The settings window lets you:
- Browse auto-discovered receivers via mDNS (no IP entry needed)
- Enter receiver IP + pairing PIN manually as fallback
- Choose display index, resolution, FPS, bitrate
- Start / stop the capture pipeline

### Headless mode

```bash
DUALLINK_NO_UI=1 \
DUALLINK_HOST=192.168.1.100 \
DUALLINK_PIN=123456 \
DUALLINK_DISPLAY=0 \
DUALLINK_WIDTH=1920 DUALLINK_HEIGHT=1080 DUALLINK_FPS=60 \
DUALLINK_KBPS=8000 \
./target/release/duallink-sender
```

| Variable | Default | Description |
|----------|---------|-------------|
| `DUALLINK_HOST` | `192.168.1.100` | Receiver IP address |
| `DUALLINK_PIN` | `000000` | 6-digit pairing PIN shown by receiver |
| `DUALLINK_DISPLAY` | `0` | Zero-based display index |
| `DUALLINK_WIDTH` / `HEIGHT` | `1920` / `1080` | Capture/encode resolution |
| `DUALLINK_FPS` | `60` | Target frame rate |
| `DUALLINK_KBPS` | `8000` | H.264 bitrate in kbps |

---

## mDNS Discovery

The sender browses for `_duallink._tcp.local.` services on start. Any running
DualLink receiver on the same subnet will appear automatically in the UI. The
TXT record carries the receiver's LAN IP, port, display count, and a short TLS
fingerprint for TOFU verification.

---

## Input Injection

Keyboard and mouse events captured inside the receiver's video window are
forwarded back to the Linux sender over the TLS signaling back-channel and
replayed via an `evdev` uinput virtual device.

---

## Encoder priority

| Priority | Element | Requires |
|----------|---------|---------|
| 1 | `vaapih264enc` | Intel/AMD GPU VA-API |
| 2 | `nvh264enc` | NVIDIA GPU (NVENC) |
| 3 | `x264enc` | Software (always available) |

---

## Implementation Status

| Feature | Phase | Status |
|---------|-------|--------|
| `duallink-capture-linux` — PipeWire (`ashpd`) + GStreamer `pipewiresrc` | 5C | ✅ |
| `duallink-transport-client` — TLS `SignalingClient` + UDP `VideoSender` | 5C | ✅ |
| `encoder.rs` — GStreamer H.264 (`vaapih264enc` / `nvh264enc` / `x264enc`) | 5C | ✅ |
| `SenderPipeline` — per-display capture → encode → send task | 5D | ✅ |
| `Arc<Notify>` clean pipeline stop | 5D | ✅ |
| `input_inject.rs` — uinput virtual mouse + keyboard | 5D | ✅ |
| egui settings UI | 5D | ✅ |
| mDNS receiver discovery panel in UI | 5E | ✅ |
| Multi-display sender (N parallel `SenderPipeline` tasks) | 5D | ✅ |
| X11 XShm fallback capture backend | 6 | 🔲 |
| Absolute mouse positioning (ABS_X/Y tablet device) | 6 | 🔲 |
