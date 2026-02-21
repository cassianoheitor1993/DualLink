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

## Phase 2 — Extended Display + 60fps ✅ COMPLETE

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

## Phase 3 — USB-C Transport 🔄 IN PROGRESS

### Sprint 3.1 — USB Ethernet Transport ✅
- **Goal:** Enable low-latency USB-C transport between Mac and Linux
- **Research finding:** Lenovo Legion 5 Pro has xHCI-only USB-C controllers (no UDC/gadget mode).
  CDC-NCM gadget approach (`infra/usb-gadget/`) requires UDC hardware not present on this laptop.
- **Decision:** Use USB-C Ethernet adapters instead — same TCP/UDP transport, ~1ms latency,
  zero code changes to the streaming pipeline.
- **Implementation (macOS):**
  - `TransportDiscovery` — scans `getifaddrs()` for interfaces on `10.0.1.x` subnet
  - `probeReachability()` — TCP connect probe to verify receiver is reachable
  - `bestEndpoint()` — prioritises USB over Wi-Fi, falls back gracefully
  - `TransportBenchmark` — measures TCP ping latency for diagnostics
- **Implementation (Linux):**
  - `duallink-core/src/usb.rs` — `detect_usb_ethernet()` scans `/sys/class/net/` + `ip addr`
  - Receiver logs USB Ethernet status at startup
  - `infra/usb-gadget/` scripts preserved for machines that support gadget mode
- **Status:** ✅ Code complete

### Sprint 3.2 — USB Pipeline Integration ✅
- **Goal:** Seamless transport selection with auto-detection
- **Implementation:**
  - ContentView: Auto/USB/Wi-Fi transport picker (`TransportSelection` enum)
  - AppState: `connectAndStream()` resolves transport endpoint before connecting
  - Reconnection logic with transport failover (USB→Wi-Fi or re-discovery)
  - Transport benchmark runs in background after connection established
- **Setup instructions:**
  1. Connect USB-C Ethernet adapter to both machines
  2. Linux: `sudo ip addr add 10.0.1.1/24 dev <iface> && sudo ip link set <iface> up`
  3. Mac: System Settings → Network → USB Ethernet → Manual → IP: 10.0.1.2, Mask: 255.255.255.0
  4. Verify: `ping 10.0.1.1` from Mac
  5. DualLink app: select "Auto" or "USB" transport mode → connects at ~1ms latency
- **Status:** ✅ Code complete — awaiting USB Ethernet adapter for hardware validation

---

## Phase 4 — Security & Polish 🔄 IN PROGRESS

### Sprint 4.1 — TLS + Pairing PIN ✅
- **Goal:** Encrypt the signaling channel and authenticate pairing with a 6-digit PIN
- **Implementation (Linux):**
  - `tokio-rustls` 0.26 + `rustls` 0.23 (ring backend) for TLS server
  - `rcgen` 0.13 — ephemeral self-signed certificate with SANs (duallink.local, localhost, 10.0.1.1)
  - SHA-256 fingerprint logged at startup for future TOFU pinning
  - `generate_pairing_pin()` — 6-digit PIN displayed in a box at receiver startup
  - `run_signaling_server()` wraps each TCP connection in `TlsAcceptor` before handling
  - `handle_signaling_conn()` validates `pairing_pin` in the hello message:
    - Match → `hello_ack(accepted: true)`
    - Mismatch → `hello_ack(accepted: false, reason: "Invalid pairing PIN")` + disconnect
- **Implementation (macOS):**
  - `NWProtocolTLS.Options` with `sec_protocol_options_set_verify_block` (TOFU — accept self-signed)
  - `SignalingMessage.pairingPin` field added, wired through `sendHello()`
  - ContentView: PIN text field with lock icon, Start button disabled when PIN is empty
  - `connectAndStream()` passes PIN through to `sendHello()`, stored for reconnects
  - `handleMessage(.helloAck)` already surfaces rejection reason as `.failed(reason)` state
- **Security model:**
  - TLS 1.2/1.3 encryption on the signaling TCP channel
  - Trust-on-first-use (TOFU) for certificate verification
  - 6-digit PIN prevents unauthorized clients from connecting
  - PIN is ephemeral — regenerated on each receiver restart
