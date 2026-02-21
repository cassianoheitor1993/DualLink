# DualLink Linux Sender

Turns a Linux machine into a DualLink **sender** — share its screen wirelessly
or via USB-C with any DualLink receiver (Linux, Windows, or macOS running the
DualLink receiver app).

> **Phase 5C** — PipeWire capture, GStreamer H.264 encoder, TLS signaling
> client, and UDP DLNK video sender are now implemented.  The egui UI and
> multi-display sender are planned for Phase 5D.

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
```

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

```bash
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

## Encoder priority

| Priority | Element | Requires |
|----------|---------|---------|
| 1 | `vaapih264enc` | Intel/AMD GPU VA-API |
| 2 | `nvh264enc` | NVIDIA GPU (NVENC) |
| 3 | `x264enc` | Software (always available) |

---

## Phase 5C Status

- [x] `duallink-capture-linux` — PipeWire portal (`ashpd`) + GStreamer `pipewiresrc` → `appsink`
- [x] `duallink-transport-client` — TLS signaling client (`SignalingClient`) + UDP sender (`VideoSender`)
- [x] `encoder.rs` — GStreamer H.264 encoder (`vaapih264enc` / `nvh264enc` / `x264enc` fallback)
- [x] Full capture → encode → send loop in `main.rs` (env-var config)
- [ ] egui settings UI (Phase 5D)
- [ ] Multi-display sender — N parallel pipelines (Phase 5D)
- [ ] X11 XShm fallback capture backend (Phase 6)
