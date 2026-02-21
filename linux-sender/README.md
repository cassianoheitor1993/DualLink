# DualLink Linux Sender

Turns a Linux machine into a DualLink **sender** — share its screen wirelessly
or via USB-C with any DualLink receiver (Linux, Windows, or macOS running the
DualLink receiver app).

> **Phase 5B skeleton** — screen capture pipeline is not yet implemented.
> See the status table in `src/main.rs` for current progress.

---

## Architecture

```
PipeWire / XShm  →  GStreamer H.264 encode  →  UDP:7878  →  Receiver
(capture)           (vaapih264enc)              (DLNK frames)
                    TCP:7879 TLS signaling
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

## Run (when implemented)

```bash
DUALLINK_RECEIVER_IP=192.168.1.100 \
DUALLINK_PAIRING_PIN=123456 \
./target/release/duallink-sender
```

---

## Encoder priority

| Priority | Element | Requires |
|----------|---------|---------|
| 1 | `vaapih264enc` | Intel/AMD GPU VA-API |
| 2 | `nvh264enc` | NVIDIA GPU (NVENC) |
| 3 | `x264enc` | Software (always available) |

---

## Phase 5B TODO

- [ ] Implement `ScreenCapturer::next_frame` via `ashpd::desktop::ScreenCast`
- [ ] GStreamer encode pipeline (appsrc → videoconvert → vaapih264enc → appsink)
- [ ] `SignalingClientRust` — TCP TLS client sending `hello` to receiver
- [ ] `VideoSenderRust` — UDP DLNK-framed packet sender
- [ ] egui settings UI
- [ ] Multi-display sender (N pipelines for N receiver displays)