- **Status:** ✅ Code complete — ready for integration testing

### Sprint 4.2 — Packaging & CI ✅
- **Goal:** Install the receiver as a system service; automate builds via CI
- **Linux packaging (`infra/linux/`):**
  - `install.sh` — builds if needed, installs binary to `/usr/local/bin/`,
    installs systemd user service, enables lingering for boot autostart.
    Supports `--uninstall` for clean removal.
  - `duallink-receiver.service` — systemd user unit: auto-restart on failure,
    display env vars (`DISPLAY`, `WAYLAND_DISPLAY`, `XDG_RUNTIME_DIR`), journald logging
- **CI (`.github/workflows/ci.yml`):**
  - `linux-receiver` job: Ubuntu 24.04, GStreamer deps, `cargo fmt` + `cargo clippy -D warnings`
    + `cargo build --release`, uploads binary artifact (14-day retention)
  - `mac-client` job: macOS 14 (Apple Silicon), `swift build -c release` + `swift test`
  - `release` job: triggers on `v*` tags — bundles binary + install script into
    `.tar.gz`, publishes GitHub Release with auto-generated notes
  - Cargo + Swift build caches for fast incremental CI runs
- **Usage:**
  ```bash
  sudo infra/linux/install.sh              # install & start
  systemctl --user status duallink-receiver
  journalctl --user -u duallink-receiver -f
  sudo infra/linux/install.sh --uninstall  # remove
  ```
- **Status:** ✅ Complete

### Sprint 4.3 — egui Control Panel GUI ✅
- **Goal:** Native Linux GUI app launchable from the app menu, replacing terminal-only UX
- **Crate:** `linux-receiver/crates/duallink-gui/` — eframe 0.29 / egui 0.29
- **Architecture:**
  - Main thread: `eframe::run_native()` renders egui window
  - Background OS thread: dedicated tokio multi-thread runtime runs the full receiver lifecycle
  - Shared state: `Arc<Mutex<GuiState>>` — receiver writes, egui reads via snapshot
- **UI features:**
  - Status badge with colour (grey/yellow/blue/green/red) and phase label
  - Large monospace pairing PIN with one-click copy button (flashes "Copied!")
  - Collapsible TLS certificate fingerprint section (TOFU reference)
  - Streaming stats chips: FPS, decoded frames, received frames, Mbit/s (1-second rolling window)
  - Log panel with auto-scroll toggle, colour-coded ERROR/WARN/info lines
  - Quit button
  - Custom dark theme (card-based layout, accent blue `#6390FF`)
- **Receiver lifecycle (in GUI):**
  - USB Ethernet auto-detection at startup
  - Auto-stops `duallink-receiver.service` if it holds the ports (no manual step needed)
  - Session reconnect loop — PIN stays valid across client disconnects
  - Actionable error messages if ports still conflict after service stop
- **Desktop integration:**
  - `infra/linux/duallink-receiver.desktop` — `Exec=duallink-gui`, `Terminal=false`
  - `infra/linux/duallink-receiver.svg` — custom dark-themed SVG icon
  - `install.sh` installs both `duallink-receiver` + `duallink-gui` to `/usr/local/bin/`
    and registers the `.desktop` + icon in `~/.local/share/`
- **transport changes:** `StartupInfo { pairing_pin, tls_fingerprint }` added as 5th return
  value from `DualLinkReceiver::start()` for GUI consumption
- **Status:** ✅ Complete — validated 2026-02-20; app appears in GNOME app launcher

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

*Last updated: 2026-02-20 — Sprint 4.3 (GUI) complete*

---

## Phase 5 — Platform Expansion & Multi-Monitor

### Sprint 5A — Multi-Monitor Protocol & Cross-Platform Receiver *(~2026-02-24)*

**Goal:** Extend transport to support N independent display port pairs; add platform receiver skeletons.

- **Multi-display transport (`duallink-transport`):**
  - `DualLinkReceiver::start_all(n: u8)` binds N port pairs (UDP `7878+2n` / TCP `7879+2n`)
  - `DisplayChannels { frame_rx, event_rx, display_index }` — per-display channel bundle
  - `SIGNALING_PORT` constant exported for advertiser use
  - `StartupInfo { pairing_pin, tls_fingerprint }` shared across all displays (single TLS identity)
