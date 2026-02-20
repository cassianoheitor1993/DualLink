---
applyTo: "linux-receiver/**"
---

# Golden Tips — Linux

> Lições aprendidas ao trabalhar com Linux, Rust, GStreamer, VAAPI, NVDEC, Wayland/X11.
> Consultar ANTES de debugar qualquer problema no linux-receiver.

---

### GT-2001: AMD Radeon 680M (iGPU) via VA-API é mais rápido que NVIDIA NVDEC no Legion 5 Pro
- **Data:** 2026-02-20
- **Contexto:** Sprint 0.3 — probe.sh no Lenovo Legion 5 Pro (RTX 3060 + Radeon 680M)
- **Sintoma:** Assumiamos que NVDEC seria o decoder mais rápido
- **Causa raiz:** AMD Radeon 680M é um iGPU dedicado a tarefas de mídia; o driver Mesa Gallium (radeonsi) tem VA-API H.264 hardware decode otimizado para conteúdo 1080p
- **Solução:** Usar `vaapih264dec` como PRIMARY decoder, não `nvh264dec`
- **Resultados medidos (1920×1080 @ 30fps, 300 frames):**
  - `vaapih264dec`: **5.1ms** avg (fastest — AMD iGPU VA-API)
  - `vaapidecodebin`: **5.5ms** avg (VA-API auto-select)
  - `nvh264dec`: **6.0ms** avg (NVIDIA NVDEC)
  - `avdec_h264`: **16.8ms** avg (software — 3ms from budget limit)
- **Pista-chave:** Legion 5 tem dois GPUs. Verificar `vainfo` antes de assumir NVIDIA é melhor.
- **Config do sistema:** GStreamer 1.24.2, Mesa 25.2.8, Driver radeonsi, NVIDIA 535.274.02
- **Decoder priority para duallink-decoder:**
  1. `vaapih264dec` — PRIMARY (AMD iGPU via VA-API)
  2. `vaapidecodebin` — VA-API auto (fallback se vaapih264dec não disponível)
  3. `nvh264dec` — NVIDIA NVDEC
  4. `avdec_h264` — software (last resort, barely fits 20ms budget)
- **Tags:** #decoder #vaapi #nvdec #gstreamer #performance #amd

---

**Total de tips:** 1
**Última atualização:** 2026-02-20
**Economia estimada:** 1 hora
