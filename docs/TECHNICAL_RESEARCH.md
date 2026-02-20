# DualLink — Technical Research Notes

## 1. Virtual Display no macOS

### Opção A: CGVirtualDisplay (Recomendada)
- **Disponível a partir de:** macOS 14 (Sonoma)
- **Framework:** CoreGraphics (private → tornando-se public)
- **Como funciona:**
  - `CGVirtualDisplay` permite criar displays virtuais que o sistema trata como monitores reais
  - Aparecem em System Preferences > Displays
  - Suportam drag-and-drop de janelas
  - Podem ter resolução e refresh rate customizados
- **Entitlements necessários:**
  - `com.apple.developer.virtual-display` (possivelmente)
  - Investigar se funciona sem entitlement especial em apps não-sandboxed
- **Riscos:**
  - API pode ser restrita ou mudar em versões futuras
  - Documentação escassa

### Opção B: DriverKit Virtual Display
- **Complexidade:** Alta
- **Requer:** Conta de desenvolvedor Apple paga
- **Como funciona:**
  - Criar um DriverKit extension que emula um display
  - Registra-se como display provider no IOKit
  - Mais estável e oficial que CGVirtualDisplay
- **Prós:** Mais controle, mais estável
- **Contras:** Muito mais complexo, requer aprovação da Apple para distribuição

### Opção C: Headless Display Emulators (Hardware)
- **Dongle HDMI/USB-C dummy** que engana o macOS
- **Prós:** Zero software necessário para o display virtual
- **Contras:** Requer hardware extra, menos flexível

### Decisão: Começar com CGVirtualDisplay, ter DriverKit como plano B.

---

## 2. Screen Capture no macOS

### ScreenCaptureKit (Recomendado)
- **Disponível a partir de:** macOS 12.3
- **Performance:** Excelente — hardware-accelerated
- **APIs chave:**
  - `SCShareableContent` — listar displays/windows
  - `SCContentFilter` — filtrar o que capturar
  - `SCStream` — stream contínuo de frames
  - `SCStreamOutput` — delegate para receber CMSampleBuffers
- **Output:** `CMSampleBuffer` contendo `CVPixelBuffer` (IOSurface-backed)
- **Latência:** Sub-millisecond para captura
- **Requisitos:** Permissão de Screen Recording

### Alternativas
- `CGDisplayStream` — mais antigo, menos eficiente
- `AVCaptureScreenInput` — deprecated
- `CGWindowListCreateImage` — muito lento para streaming

### Decisão: ScreenCaptureKit é a escolha clara.

---

## 3. Video Encoding no macOS

### VideoToolbox (Recomendado)
- **Hardware encoding:** Sim (Apple Silicon Media Engine)
- **Codecs suportados:**
  - H.264 (AVC) — universalmente suportado
  - H.265 (HEVC) — melhor compressão, mais pesado
- **APIs chave:**
  - `VTCompressionSession` — sessão de encoding
  - `VTCompressionSessionEncodeFrame` — encodar frame
- **Configurações importantes para baixa latência:**
  ```
  kVTCompressionPropertyKey_RealTime: true
  kVTCompressionPropertyKey_AllowFrameReordering: false
  kVTCompressionPropertyKey_MaxKeyFrameInterval: 60
  kVTCompressionPropertyKey_ProfileLevel: H264_Baseline_AutoLevel
  ```
- **Performance esperada:** < 3ms por frame em Apple Silicon

### Decisão: VideoToolbox com H.264 para MVP, avaliar H.265 na Fase 2.

---

## 4. WebRTC

### Google WebRTC (via C++ ou wrappers)
- **macOS:** Usar via Swift wrapper ou compilar native
- **Linux:** webrtc-rs (Rust) ou GStreamer webrtcbin
- **Vantagens:**
  - NAT traversal embutido (ICE/STUN/TURN)
  - DTLS-SRTP encryption embutida
  - Adaptive bitrate
  - Congestion control
- **Desvantagens:**
  - Overhead de setup
  - Pode adicionar latência vs raw UDP
- **Tuning para baixa latência:**
  - Desabilitar buffering
  - Usar Jitter Buffer mínimo
  - Preferir UDP
  - Configurar `maxBitrate` adequadamente
  - Usar `encodedInsertableStreams` se preciso