- **DLNK frame header extended** — byte `[17]` encodes display index (0–7)
- **`duallink-decoder` update** — `DecoderFactory::best_available_with_display(w, h)` creates a GStreamer decode+display pipeline per display
- **Status:** ✅ Complete

---

### Sprint 5B — Windows & macOS Receiver Skeletons + Linux Sender Scaffold *(~2026-02-26)*

**Goal:** Scaffold cross-platform receiver crates and Linux sender workspace.

- **Windows receiver skeleton** (`windows-sender/crates/duallink-core`) — shared types crate (Rust, MSVC target)
- **macOS receiver scaffold** — Swift Package Manager workspace; `ScreenCapture`, `VideoEncoder`, `VirtualDisplay`, `Signaling`, `Streaming`, `InputInjection`, `Transport`, `Discovery` target structure
- **Linux sender workspace** (`linux-sender/`) — Cargo workspace with `duallink-core`, `duallink-capture-linux`, `duallink-linux-sender`
- **CI:** `linux-sender-build` and `windows-sender-build` jobs added to `ci.yml`
- **Status:** ✅ Complete

---

### Sprint 5C — Rust Linux Sender Transport Client *(~2026-03-01)*

**Goal:** Implement the full sender-side transport client in Rust for Linux.

- **`duallink-transport-client` crate** — `SignalingClient` (TLS/TCP) + `VideoSender` (UDP DLNK-framed)
  - `hello` / PIN / config handshake with receiver
  - `send_frame()` — DLNK header construction + UDP packet dispatch
- **`duallink-capture-linux` crate** — PipeWire `xdg-desktop-portal` screen capture via `ashpd`
  - `PipeWireCapture::open_session(display_index)` → raw BGRA frame stream
- **`duallink-linux-sender/src/encoder.rs`** — GStreamer H.264 encoder
  - Elements tried in order: `vaapih264enc` → `nvh264enc` → `x264enc`
  - `appsrc → videoconvert → <encoder> → appsink` pipeline
- **Status:** ✅ Complete

---

### Sprint 5D — Sender UI + Input Injection + Multi-Display *(~2026-03-04)*

**Goal:** Working end-to-end Linux sender with GUI and input forwarding.

- **`duallink-linux-sender/src/ui.rs`** — egui settings UI
  - Inputs: receiver host, pairing PIN, resolution, FPS, bitrate, display count
  - Start / Stop buttons wired to `SenderPipeline`
  - mDNS discovery picker (browse `_duallink._tcp.local.` — Phase 5E backfill)
- **`SenderPipeline`** — per-display capture → encode → UDP-send async task
  - `Arc<Notify>` stop signal (clean shutdown without channel races)
  - Reconnect loop: pipeline restarts on disconnect without process restart
- **`duallink-linux-sender/src/input_inject.rs`** — uinput virtual mouse + keyboard
  - Receives `InputEvent` from receiver via signaling TCP back-channel
  - Creates `VirtualDevice` (evdev) for mouse and keyboard separately
- **`duallink-app` CLI** — updated to use `start_all()` and run N display loops concurrently
- **Status:** ✅ Complete

---

### Sprint 5E — mDNS Advertising + Windows WGC Sender *(~2026-03-08)*

**Goal:** Zero-config discovery for all platforms; full Windows sender pipeline.

- **`duallink-discovery` crate** (`linux-receiver/crates/`):
  - `DualLinkAdvertiser::register(name, displays, port, ip, fp)` — registers `_duallink._tcp.local.` mDNS service via `mdns-sd`
  - TXT record: `version`, `displays`, `port`, `host`, `fp` (first 16 hex chars of TLS fingerprint)
  - `detect_local_ip()` — UDP probe trick for primary LAN IPv4 (no packets sent)
