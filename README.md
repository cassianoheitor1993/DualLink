# DualLink

> Transform your Linux laptop into an external display for macOS — via USB-C or Wi-Fi.

## Overview

DualLink connects a MacBook Pro to a Linux laptop (e.g., Lenovo Legion 5 Pro) to use it as:
- **Screen Mirror** — duplicate your Mac display
- **Extended Display** — use it as a real secondary monitor

### Key Features
- Hardware-accelerated encoding (VideoToolbox) and decoding (VAAPI/NVDEC)
- Low latency: < 40ms (USB-C) / < 80ms (Wi-Fi)
- WebRTC-based streaming with encryption
- Automatic device discovery via mDNS

## Architecture

```
┌─────────────────────┐         ┌─────────────────────┐
│   macOS (Sender)    │         │  Linux (Receiver)   │
│                     │         │                     │
│  Virtual Display    │         │  WebRTC Receiver    │
│       ↓             │  Wi-Fi  │       ↓             │
│  ScreenCaptureKit   │ ──or──→ │  GPU Decoder        │
│       ↓             │  USB-C  │  (VAAPI/NVDEC)      │
│  VideoToolbox H.264 │         │       ↓             │
│       ↓             │         │  Fullscreen Render  │
│  WebRTC Sender      │         │                     │
└─────────────────────┘         └─────────────────────┘
```

## Project Structure

```
duallink/
├── mac-client/          # macOS sender app (Swift)
├── linux-receiver/      # Linux receiver app (Rust)
├── shared-protocol/     # Protocol definitions (Protobuf)
├── docs/                # Documentation & specs
├── infra/               # CI/CD, Docker, scripts
└── ai-agent-instructions/
```

## Requirements

### macOS Client
- macOS 14+ (Sonoma)
- Xcode 15+
- Swift 5.9+

### Linux Receiver
- Linux with Wayland or X11
- Rust 1.75+
- GStreamer 1.20+
- NVIDIA drivers (for NVDEC) or Mesa (for VAAPI)

## Getting Started

> 🚧 Project is in early development. See [docs/WORK_PLAN.md](docs/WORK_PLAN.md) for the roadmap.

### macOS
```bash
cd mac-client
# Open in Xcode or build via command line
swift build
```

### Linux
```bash
cd linux-receiver
cargo build
```

## Documentation

- [Work Plan](docs/WORK_PLAN.md) — Full development roadmap
- [Milestones](docs/MILESTONES.md) — Epics and user stories
- [Technical Research](docs/TECHNICAL_RESEARCH.md) — Technology decisions and PoC notes

## License

MIT

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) (coming soon).