### Alternativa: GStreamer + custom UDP
- Mais controle sobre latência
- Mais trabalho de implementação
- Sem NAT traversal built-in

### Decisão: WebRTC para MVP (praticidade), avaliar custom UDP se latência insuficiente.

---

## 5. Video Decoding no Linux

### VAAPI (Intel/AMD)
- **Suporte:** Amplo em Linux
- **GStreamer element:** `vaapih264dec`
- **Performance:** Excelente para H.264

### NVDEC (NVIDIA)
- **Para:** Lenovo Legion 5 Pro (RTX 3070/4060)
- **GStreamer element:** `nvh264dec` (via nvidia-plugins)
- **Performance:** Excelente
- **Requer:** NVIDIA drivers proprietários

### GStreamer Pipeline Exemplo
```
webrtcbin → rtph264depay → h264parse → nvh264dec → glimagesink
```

### Decisão: Suportar NVDEC (primary para Legion) e VAAPI (fallback).

---

## 6. Rendering no Linux

### Wayland
- **Recomendado para:** Futuro
- **Renderer:** `wl_surface` + EGL
- **GStreamer sink:** `waylandsink` ou `gtkwaylandsink`

### X11
- **Recomendado para:** Compatibilidade
- **GStreamer sink:** `xvimagesink` ou `glimagesink`

### Abordagem Recomendada
- Usar `glimagesink` do GStreamer (funciona em ambos)
- Ou usar `wgpu` em Rust para rendering agnóstico

### Decisão: GStreamer video sink para MVP, considerar wgpu para mais controle.

---

## 7. USB-C Communication

### USB Networking (CDC-NCM)
- **Como funciona:** USB-C funciona como interface de rede
- **Prós:** macOS e Linux suportam nativamente, funciona como Ethernet
- **Contras:** Adiciona overhead de rede TCP/IP
- **Latência:** Boa (< 1ms de overhead de transporte)
- **Setup:** Pode requerer configuração de IP manual

### USB Bulk Transfer (Raw)
- **Como funciona:** Comunicação direta device-to-device
- **macOS:** IOKit USB APIs
- **Linux:** libusb
- **Prós:** Mínimo overhead, máxima performance
- **Contras:** Muito mais complexo, requer USB gadget mode no Linux
- **Requisito:** Um dos dispositivos precisa ser USB device (gadget mode)

### USB Gadget Mode no Linux
- O Lenovo Legion pode funcionar como USB device (gadget) via `configfs`
- Criar uma função `ffs` (FunctionFS) para custom protocol
- Ou usar `g_ether` para Ethernet sobre USB

### Decisão:
1. **Fase 3 inicial:** Tentar CDC-NCM (Ethernet over USB) — mínimo esforço
2. **Otimização:** Se latência insuficiente, migrar para USB bulk transfer

---

## 8. mDNS Discovery

### macOS
- **API:** `NWBrowser` (Network.framework) ou `NetServiceBrowser`
- **Protocolo:** Bonjour (mDNS nativo)

### Linux
- **Biblioteca:** `mdns-sd` crate em Rust, ou Avahi
- **Compatível com Bonjour:** Sim

### Service Type
```
_duallink._tcp.local.
```

---

## 9. Comparativo de Latência Esperada

| Componente | Tempo Estimado |
|-----------|:-------------:|
| Screen Capture (ScreenCaptureKit) | ~1-2ms |
| Encoding H.264 (VideoToolbox) | ~2-4ms |
| Network Transport (Wi-Fi LAN) | ~5-15ms |
| Network Transport (USB-C NCM) | ~1-3ms |
| Jitter Buffer (WebRTC) | ~10-30ms |
| Decoding H.264 (NVDEC) | ~2-4ms |
| Rendering | ~1-2ms |
| **Total Wi-Fi** | **~22-57ms** |
| **Total USB** | **~8-16ms** |

> Nota: Jitter buffer é o maior contribuinte para latência Wi-Fi.
> WebRTC tuning pode reduzir significativamente.

---

*Documento criado em: 2026-02-20*
