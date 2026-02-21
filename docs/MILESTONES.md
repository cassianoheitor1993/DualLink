# DualLink — Milestones & Epics

## Milestone 0: Research & Technical Validation
**Deadline:** Semana 3
**Owner:** Solo dev / Arquiteto
**Critério de saída:** Todos os 3 PoCs validados com benchmarks documentados

### Epic 0.1 — Virtual Display Research (macOS)
- **Story 0.1.1:** Como desenvolvedor, quero criar um virtual display via CGVirtualDisplay API para validar que macOS permite displays arbitrários
  - Acceptance: Display aparece em System Preferences > Displays
  - Acceptance: Resolução 1920x1080 configurada
- **Story 0.1.2:** Como desenvolvedor, quero documentar as alternativas (DriverKit, CGDisplayStream) caso CGVirtualDisplay não funcione
  - Acceptance: Documento comparativo com prós/contras
- **Story 0.1.3:** Como desenvolvedor, quero entender as restrições de SIP e permissões necessárias
  - Acceptance: Lista de entitlements e permissões documentada

### Epic 0.2 — Screen Capture + Encoding PoC (macOS)
- **Story 0.2.1:** Como desenvolvedor, quero capturar frames de um display específico usando ScreenCaptureKit
  - Acceptance: CVPixelBuffer capturado a 30fps+
  - Acceptance: Funciona com virtual display
- **Story 0.2.2:** Como desenvolvedor, quero encodar frames em H.264 usando VideoToolbox
  - Acceptance: Hardware encoding confirmado
  - Acceptance: Encoding latency < 5ms por frame
- **Story 0.2.3:** Como desenvolvedor, quero medir CPU/GPU durante captura+encoding
  - Acceptance: Relatório de benchmark documentado

### Epic 0.3 — Decoding + Rendering PoC (Linux)
- **Story 0.3.1:** Como desenvolvedor, quero decodificar H.264 via GPU no Linux
  - Acceptance: VAAPI ou NVDEC funcional
  - Acceptance: Decoding latency < 5ms
- **Story 0.3.2:** Como desenvolvedor, quero renderizar vídeo fullscreen
  - Acceptance: Janela fullscreen sem borders
  - Acceptance: Funciona em Wayland e X11
- **Story 0.3.3:** Como desenvolvedor, quero testar WebRTC entre Mac e Linux
  - Acceptance: Conexão P2P estabelecida
  - Acceptance: Video stream visível

---

## Milestone 1: MVP — Screen Mirroring (Wi-Fi)
**Deadline:** Semana 7
**Critério de saída:** Espelhamento funcional 1080p/30fps com latência < 100ms

### Epic 1.1 — macOS Sender Application
- **Story 1.1.1:** Criar projeto Xcode com Swift Package Manager
- **Story 1.1.2:** Implementar módulo `ScreenCapture` usando ScreenCaptureKit
- **Story 1.1.3:** Implementar módulo `VideoEncoder` usando VideoToolbox
- **Story 1.1.4:** Implementar módulo `WebRTCSender` para streaming
- **Story 1.1.5:** Implementar módulo `SignalingClient` (WebSocket)
- **Story 1.1.6:** Implementar UI SwiftUI: status, start/stop, device list
- **Story 1.1.7:** Integrar pipeline completo: Capture → Encode → Stream

### Epic 1.2 — Linux Receiver Application
- **Story 1.2.1:** Criar projeto Cargo com workspace
- **Story 1.2.2:** Implementar módulo `webrtc_receiver`
- **Story 1.2.3:** Implementar módulo `video_decoder` (GStreamer)
- **Story 1.2.4:** Implementar módulo `renderer` (fullscreen)
- **Story 1.2.5:** Implementar módulo `signaling_client`
- **Story 1.2.6:** Implementar UI Tauri: status, configurações
- **Story 1.2.7:** Integrar pipeline: Receive → Decode → Render

### Epic 1.3 — Shared Protocol
- **Story 1.3.1:** Definir mensagens Protobuf (signaling, control, status)
- **Story 1.3.2:** Implementar discovery via mDNS
- **Story 1.3.3:** Implementar signaling server embarcado
- **Story 1.3.4:** Documentar protocolo

