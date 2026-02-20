# DualLink — Progress Log

---

## Phase 0 — Research & Technical Validation ✅ COMPLETE

### Sprint 0.1 — Virtual Display Research (macOS) ✅
- Validated `CGVirtualDisplay` API on macOS 14+
- PoC: `poc/poc-virtual-display-app/` — creates 1920×1080 virtual display visible in System Preferences
- Entitlements documented, no SIP restrictions for non-sandboxed apps
- DriverKit evaluated as fallback (not needed)

### Sprint 0.2 — Screen Capture + Encoding PoC (macOS) ✅
- PoC: `poc/poc-screen-capture/` — ScreenCaptureKit capturing at 30fps+
- VideoToolbox H.264 encoding validated with hardware acceleration
- Encoding latency: ~2–4ms per frame on Apple Silicon

### Sprint 0.3 — Decoding + Rendering PoC (Linux) ✅
- PoC: `poc/poc-gstreamer/` — GStreamer probe script validated all decoder elements
- `probe.sh --no-display` results on Lenovo Legion 5 Pro (AMD Radeon 680M + RTX):
  - `vaapih264dec`: 5.1ms avg (PRIMARY)
  - `vaapidecodebin`: 5.5ms avg
  - `nvh264dec`: 6.0ms avg
  - `avdec_h264`: 16.8ms avg (software fallback)
- VAAPI confirmed operational on Ubuntu 24.04

---

## Phase 1 — MVP: Screen Mirroring (Wi-Fi) ✅ COMPLETE

### Sprint 1.1 — macOS Sender Core ✅
- **Project structure:** Swift Package Manager (`mac-client/`)
- **Modules implemented:**
  - `ScreenCapture/` — ScreenCaptureKit integration (display-specific capture)
  - `VideoEncoder/` — VideoToolbox H.264 encoding with hardware acceleration
    - Baseline profile, real-time mode, no B-frames
    - AVCC→Annex-B conversion with SPS/PPS injection on keyframes
  - `Streaming/` — UDP transport (DLNK protocol v1)
    - `FramePacketizer` — NAL data fragmentation into MTU-sized UDP datagrams
    - `VideoSender` — NWConnection-based UDP sender
  - `Signaling/` — TCP control channel
    - Length-prefixed JSON messages (hello, hello_ack, config_update, keepalive, stop)
    - `SignalingClient` actor with keepalive loop (1Hz)
  - `DualLinkCore/` — Shared models (StreamConfig, Resolution, PeerInfo, etc.)
  - `VirtualDisplay/` — CGVirtualDisplay management
  - `Discovery/` — mDNS/Bonjour service browsing
  - `DualLinkApp/` — SwiftUI app with connection UI

### Sprint 1.2 — Linux Receiver Core ✅
- **Project structure:** Cargo workspace (`linux-receiver/`)
- **Crates implemented:**
  - `duallink-core` — Shared types, errors, config (serde with camelCase/snake_case compat)
  - `duallink-transport` — UDP video receiver + TCP signaling server
    - DLNK protocol v1 parser (16-byte header + payload)
    - `FrameReassembler` — multi-fragment frame assembly with timeout eviction
    - `SignalingServer` — length-prefixed JSON, hello handshake with hello_ack
  - `duallink-decoder` — GStreamer H.264 decoder
    - Automatic codec probe: vaapih264dec → vaapidecodebin → nvh264dec → avdec_h264
    - Pipeline: `appsrc → h264parse → [decoder] → videoconvert → BGRA → appsink`
    - Annex-B byte-stream input, 500ms pull timeout for pipeline fill
  - `duallink-discovery` — mDNS service discovery via `mdns-sd` crate
  - `duallink-renderer` — Renderer trait defined (placeholder impl)
  - `duallink-input` — Input capture placeholder (Sprint 2.3)
  - `duallink-signaling` — Signaling abstractions
  - `duallink-webrtc` — WebRTC placeholder
  - `duallink-app` — Binary entry point
    - Dedicated decode thread via `spawn_blocking` + `mpsc::channel`
    - Stats logging: frames received/decoded/errors per 300 frames

### Sprint 1.3 — Shared Protocol ✅
- **DLNK UDP Frame Protocol v1:**
  ```
  [0..4]   magic      u32 BE   0x444C4E4B ("DLNK")
  [4..8]   frame_seq  u32 BE   monotonic frame counter
  [8..10]  frag_idx   u16 BE   0-based fragment index
  [10..12] frag_count u16 BE   total fragments for this frame
  [12..16] pts_ms     u32 BE   presentation timestamp (ms)
  [16]     flags      u8       bit0 = keyframe
  [17..20] reserved   [u8; 3]
  [20..]   payload    [u8]     H.264 NAL unit slice
  ```
- **Signaling Protocol v1** (TCP, length-prefixed JSON):
  - Message types: hello, hello_ack, config_update, keepalive, stop
  - StreamConfig exchanged in hello (resolution, targetFPS, maxBitrateBps, codec, lowLatencyMode)
- **mDNS service type:** `_duallink._tcp.local.`

### Sprint 1.4 — Integration & QA ✅
- **End-to-end validated:** MacBook Pro → Lenovo Legion 5 Pro over Wi-Fi (10.0.0.x LAN)
- **Results (2026-02-20):**
  - Handshake: hello → hello_ack in ~500ms
  - Decoder: `vaapih264dec` (VA-API hardware) selected automatically
  - First frame decoded after 4 pipeline-fill frames (~2s warmup)
  - Steady state: **1200 frames received, 1195 decoded, 4 errors** (99.6% success)
  - Throughput: ~30fps sustained (matching config `target_fps: 30`)
  - Keyframe size: ~110KB, P-frame: ~2–35KB
