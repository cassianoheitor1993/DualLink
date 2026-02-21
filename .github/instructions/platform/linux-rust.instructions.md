---
applyTo: "linux-receiver/**,linux-sender/**"
---

# Platform: Linux (Rust)

> Carregado automaticamente ao editar arquivos em `linux-receiver/` ou `linux-sender/`.

## Ambiente

- **Linguagem:** Rust 2021 edition
- **Async Runtime:** tokio
- **Min Rust Version:** 1.75+
- **Target:** x86_64-unknown-linux-gnu
- **GPU:** NVIDIA (NVDEC/NVENC) ou AMD/Intel (VAAPI)

## Dependências Core

| Componente | Crate / Lib | Uso |
|-----------|-------------|-----|
| Async Runtime | `tokio` | Event loop, tasks, channels |
| Video Pipeline | `gstreamer` + `gstreamer-video` | Decode (receiver) / Encode (sender) |
| GPU Accel | VAAPI plugin / NVDEC+NVENC plugin | Hardware acceleration |
| TLS Transport | `rustls` + `tokio-rustls` | Signaling channel (TCP 7879+2n) |
| UDP Transport | `tokio::net::UdpSocket` | Video data (UDP 7878+2n) |
| Error Handling | `thiserror` + `anyhow` | Typed errors + context |
| Logging | `tracing` + `tracing-subscriber` | Structured logging |
| UI | `eframe` + `egui` | Desktop UI (receiver GUI + sender UI) |
| mDNS | `mdns-sd` | Service advertise (receiver) + browse (sender) |
| Screen Capture | `ashpd` (PipeWire portal) | Wayland capture (linux-sender) |
| Input Inject | `uinput` crate | Virtual evdev device (linux-sender) |
| Crypto | `rcgen` + `sha2` | TLS self-signed cert + fingerprint |

## Estrutura — linux-receiver

```
linux-receiver/
├── Cargo.toml                       # Workspace root
├── crates/
│   ├── duallink-core/               # Core types: StreamConfig, DisplayInfo, errors
│   ├── duallink-decoder/            # GStreamer H.264 decode pipeline
│   ├── duallink-renderer/           # Fullscreen render (GStreamer sink / glutin)
│   ├── duallink-transport/          # DLNK receiver: TLS signaling + UDP ingest
│   ├── duallink-discovery/          # mDNS advertiser (DualLinkAdvertiser)
│   ├── duallink-input/              # Input capture & TCP back-channel forwarding
│   ├── duallink-gui/                # egui multi-display control panel
│   └── duallink-app/                # Binary entry point (CLI: duallink-receiver)
├── tests/
└── benches/
```

## Estrutura — linux-sender

```
linux-sender/
├── Cargo.toml                       # Workspace root
├── crates/
│   ├── duallink-capture-linux/      # PipeWire capture via ashpd + GStreamer pipewiresrc
│   ├── duallink-transport-client/   # DLNK sender: TLS SignalingClient + UDP VideoSender
│   ├── duallink-discovery-browse/   # mDNS browser (finds _duallink._tcp.local. receivers)
│   ├── duallink-input-inject/       # uinput virtual device (mouse + keyboard)
│   └── duallink-linux-sender/       # Binary: duallink-sender (GUI + headless)
```

## Padrões Rust para Este Projeto

### Error Handling

```rust
// Usar thiserror para erros de módulo (libraries/crates)
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DecoderError {
    #[error("GStreamer initialization failed: {0}")]
    GstInit(#[from] gstreamer::glib::Error),
    #[error("Hardware decoder unavailable")]
    HardwareUnavailable,
    #[error("Failed to decode frame: {reason}")]
    DecodeFailed { reason: String },
}

// Usar anyhow apenas no binary (main.rs)
```

### Async & Pipeline Stop Pattern

```rust
// Arc<Notify> para parar pipelines de forma limpa
use std::sync::Arc;
use tokio::sync::Notify;

pub struct SenderPipeline {
    stop: Arc<Notify>,
}

impl SenderPipeline {
    pub fn stop(&self) { self.stop.notify_one(); }

    async fn run_inner(stop: Arc<Notify>) {
        loop {
            tokio::select! {
                _ = stop.notified() => break,
                // frame = capture.next_frame() => { encode + send }
            }
        }
    }
}

// Evitar: Arc<Mutex<bool>> polling para parar tasks
// Evitar: block_on() dentro de contexto async
```