- **`duallink-app/src/app.rs`** — wires `detect_local_ip()` + `DualLinkAdvertiser::register()` at startup; logs `"Enter <ip> in the DualLink sender app"`
- **Windows sender (`duallink-windows-sender`):**
  - `WGCCapture` — `GraphicsCaptureSession` + `FramePool` → D3D11 texture → CPU staging → BGRA bytes
  - `GStreamer H.264 encoder` — `appsrc → videoconvert → mfh264enc / nvh264enc / x264enc → appsink`
  - `WinSenderPipeline` — per-display capture → encode → UDP + TLS signaling task
  - `WinSenderUi` (egui) — receiver host, PIN, resolution, FPS, bitrate fields + Start/Stop
  - mDNS discovery panel in UI using `mdns-sd` browse
- **macOS sender — SwiftUI Discovery UI:**
  - `DiscoveryService.swift` — `NWBrowser` for `_duallink._tcp` + TXT record parsing
  - `ContentView.swift` — receiver picker, connection status, PIN entry field
- **linux-sender UI** — mDNS browse + receiver picker added to UI
- **Status:** ✅ Complete — committed `f85a6b6` (18 files, +1605 insertions)

---

### Sprint 5F — SendInput Injection + Decoder Hot-Reload + SwiftUI Discovery *(~2026-03-10)*

**Goal:** Full input round-trip on Windows; seamless resolution changes; polished macOS UI.

- **Windows SendInput injection (`input_inject.rs`):**
  - `SendInputInjector::inject(event)` — translates `InputEvent` to `SendInput` Win32 calls
  - Full VK map: letters, digits, F-keys, modifiers, arrows, media keys
  - Mouse absolute positioning via `MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE`
- **`WinSenderPipeline` stop fix:**
  - Replaced `oneshot::Sender` (drop-based, unreliable) with `Arc<Notify>`
  - `stop()` calls `notify_waiters()`; task awaits `stop_notify.notified()`
  - Eliminates "pipeline won't stop" race on quick Start→Stop cycles
- **Decoder hot-reload (`duallink-app/src/app.rs`):**
  - `ConfigUpdated` with resolution change breaks frame loop with `"config_updated"` reason
  - `pending_config: Option<StreamConfig>` carries new config into next `'reconnect` iteration
  - Decoder re-initialised with new `width × height` without TCP reconnect
- **SwiftUI Discovery polish (`mac-client`):**
  - `NWTXTRecord` → `dictionary` computed property (iterates `self.keys` + `getEntry(for:)`)
  - `PeerInfo` view updated to show LAN IP, display count, short fingerprint
  - Auto-connects when only one receiver is visible on the LAN
- **Status:** ✅ Complete — committed `61844ed` (7 files, +409 insertions)

---

### Sprint 5G — duallink-gui mDNS + Multi-Display + LAN IP *(~2026-03-12)*

**Goal:** Bring the egui GUI receiver up to parity with the CLI receiver.

- **`duallink-gui` upgraded to multi-display (`start_all`):**
  - Imports `duallink-discovery` dependency — `DualLinkAdvertiser` + `detect_local_ip()`
  - `DualLinkReceiver::start()` → `start_all(DUALLINK_DISPLAY_COUNT)` (env-var, default 1)
  - Extra displays (index ≥ 1) run as background tokio tasks (`run_background_display`)
  - Display 0 drives GUI state as before
- **mDNS advertising from GUI:**
  - `detect_local_ip()` called after startup; `DualLinkAdvertiser::register()` called with PIN fingerprint
  - `GuiState.lan_ip`, `GuiState.mdns_active`, `GuiState.display_count` fields added
- **PIN card shows connection info:**
  - LAN IP: `"Connect from: 192.168.X.Y  •  1 display"` row below PIN
  - mDNS badge: `"mDNS ✓"` (green) / `"mDNS ✗"` (orange)
- **Decoder hot-reload in GUI:**
  - `pending_config: Option<StreamConfig>` — `ConfigUpdated` with resolution change breaks frame loop, decoder re-initialised next iteration
- **Streaming stats — Displays chip** added to stats card
- **windows-sender README rewritten** — removes "Phase 5B skeleton" warning, documents full Phase 5E/5F feature set
- **CI upgrades:**
  - `linux-sender-build`: renamed from "skeleton"; upgraded to `cargo build` (not just `check`)
  - `windows-sender-build`: checks `duallink-windows-sender` crate (not just core)
- **Status:** ✅ Complete
