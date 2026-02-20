# Sprint 0.3 — GStreamer Decode PoC (Linux)

**Máquina alvo:** Lenovo Legion 5 Pro (NVIDIA GeForce RTX 3070 / AMD Ryzen 9 5900HX)
**OS:** Ubuntu 22.04+ / Fedora 40+ / Arch Linux

---

## Objetivo

Validar que o Linux receiver pode:
1. Receber frames H.264 e decodificar via hardware (NVDEC / VAAPI)
2. Exibir na tela com latência < 20ms (decode + render)
3. Sustentar 1080p@30fps sem drops

**Pergunta-chave:** Qual decoder tem menor latência para H.264 1080p@30fps:
- `nvdec` (NVIDIA hardware, zero-copy) ← preferencial (Legion tem RTX 3070)
- `vaapidecodebin` (VA-API, Intel/AMD, vendor-neutral)
- `avdec_h264` (software fallback, baseline mínimo)

---

## Setup

### 1. Instalar dependências

```bash
chmod +x setup.sh && ./setup.sh
```

### 2. Probe rápido (CLI — não precisa compilar Rust)

```bash
# Testa todos os decoders disponíveis e mede FPS
chmod +x probe.sh && ./probe.sh
```

### 3. Benchmark de latência Rust (mais preciso)

```bash
cargo run --release 2>&1 | tee results.txt
```

---

## Estrutura

```
poc/poc-gstreamer/
├── README.md         ← este arquivo
├── setup.sh          ← instala GStreamer + plugins
├── probe.sh          ← probe CLI com gst-launch-1.0
├── Cargo.toml        ← benchmark Rust com gstreamer-rs
└── src/
    └── main.rs       ← medição de latência por frame
```

---

## Resultados esperados (Lenovo Legion 5 Pro)

| Decoder | Latência esperada | Notas |
|---------|-------------------|-------|
| `nvdec` | ~2-5ms | NVDEC bypass CPU — zero-copy com EGLImage |
| `vaapi` | ~5-15ms | AMD iGPU ou NVIDIA via mesa |
| `avdec_h264` | ~8-20ms | CPU-bound, baseline de comparação |

**Critério de sucesso:** qualquer decoder < 20ms @ 1080p/30fps sustentado.

---

## Saída esperada

```
=== GStreamer Decode PoC ===
GStreamer: 1.22.x

[1/3] Checking decoders...
  ✅ nvdec          — NVIDIA hardware H.264 decode
  ✅ vaapidecodebin — VA-API hardware decode
  ✅ avdec_h264     — Software (libavcodec) fallback

[2/3] Running latency benchmarks (300 frames each)...
  nvdec:        avg= 3.2ms  p99= 6.1ms  fps=30.0  ✅
  vaapidecodebin: avg= 9.4ms  p99=14.2ms  fps=29.8  ✅
  avdec_h264:   avg=11.8ms  p99=18.3ms  fps=29.6  ✅

[3/3] Decisions...
  ✅ Hardware decode viable — use nvdec as primary
  ✅ vaapidecodebin as fallback
  ✅ avdec_h264 as last resort
  → Implement duallink-decoder with DecoderFactory::best_available()
```

---

## Próximos passos após validação

- [ ] Implementar `duallink-decoder` crate com NVDEC + VAAPI + SW fallback
- [ ] Integrar com `duallink-transport` (receber RTP H.264 via WebRTC)
- [ ] Benchmark end-to-end: captura macOS → encode → transmit → decode Linux