### Hot-Reload de Configuração

```rust
// Pattern usado no duallink-app para reconfigurar stream sem reiniciar processo
struct AppState {
    pending_config: Option<StreamConfig>,
}

// Quando chega nova config via sinal TCP:
//   1. Armazenar em pending_config
//   2. Enviar "config_updated" ao loop de decode
//   3. O loop quebra 'reconnect, reinicializa o decoder com novo config
```

### Logging

```rust
use tracing::{info, warn, error, debug, instrument};

#[instrument(skip(frame_data), fields(size = frame_data.len()))]
pub async fn decode_frame(frame_data: &[u8]) -> Result<DecodedFrame, DecoderError> {
    debug!("decoding frame");
    info!(latency_ms = elapsed.as_millis(), "frame decoded");
}
```

## GStreamer — Decoder Priority (Receiver)

```rust
// Sempre tentar decoders nesta ordem:
let pipeline_desc = match detect_gpu() {
    Gpu::Vaapi  => "appsrc name=src ! h264parse ! vaapih264dec ! videoconvert ! appsink name=sink",
    Gpu::Nvidia => "appsrc name=src ! h264parse ! nvh264dec    ! videoconvert ! appsink name=sink",
    Gpu::None   => "appsrc name=src ! h264parse ! avdec_h264   ! videoconvert ! appsink name=sink",
};
// vaapidecodebin tambem funciona como auto-selector VAAPI
```

## GStreamer — Encoder Priority (linux-sender)

```rust
let encoder = match detect_gpu() {
    Gpu::Vaapi  => "vaapih264enc",
    Gpu::Nvidia => "nvh264enc",
    Gpu::None   => "x264enc",
};
// Usar tune=zerolatency para baixa latencia
```

### Best Practices GStreamer

1. Sempre checar capabilities: `gst-inspect-1.0 vaapih264dec`
2. Configurar `appsrc` caps **antes** de PLAYING
3. Usar `appsink` com `emit-signals=true` + callback, nao polling
4. Tratar `EOS` e `Error` do bus — nao ignorar silenciosamente
5. `set_state(Null)` **antes** de dropar o pipeline

## mDNS

```rust
// Receiver — anunciar servico
use duallink_discovery::DualLinkAdvertiser;

let advertiser = DualLinkAdvertiser::register(
    "my-receiver",
    2,           // num displays
    7879,        // porta base TCP
    local_ip,    // detect_local_ip()
    &tls_fp,     // SHA-256 fingerprint hex
)?;

// Sender — descobrir receivers
// Usar mdns-sd::ServiceDaemon para browse _duallink._tcp.local.
```

### detect_local_ip() — UDP Probe Trick

```rust
fn detect_local_ip() -> IpAddr {
    let socket = UdpSocket::bind("0.0.0.0:0").unwrap();
    socket.connect("8.8.8.8:80").unwrap();
    socket.local_addr().unwrap().ip()
}
```

## Performance

- Hot path decode->render: **zero allocations**
- Usar `jemalloc` como allocator global
- `tokio::task::spawn_blocking` para operacoes CPU-bound
- Profiling com `perf` ou `cargo flamegraph`

## Armadilhas Conhecidas

1. **GStreamer plugins ausentes** — checar com `gst-inspect-1.0 vaapih264dec`
2. **NVIDIA** — NVDEC/NVENC requer driver proprietario (>= 470.x), nao funciona com nouveau
3. **Wayland vs X11** — `glutin`/`winit` abstrai, mas testar os dois
4. **uinput** — requer `/dev/uinput` + `uinput` kernel module; pode precisar de udev rule
5. **mDNS UDP multicast** — porta 5353/UDP pode ser bloqueada por firewall
6. **PipeWire screen capture** — requer XDG portal ativo; testar com `xdg-desktop-portal-gnome` ou `-wlr`