- **Issues resolved during integration:**
  - `Cargo.toml` duplicate keys in duallink-app manifest
  - `mdns-sd` API incompatibility (`ServiceBrowser` removed in v0.10)
  - Missing `thiserror` dependency in duallink-discovery
  - `DecoderError` import path (errors module not re-exported)
  - GStreamer closure type inference issues
  - StreamConfig serde field mismatch (camelCase vs snake_case) — fixed with `#[serde(alias)]`
  - H.264 stream format mismatch (AVCC vs Annex-B) — fixed Mac-side AVCC→Annex-B conversion
  - Concurrent GStreamer access via multiple `spawn_blocking` — fixed with dedicated decode thread
  - Caps mismatch (`avc` → `byte-stream`) after Annex-B conversion

---

## Phase 2 — Extended Display + 60fps 🔄 IN PROGRESS

### Sprint 2.1 — Fullscreen Renderer ✅
- **Goal:** Render decoded video in a fullscreen window on Linux
- **Approach:** GStreamer `autovideosink` integrated into decode pipeline (zero extra CPU copies)
- **Implementation:**
  - `GStreamerDisplayDecoder` in `duallink-decoder` — combined decode+display pipeline:
    `appsrc → h264parse → vaapih264dec → vaapipostproc → autovideosink sync=false`
  - VA-API surface alignment fix: `vaapipostproc` handles GPU surface height padding (e.g. 1088→1080)
    without CPU-side `videoconvert` failures
  - `DecoderFactory::best_available_with_display()` factory method
  - `push_frame()` — push encoded data, GStreamer handles decode AND display
  - GStreamer creates native X11/Wayland window via `autovideosink`
  - Dedicated `spawn_blocking` thread serialises GStreamer access
  - Cursor now visible in capture (`showsCursor = true`)
- **Architecture decision:** Using a single GStreamer pipeline (decode→display) instead of
  a separate `Renderer` pulling `DecodedFrame`. This avoids 2 unnecessary CPU copies per frame
  and leverages GStreamer's native windowing. The `Renderer` trait is preserved for future
  use cases (overlays, wgpu-based compositing).
- **Status:** ✅ Validated — fullscreen rendering on X11 with VA-API hardware decode

### Sprint 2.2 — 60fps Upgrade ✅
- **Goal:** Increase capture/encode/decode pipeline to 60fps sustained
- **Implementation:**
  - Added 60fps toggle in ContentView (ConnectView → ControlsView)
  - `StreamConfig.highPerformance` preset: 1920×1080 @ 60fps, 15Mbps
  - No Linux-side changes needed — GStreamer pipeline handles variable framerate natively
- **Status:** ✅ Validated — 60fps streaming over Wi-Fi (some latency expected, USB mode in Phase 3)

### Sprint 2.3 — Input Forwarding ✅
- **Goal:** Capture mouse/keyboard on Linux GStreamer window, forward to macOS for injection
- **Architecture:** GStreamer bus navigation events → InputSender (mpsc) → TCP signaling → Mac CGEvent
- **Implementation (Linux):**
  - `duallink-core/src/input.rs` — `InputEvent` enum (MouseMove, MouseDown, MouseUp, MouseScroll,
    KeyDown, KeyUp) with `#[serde(tag = "kind")]` for cross-platform JSON serialisation
  - `GStreamerDisplayDecoder::poll_input_events()` — drains GStreamer bus for navigation messages
  - `parse_navigation_event()` — converts GstNavigationMessage to `InputEvent` with normalised [0,1] coordinates
  - `x11_keyval_from_name()` — maps X11 key names to keyvals (common keys + Unicode fallback)
  - `InputSender` struct in transport crate — wraps `mpsc::Sender<InputEvent>` with `try_send()`
  - `SignalingMessage::InputEvent` message type added to TCP protocol
  - Signaling connection refactored: TCP stream split into reader/writer with `Arc<Mutex<WriteHalf>>`
  - Input writer task spawned after hello handshake — forwards queued events as JSON
- **Implementation (macOS):**
  - `InputEvent` + `MouseButton` added to `DualLinkCore/Models.swift` with custom `Codable`
    matching Rust's `#[serde(tag = "kind")]` format
  - `SignalingClient` updated: `onInputEvent` callback, `input_event` message handling
  - `InputInjectionManager` in `InputInjection/` module:
    - CGEvent injection: mouse move, click, scroll, key press/release
    - Normalised coordinate → absolute display coordinate mapping
    - X11 keyval → macOS virtual keycode translation table
    - Targets virtual display via `CGDirectDisplayID`
  - Wired in `DualLinkApp.swift`: `inputInjector.configure(displayID:)` + `onInputEvent` callback
- **Status:** ✅ Code complete — ready for integration testing

---

## Hardware Tested

| Machine | Role | OS | GPU | Status |
|---------|------|-----|-----|--------|
| MacBook Pro (M-series) | Sender | macOS 14+ | Apple Silicon | ✅ Validated |
| Lenovo Legion 5 Pro | Receiver | Ubuntu 24.04 | AMD Radeon 680M + NVIDIA RTX | ✅ Validated |

## Environment

- **Rust:** 1.75+ (workspace)
- **Swift:** 5.9+ (SPM)
- **GStreamer:** 1.24.2 (Ubuntu 24.04 packages)
- **VA-API:** Functional (gstreamer1.0-vaapi)

---

*Last updated: 2026-02-21*