### Epic 1.4 — Integration & QA
- **Story 1.4.1:** Teste end-to-end em rede local
- **Story 1.4.2:** Otimizar parâmetros WebRTC para latência
- **Story 1.4.3:** Implementar reconexão automática
- **Story 1.4.4:** Testes em diferentes condições de rede
- **Story 1.4.5:** Escrever testes automatizados dos módulos

---

## Milestone 2: Extended Display + 60fps
**Deadline:** Semana 11
**Critério de saída:** Monitor virtual funcionando como extensão real a 60fps

### Epic 2.1 — Virtual Display Driver
- **Story 2.1.1:** Implementar gerenciamento completo de CGVirtualDisplay
- **Story 2.1.2:** Suportar múltiplas resoluções (1080p, 1440p, 4K)
- **Story 2.1.3:** Gerenciar lifecycle (criação, destruição, reconexão)
- **Story 2.1.4:** Integrar com ScreenCaptureKit para captura isolada

### Epic 2.2 — Performance 60fps
- **Story 2.2.1:** Otimizar pipeline de captura para 60fps sustentados
- **Story 2.2.2:** Avaliar e possivelmente migrar para H.265
- **Story 2.2.3:** Implementar bitrate adaptativo
- **Story 2.2.4:** Implementar frame pacing e vsync no Linux
- **Story 2.2.5:** Benchmark e otimização contínua

### Epic 2.3 — Input Forwarding
- **Story 2.3.1:** Capturar eventos de mouse e teclado no Linux
- **Story 2.3.2:** Serializar e enviar via WebRTC DataChannel
- **Story 2.3.3:** Receber e injetar eventos no macOS (CGEvent)
- **Story 2.3.4:** Implementar gestos básicos de trackpad
- **Story 2.3.5:** Mapear coordinate system entre displays

---

## Milestone 3: USB-C Mode
**Deadline:** Semana 14
**Critério de saída:** Streaming funcional via USB com latência < 40ms

### Epic 3.1 — USB Transport Layer
- **Story 3.1.1:** Implementar comunicação USB via IOKit (macOS) + libusb (Linux)
- **Story 3.1.2:** Definir framing protocol sobre USB bulk transfer
- **Story 3.1.3:** Benchmark de throughput
- **Story 3.1.4:** Implementar error handling e recovery

### Epic 3.2 — Pipeline Integration
- **Story 3.2.1:** Abstrair transport layer (interface comum USB/Wi-Fi)
- **Story 3.2.2:** Auto-detecção de modo de conexão
- **Story 3.2.3:** Fallback automático USB → Wi-Fi
- **Story 3.2.4:** Testes de estabilidade prolongados

---

## Milestone 4: Security & Distribution
**Deadline:** Semana 16
**Critério de saída:** Produto pronto para distribuição pública

### Epic 4.1 — Security
- **Story 4.1.1:** TLS no canal de signaling
- **Story 4.1.2:** QR Code pairing flow
- **Story 4.1.3:** Certificados de sessão temporários
- **Story 4.1.4:** Criptografia DTLS-SRTP no stream

### Epic 4.2 — Packaging & CI/CD
- **Story 4.2.1:** Build script para .dmg (macOS)
- **Story 4.2.2:** Build script para AppImage (Linux)
- **Story 4.2.3:** GitHub Actions: build + test + release
- **Story 4.2.4:** Documentação de usuário final
- **Story 4.2.5:** README, CONTRIBUTING, LICENSE

---


---

## Milestone 5: Platform Expansion ✅ CONCLUÍDO
**Commits:** 5A `31578ed` → 5G `3c8ec15`
**Critério de saída:** 4 senders funcionando (macOS, Linux, Windows, + receiver Linux) com mDNS auto-discovery e input injection

