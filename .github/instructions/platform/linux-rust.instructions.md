---
applyTo: "linux-receiver/**"
---

# Platform: Linux (Rust)

> Carregado automaticamente ao editar arquivos em `linux-receiver/`.

## Ambiente

- **Linguagem:** Rust 2021 edition
- **Async Runtime:** tokio
- **Min Rust Version:** 1.75+
- **Target:** x86_64-unknown-linux-gnu
- **GPU:** NVIDIA (NVDEC primary), AMD/Intel (VAAPI fallback)

## Dependências Core

| Componente | Crate / Lib | Uso |
|-----------|-------------|-----|
| Async Runtime | `tokio` | Event loop, tasks, channels |
| Video Decoding | `gstreamer` + `gstreamer-video` | Pipeline de decoding |
| GPU Decoding | VAAPI plugin / NVDEC plugin | Hardware acceleration |
| WebRTC | `webrtc-rs` ou `gstreamer-webrtc` | Receber stream |
| Serialização | `prost` | Protocol Buffers |
| Error Handling | `thiserror` + `anyhow` | Typed errors + context |
| Logging | `tracing` + `tracing-subscriber` | Structured logging |
| UI | `tauri` v2 | Desktop UI |
| Window | `winit` ou GStreamer sink | Fullscreen rendering |
| mDNS | `mdns-sd` | Service discovery |

## Estrutura do linux-receiver

```
linux-receiver/
├── Cargo.toml                  # Workspace root
├── crates/
│   ├── duallink-core/          # Core types, config, errors
│   ├── duallink-decoder/       # GStreamer video decoding
│   ├── duallink-renderer/      # Fullscreen rendering
│   ├── duallink-webrtc/        # WebRTC receiver
│   ├── duallink-signaling/     # Signaling client
│   ├── duallink-discovery/     # mDNS discovery
│   ├── duallink-transport/     # Abstração USB/Wi-Fi
│   ├── duallink-input/         # Input capture & forwarding
│   └── duallink-app/           # Binary entry point + Tauri UI
├── tests/                      # Integration tests
└── benches/                    # Benchmarks
```

## Padrões Rust para Este Projeto

### Error Handling

```rust
// ✅ Usar thiserror para erros de módulo
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DecoderError {
    #[error("GStreamer initialization failed: {0}")]
    GstInit(#[from] gstreamer::glib::Error),
    
    #[error("Hardware decoder unavailable, falling back to software")]
    HardwareUnavailable,
    
    #[error("Failed to decode frame: {reason}")]
    DecodeFailed { reason: String },
}

// ✅ Usar anyhow apenas no binary (main, CLI)
// ✅ Usar thiserror em libraries/crates
```

### Async Patterns

```rust
// ✅ Usar tokio channels para comunicação entre módulos
use tokio::sync::mpsc;

pub struct DecoderPipeline {
    frame_rx: mpsc::Receiver<EncodedFrame>,
    decoded_tx: mpsc::Sender<DecodedFrame>,
}

impl DecoderPipeline {
    pub async fn run(&mut self) -> Result<(), DecoderError> {
        while let Some(frame) = self.frame_rx.recv().await {
            let decoded = self.decode(frame).await?;
            self.decoded_tx.send(decoded).await
                .map_err(|_| DecoderError::DecodeFailed { 
                    reason: "renderer disconnected".into() 
                })?;
        }
        Ok(())
    }
}

// ❌ Evitar: Arc<Mutex<>> para comunicação entre tasks (usar channels)
// ❌ Evitar: block_on() dentro de contexto async
```

### Buffer Management

```rust
// ✅ Zero-copy: usar slices e referências quando possível
pub struct EncodedFrame<'a> {
    pub data: &'a [u8],
    pub timestamp_us: u64,
    pub is_keyframe: bool,
}

// ✅ Para dados owned, usar bytes::Bytes (reference-counted, cheap clone)
use bytes::Bytes;

pub struct OwnedFrame {
    pub data: Bytes,
    pub timestamp_us: u64,
}
```

### Logging

```rust
// ✅ Usar tracing com campos estruturados
use tracing::{info, warn, error, debug, instrument};

#[instrument(skip(frame_data), fields(frame_size = frame_data.len()))]
pub async fn decode_frame(frame_data: &[u8]) -> Result<DecodedFrame, DecoderError> {
    debug!("decoding frame");
    // ...
    info!(latency_ms = elapsed.as_millis(), "frame decoded");
}
```

## GStreamer Integration

### Pipeline Pattern

```rust
// Pipeline de decoding típico
let pipeline_desc = if has_nvdec() {
    "appsrc name=src ! h264parse ! nvh264dec ! videoconvert ! appsink name=sink"
} else if has_vaapi() {
    "appsrc name=src ! h264parse ! vaapih264dec ! videoconvert ! appsink name=sink"  
} else {
    "appsrc name=src ! h264parse ! avdec_h264 ! videoconvert ! appsink name=sink"
};
```

### Best Practices GStreamer em Rust

1. **Sempre checar capabilities** do decoder antes de usar
2. **Configurar `appsrc`** com caps corretos (H.264, framerate, etc.)
3. **Usar `appsink` com callbacks** em vez de polling
4. **Tratar `EOS` e `Error` messages** do bus
5. **Cleanup adequado** — `set_state(Null)` antes de drop

## Performance Rules

- Hot path (decode → render): **zero allocations**
- Usar `jemalloc` como allocator global
- Preferir stack allocation para buffers pequenos (< 4KB)
- Usar `tokio::task::spawn_blocking` para operações CPU-bound
- Profile com `perf` antes de otimizar

## Armadilhas Linux Conhecidas

1. **GStreamer plugins** — verificar instalação: `gst-inspect-1.0 nvh264dec`
2. **NVIDIA drivers** — NVDEC requer driver proprietário, não funciona com nouveau
3. **Wayland vs X11** — testar em ambos; `winit` abstrai mas não completamente
4. **Permissões USB** — pode precisar de udev rules para acesso sem root
5. **Firewall** — mDNS (5353/UDP) e WebRTC (STUN/TURN) podem ser bloqueados
