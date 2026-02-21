# DualLink

> Turn any machine into an external display — share your screen between macOS, Linux, and Windows over Wi-Fi or USB-C.

## Overview

DualLink is a cross-platform screen-sharing app that lets you use a laptop as a real
secondary display.  Any machine can be a **sender** (sharing its screen) and any machine
can be a **receiver** (displaying the incoming stream).

| Role | Platform | Status |
|------|----------|--------|
| Sender | macOS 14+ (Sonoma) | ✅ Phase 5F |
| Sender | Linux (PipeWire + GStreamer) | ✅ Phase 5F |
| Sender | Windows 10+ (WGC + GStreamer) | ✅ Phase 5F |
| Receiver | Linux (GStreamer + VA-API/NVDEC) | ✅ Phase 5G |
| Receiver | Windows (planned) | 🔲 Phase 6 |
| Receiver | macOS (planned) | 🔲 Phase 6 |

## Key Features

- **Custom DLNK protocol** — lightweight UDP video framing + TLS TCP signaling (no WebRTC overhead)
- **Hardware-accelerated encode & decode** — VideoToolbox / GStreamer VA-API / NVDEC / Media Foundation
- **Zero-config discovery** — mDNS (`_duallink._tcp.local.`) with TXT record carrying IP, port, PIN hint
- **PIN + TLS TOFU pairing** — 6-digit pairing PIN + certificate fingerprint; no cloud, no accounts
- **Multi-display** — receiver exposes N independent port pairs; senders connect to each
- **Input round-trip** — mouse/keyboard captured on receiver, forwarded back to sender (uinput / CGEvent / SendInput)
- **Decoder hot-reload** — resolution changes reconfigure the decoder pipeline without reconnecting
- **USB-C auto-detect** — switches to USB Ethernet when plugged in (sub-5ms latency)

## Architecture

```
Sender                                    Receiver
──────────────────────────────────────    ─────────────────────────────────────
 ScreenCapture  (SCK / PipeWire / WGC)    DualLinkReceiver::start_all(n)
       │                                        │
 VideoEncoder   (VTB / GStreamer / MF)    GStreamer decode+display
       │                                        │
 TransportClient                          mDNS advertise  (_duallink._tcp.local.)
   SignalingClient  →  TLS TCP:7879+2n  →  SignalingServer
   VideoSender      →  UDP  :7878+2n   →  UDP receiver
   ← InputEvent     ←  TLS TCP (back)  ←  InputSender
```

## Project Structure

```
screen-mirroring-app/
├── mac-client/          # macOS sender (Swift 5.9, SPM)
├── linux-receiver/      # Linux receiver (Rust workspace)
│   └── crates/
│       ├── duallink-core/          # Shared types, config, errors
│       ├── duallink-transport/     # UDP + TLS TCP receiver stack
│       ├── duallink-transport-client/ # UDP + TLS TCP sender stack (re-exported)
│       ├── duallink-decoder/       # GStreamer H.264 decode + display
│       ├── duallink-discovery/     # mDNS advertiser + detect_local_ip
│       ├── duallink-input/         # uinput injection (Linux)
│       ├── duallink-app/           # CLI receiver binary
│       └── duallink-gui/           # egui GUI receiver binary
├── linux-sender/        # Linux sender (Rust workspace)
│   └── crates/
│       ├── duallink-capture-linux/ # PipeWire + GStreamer capture
│       └── duallink-linux-sender/  # egui UI + GStreamer encode + send
├── windows-sender/      # Windows sender (Rust workspace, MSVC)
│   └── crates/
│       ├── duallink-core/          # Shared types
│       └── duallink-windows-sender/ # WGC capture + GStreamer + egui
├── docs/                # Documentation & specs
├── infra/               # CI/CD, systemd service, install scripts
└── .github/             # Copilot instructions (modular)
```

## Quick Start

### Linux Receiver

```bash
# Install GStreamer + VA-API
sudo apt-get install -y gstreamer1.0-plugins-{base,good,bad,ugly} \
  gstreamer1.0-vaapi gstreamer1.0-libav

cd linux-receiver
cargo build --release -p duallink-gui

# Run GUI receiver (shows PIN + LAN IP for senders to connect)
./target/release/duallink-gui

# OR headless CLI receiver
./target/release/duallink-receiver

# Set number of virtual display streams (default 1)
DUALLINK_DISPLAY_COUNT=2 ./target/release/duallink-gui
```

### macOS Sender

```bash
cd mac-client
./run_app.sh          # build + bundle + launch (required for CGVirtualDisplay)
```

### Linux Sender

```bash
cd linux-sender
cargo build --release -p duallink-linux-sender
./target/release/duallink-sender    # opens egui settings window
```

### Windows Sender

```powershell
cd windows-sender
$env:PKG_CONFIG_PATH = "C:\gstreamer\1.0\msvc_x86_64\lib\pkgconfig"
cargo build --release -p duallink-windows-sender
.\target\release\duallink-sender.exe   # opens egui settings window
```

## Connection Flow

1. Start receiver → note the **6-digit PIN** and **LAN IP** shown in the UI (or log)
2. Start sender → receivers auto-appear via mDNS; select one (or enter IP manually)
3. Enter PIN → TLS TOFU handshake → streaming begins
4. Resize/move windows freely — resolution changes hot-reload the decoder

## Documentation

- [Work Plan](docs/WORK_PLAN.md) — Full development roadmap
- [Milestones](docs/MILESTONES.md) — Epics and user stories
- [Progress](docs/PROGRESS.md) — Sprint-by-sprint implementation log
- [Technical Research](docs/TECHNICAL_RESEARCH.md) — Technology decisions

## CI

| Job | Runs on | What |
|-----|---------|------|
| `linux-receiver` | Ubuntu 24.04 | `cargo build --release` + GStreamer |
| `mac-client` | macOS 14 | `swift build` |
| `linux-sender-build` | Ubuntu 24.04 | `cargo build --workspace` |
| `windows-sender-build` | Windows | `cargo check` (+ GStreamer if available) |

## Requirements

| Platform | Requirements |
|----------|-------------|
| Linux receiver | Rust 1.75+, GStreamer 1.22+, VA-API or NVDEC drivers |
| macOS sender | macOS 14+, Swift 5.9+, `run_app.sh` (needs `.app` bundle for CGVirtualDisplay) |
| Linux sender | Rust 1.75+, GStreamer 1.22+, PipeWire (Ubuntu 22.04+) |
| Windows sender | Rust 1.75+ MSVC, GStreamer 1.22+ MSVC, Windows 10+ |

## License

MIT