### Epic 5.1 — Multi-Display Transport (5A)
- **Story 5.1.1:** ✅ `start_all()` no receiver inicia N pipelines paralelos (um por display)
- **Story 5.1.2:** ✅ `DisplayChannels` — bounded channel por display index para frames
- **Story 5.1.3:** ✅ `byte[17]` no DLNK frame header carrega display_index para demux

### Epic 5.2 — Linux Sender (5C)
- **Story 5.2.1:** ✅ `duallink-capture-linux` — PipeWire portal via `ashpd` + GStreamer `pipewiresrc`
- **Story 5.2.2:** ✅ `encoder.rs` — vaapih264enc / nvh264enc / x264enc fallback chain
- **Story 5.2.3:** ✅ `duallink-transport-client` — `SignalingClient` (TLS TCP) + `VideoSender` (UDP)
- **Story 5.2.4:** ✅ `SenderPipeline` — `Arc<Notify>` stop, per-display task (5D)
- **Story 5.2.5:** ✅ `duallink-input-inject` — uinput virtual mouse + keyboard (5D)
- **Story 5.2.6:** ✅ egui settings UI com mDNS receiver picker (5D/5E)

### Epic 5.3 — Windows Sender (5B)
- **Story 5.3.1:** ✅ WGC capture via `windows` crate (`GraphicsCaptureItem`)
- **Story 5.3.2:** ✅ Media Foundation H.264 encoder (`IMFTransform`)
- **Story 5.3.3:** ✅ `SendInput` win32 mouse/keyboard injection
- **Story 5.3.4:** ✅ egui settings UI com mDNS receiver picker

### Epic 5.4 — mDNS Auto-Discovery (5E)
- **Story 5.4.1:** ✅ `DualLinkAdvertiser::register()` em `duallink-discovery` (receiver)
- **Story 5.4.2:** ✅ `detect_local_ip()` — UDP probe trick, sem envio de pacotes
- **Story 5.4.3:** ✅ TXT record: `version`, `displays`, `port`, `host`, `fp`
- **Story 5.4.4:** ✅ mDNS browser + receiver picker no egui dos senders
- **Story 5.4.5:** ✅ mDNS browse via `NWBrowser` no mac-client Swift

### Epic 5.5 — Input Injection Full (5D/5F)
- **Story 5.5.1:** ✅ Linux uinput: `REL_X/Y` mouse, button events, `EV_KEY` keyboard
- **Story 5.5.2:** ✅ macOS CGEvent injection (receiver side em mac-client)
- **Story 5.5.3:** ✅ Windows `SendInput` injection em windows-sender
- **Story 5.5.4:** ✅ `InputEvent` JSON mensagem no back-channel TLS TCP

### Epic 5.6 — Decoder Hot-Reload (5F)
- **Story 5.6.1:** ✅ `pending_config: Option<StreamConfig>` no `AppState`
- **Story 5.6.2:** ✅ `"config_updated"` signal quebra loop de decode sem reiniciar processo
- **Story 5.6.3:** ✅ Reinicialização do decoder na próxima iteração `'reconnect`

### Epic 5.7 — duallink-gui & UX (5F/5G)
- **Story 5.7.1:** ✅ `duallink-gui` crate com egui multi-display panel
- **Story 5.7.2:** ✅ LAN IP e TLS fingerprint exibidos no UI do receiver
- **Story 5.7.3:** ✅ CI jobs para linux-sender e windows-sender (5G)
- **Story 5.7.4:** ✅ Lint fixes e documentação atualizada (5G)

---

## Métricas de Acompanhamento

| Métrica | Target MVP | Target Final |
|---------|:----------:|:------------:|
| Latência (Wi-Fi) | < 100ms | < 80ms |
| Latência (USB) | — | < 40ms |
| FPS | 30 | 60 |
| Resolução | 1080p | Até 4K |
| CPU usage | < 30% | < 25% |
| Uptime | 1h | 8h+ |
| Tempo de setup | < 5min | < 1min |
| Plataformas sender | 1 (macOS) | 3 (macOS+Linux+Windows) |

---

*Documento criado em: 2026-02-20*
*Última atualização: 2026-05-30 (Milestone 5 / Phase 5G)*
