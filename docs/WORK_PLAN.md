# DualLink — Plano de Trabalho

> **Objetivo:** Transformar um laptop Linux (Lenovo Legion 5 Pro) em monitor externo para macOS (MacBook Pro) via USB-C ou Wi-Fi, com baixa latência e aceleração por GPU.

---

## Índice

1. [Visão Geral das Fases](#visão-geral-das-fases)
2. [Fase 0 — Research & Validação Técnica](#fase-0--research--validação-técnica)
3. [Fase 1 — MVP: Espelhamento via Wi-Fi](#fase-1--mvp-espelhamento-via-wi-fi)
4. [Fase 2 — Extensão Real de Tela](#fase-2--extensão-real-de-tela)
5. [Fase 3 — Modo USB-C](#fase-3--modo-usb-c)
6. [Fase 4 — Polish & Packaging](#fase-4--polish--packaging)
7. [Backlog Detalhado](#backlog-detalhado)
8. [Riscos & Mitigações](#riscos--mitigações)
9. [Critérios de Sucesso](#critérios-de-sucesso)
10. [Stack & Ferramentas](#stack--ferramentas)

---

## Visão Geral das Fases

```
Fase 0 ─── Research & PoC ──────────────────── ~3 semanas
Fase 1 ─── MVP Wi-Fi (Espelhamento) ────────── ~4 semanas
Fase 2 ─── Extensão de Tela + 60fps ────────── ~4 semanas
Fase 3 ─── Modo USB-C ──────────────────────── ~3 semanas
Fase 4 ─── Polish, Packaging & Security ────── ~2 semanas
                                         Total: ~16 semanas
```

---

## Fase 0 — Research & Validação Técnica

**Duração estimada:** 3 semanas
**Objetivo:** Validar viabilidade técnica dos componentes críticos antes de investir em implementação.

### Sprint 0.1 — Pesquisa de Virtual Display no macOS (Semana 1)

| # | Tarefa | Resultado Esperado |
|---|--------|--------------------|
| 0.1.1 | Pesquisar CGVirtualDisplay API (macOS 14+) | Documentação + código de exemplo funcional |
| 0.1.2 | Avaliar DriverKit como alternativa | Comparativo CGVirtualDisplay vs DriverKit |
| 0.1.3 | Testar criação de display virtual com resolução customizada | PoC rodando: display virtual 1920x1080 criado |
| 0.1.4 | Investigar limitações de SIP/permissões | Documento de requisitos de sistema |

### Sprint 0.2 — PoC de Captura + Encoding no macOS (Semana 2)

| # | Tarefa | Resultado Esperado |
|---|--------|--------------------|
| 0.2.1 | PoC ScreenCaptureKit — capturar frames de um display | App capturando frames em tempo real |
| 0.2.2 | PoC VideoToolbox — encoding H.264 hardware | Pipeline: frame → H.264 em < 5ms |
| 0.2.3 | Medir performance (CPU, GPU, latência de encode) | Benchmark documentado |
| 0.2.4 | Testar captura do display virtual criado em 0.1 | Validar que ScreenCaptureKit funciona com virtual display |

### Sprint 0.3 — PoC de Decoding + Rendering no Linux (Semana 3)

| # | Tarefa | Resultado Esperado |
|---|--------|--------------------|
| 0.3.1 | PoC GStreamer — decoder H.264 via VAAPI/NVDEC | Stream de teste decodificando em GPU |
| 0.3.2 | PoC rendering fullscreen via Wayland/X11 | Janela fullscreen renderizando vídeo |
| 0.3.3 | Testar WebRTC básico entre duas máquinas | Conexão WebRTC ponto-a-ponto funcionando |
| 0.3.4 | Medir latência end-to-end (frame captura → pixel na tela) | Benchmark < 100ms Wi-Fi |

**Gate de decisão:** Se os 3 PoCs funcionarem, prosseguir para Fase 1.

---

## Fase 1 — MVP: Espelhamento via Wi-Fi

**Duração estimada:** 4 semanas
**Objetivo:** Espelhamento funcional da tela do macOS em um Linux via Wi-Fi.

### Sprint 1.1 — macOS Sender Core (Semana 4-5)

| # | Tarefa | Resultado Esperado |
|---|--------|--------------------|
| 1.1.1 | Criar projeto Swift do mac-client | Projeto Xcode configurado |
| 1.1.2 | Implementar ScreenCapture module (ScreenCaptureKit) | Módulo capturando frames continuamente |
| 1.1.3 | Implementar VideoEncoder module (VideoToolbox H.264) | Encoding hardware funcional |
| 1.1.4 | Implementar WebRTC signaling client | Conexão WebRTC estabelecida |
| 1.1.5 | Integrar pipeline: Capture → Encode → WebRTC Send | Stream de vídeo saindo do Mac |
| 1.1.6 | UI básica: botão start/stop, status de conexão | Interface mínima |

### Sprint 1.2 — Linux Receiver Core (Semana 4-5, paralelo)

| # | Tarefa | Resultado Esperado |
|---|--------|--------------------|
| 1.2.1 | Criar projeto Rust do linux-receiver | Cargo project configurado |
| 1.2.2 | Implementar WebRTC signaling + receiver | Recebendo stream WebRTC |
| 1.2.3 | Implementar VideoDecoder module (GStreamer + VAAPI/NVDEC) | Decodificando H.264 em GPU |
| 1.2.4 | Implementar Renderer fullscreen (Wayland/X11) | Vídeo exibido em tela cheia |
| 1.2.5 | Integrar pipeline: WebRTC Recv → Decode → Render | Pipeline completo funcional |
| 1.2.6 | UI básica com Tauri: status, configurações | Interface mínima |

### Sprint 1.3 — Protocolo Compartilhado & Signaling (Semana 4-5, paralelo)

| # | Tarefa | Resultado Esperado |
|---|--------|--------------------|
| 1.3.1 | Definir protocolo de signaling (JSON/Protobuf) | Schema definido |
| 1.3.2 | Implementar discovery via mDNS/Bonjour | Dispositivos se encontram na rede |
| 1.3.3 | Implementar handshake de conexão | Pairing funcional |
| 1.3.4 | Definir mensagens de controle (resolução, fps, etc.) | Protocolo documentado |

### Sprint 1.4 — Integração & Testes (Semana 6-7)

| # | Tarefa | Resultado Esperado |
|---|--------|--------------------|
| 1.4.1 | Teste end-to-end: Mac → Linux espelhamento | Espelhamento funcionando |
| 1.4.2 | Otimizar latência WebRTC (ICE, DTLS tuning) | Latência < 80ms Wi-Fi |
| 1.4.3 | Ajustar resolução dinâmica (1080p target) | Stream estável em 1080p/30fps |
| 1.4.4 | Tratar reconexão automática | Reconecta em caso de queda |
| 1.4.5 | Testes em diferentes condições de rede | Relatório de qualidade |

**Entregável Fase 1:** App funcional que espelha tela do Mac em Linux via Wi-Fi.

---

## Fase 2 — Extensão Real de Tela

**Duração estimada:** 4 semanas
**Objetivo:** Criar um display virtual no macOS que funcione como monitor real secundário, com 60fps.

### Sprint 2.1 — Virtual Display Driver (Semana 8-9)

| # | Tarefa | Resultado Esperado |
|---|--------|--------------------|
| 2.1.1 | Implementar CGVirtualDisplay integration completa | Display virtual aparece em System Preferences |
| 2.1.2 | Configurar resoluções suportadas (1080p, 1440p, 4K) | Resolução selecionável |
| 2.1.3 | Gerenciar lifecycle do display (create/destroy) | Display criado/removido limpo |
| 2.1.4 | Implementar captura específica do display virtual | Capturando apenas o display virtual |
| 2.1.5 | Testar com apps reais (arrastar janelas, etc.) | macOS trata como monitor real |

### Sprint 2.2 — Upgrade para 60fps (Semana 9-10)

| # | Tarefa | Resultado Esperado |
|---|--------|--------------------|
| 2.2.1 | Otimizar pipeline de captura para 60fps | Captura sustentada em 60fps |
| 2.2.2 | Avaliar H.265 vs H.264 para 60fps | Decisão documentada |
| 2.2.3 | Ajustar bitrate adaptativo | Qualidade estável com banda variável |
| 2.2.4 | Otimizar decoder Linux para 60fps | Rendering suave em 60fps |
| 2.2.5 | Frame pacing e vsync | Sem tearing ou stuttering |

### Sprint 2.3 — Input Forwarding (Semana 10-11)

| # | Tarefa | Resultado Esperado |
|---|--------|--------------------|
| 2.3.1 | Capturar mouse/teclado no Linux receiver | Eventos de input capturados |
| 2.3.2 | Enviar eventos de input via canal de dados WebRTC | Input transmitido ao Mac |
| 2.3.3 | Injetar eventos no macOS (CGEvent) | Mouse/teclado funcionando no display virtual |
| 2.3.4 | Suportar gestos de trackpad básicos | Scroll e zoom funcionando |

**Entregável Fase 2:** Monitor externo real via Wi-Fi com input bidirecional a 60fps.

---

## Fase 3 — Modo USB-C

**Duração estimada:** 3 semanas
**Objetivo:** Streaming de alta performance via USB-C para latência mínima.

### Sprint 3.1 — Transporte USB (Semana 12-13)

| # | Tarefa | Resultado Esperado |
|---|--------|--------------------|
| 3.1.1 | Pesquisar USB bulk transfer macOS ↔ Linux | Abordagem técnica definida |
| 3.1.2 | Implementar canal USB via libusb/IOKit | Transferência de dados via USB |
| 3.1.3 | Implementar framing protocol sobre USB | Pacotes de vídeo sobre USB |
| 3.1.4 | Benchmark: throughput USB-C (target > 2Gbps) | Throughput medido e documentado |

### Sprint 3.2 — Integração USB no Pipeline (Semana 13-14)

| # | Tarefa | Resultado Esperado |
|---|--------|--------------------|
| 3.2.1 | Substituir transport layer (WebRTC → USB) | Pipeline com USB funcional |
| 3.2.2 | Manter fallback para Wi-Fi | Auto-detecção USB vs Wi-Fi |
| 3.2.3 | Otimizar latência USB (target < 40ms) | Latência medida e validada |
| 3.2.4 | Testar estabilidade em uso prolongado | 8h de uso contínuo sem crash |

**Entregável Fase 3:** Modo USB-C funcionando com latência < 40ms.

---

## Fase 4 — Polish & Packaging

**Duração estimada:** 2 semanas
**Objetivo:** Tornar o produto utilizável, seguro e distribuível.

### Sprint 4.1 — Segurança (Semana 15)

| # | Tarefa | Resultado Esperado |
|---|--------|--------------------|
| 4.1.1 | Implementar TLS para canal de controle | Comunicação encriptada |
| 4.1.2 | QR Code pairing | Pairing seguro e fácil |
| 4.1.3 | Certificados temporários por sessão | Sem certificados persistentes |
| 4.1.4 | Criptografia end-to-end no stream | Dados protegidos |

### Sprint 4.2 — Packaging & Distribuição (Semana 16)

| # | Tarefa | Resultado Esperado |
|---|--------|--------------------|
| 4.2.1 | Criar installer .dmg para macOS | Instalação drag-and-drop |
| 4.2.2 | Criar AppImage para Linux | Binário portátil |
| 4.2.3 | Documentação de usuário (README, quickstart) | Docs completos |
| 4.2.4 | CI/CD com GitHub Actions | Build automatizado |
| 4.2.5 | Testes de regressão automatizados | Suite de testes rodando |

**Entregável Fase 4:** Produto pronto para distribuição.

---

## Backlog Detalhado

### Prioridade Alta (Must Have)

- [ ] Virtual display no macOS
- [ ] Screen capture via ScreenCaptureKit
- [ ] Hardware encoding H.264 via VideoToolbox
- [ ] WebRTC streaming sender (macOS)
- [ ] WebRTC streaming receiver (Linux)
- [ ] GPU decoding via VAAPI ou NVDEC (Linux)
- [ ] Fullscreen rendering (Linux)
- [ ] Device discovery (mDNS)
- [ ] Conexão/desconexão limpa
- [ ] UI básica em ambas as plataformas

### Prioridade Média (Should Have)

- [ ] Extensão de tela (monitor secundário real)
- [ ] 60fps
- [ ] Input forwarding (mouse + teclado)
- [ ] Modo USB-C
- [ ] Adaptive bitrate
- [ ] Reconexão automática
- [ ] Seleção de resolução

### Prioridade Baixa (Nice to Have)

- [ ] Multi-monitor
- [ ] H.265 encoding
- [ ] CLI mode
- [ ] Audio streaming
- [ ] HiDPI / Retina support
- [ ] Modo escuro / tema
- [ ] Métricas de performance no app

---

## Riscos & Mitigações

| # | Risco | Probabilidade | Impacto | Mitigação |
|---|-------|:------------:|:-------:|-----------|
| R1 | macOS restringe virtual displays (SIP, permissões) | Alta | **Crítico** | Testar CGVirtualDisplay primeiro; fallback para DriverKit; considerar captura da tela principal como degradação graciosa |
| R2 | Latência alta no Wi-Fi | Média | Alto | WebRTC tuning (ICE, codec settings); UDP custom como alternativa; bitrate adaptativo |
| R3 | USB bulk transfer complexo entre macOS/Linux | Média | Alto | Validar PoC antes da Fase 3; considerar USB NCM (network over USB) como alternativa |
| R4 | GStreamer/VAAPI instável em diferentes GPUs Linux | Média | Médio | Testar com NVIDIA (NVDEC) e AMD (VAAPI); ter fallback para software decoding |
| R5 | Performance de encoding degrada em uso prolongado | Baixa | Médio | Monitorar thermal throttling; implementar adaptive quality |
| R6 | ScreenCaptureKit não captura virtual display | Média | **Crítico** | Testar no Sprint 0.2; alternativa: IOSurface capture |
| R7 | Wayland vs X11 fragmentation no Linux | Média | Médio | Abstrair renderer; suportar ambos via GStreamer sink |

---

## Critérios de Sucesso

### MVP (Fase 1)
- ✅ Espelhamento funcional Mac → Linux via Wi-Fi
- ✅ Resolução mínima 1080p
- ✅ 30fps sustentados
- ✅ Latência < 100ms
- ✅ CPU < 30% em ambas as máquinas
- ✅ Setup em < 2 minutos

### Produto Completo (Fase 4)
- ✅ Extensão de tela funcional (monitor secundário)
- ✅ 60fps sustentados
- ✅ Latência < 40ms (USB) / < 80ms (Wi-Fi)
- ✅ CPU < 25%
- ✅ 8h de uso contínuo sem crash
- ✅ Conexão USB ou Wi-Fi auto-detectada
- ✅ Instalação em 1 clique por plataforma
- ✅ Comunicação encriptada

---

## Stack & Ferramentas

### macOS Client
| Componente | Tecnologia |
|-----------|-----------|
| Linguagem | Swift |
| Captura de tela | ScreenCaptureKit |
| Encoding | VideoToolbox (H.264/H.265) |
| Virtual Display | CGVirtualDisplay (macOS 14+) |
| Networking | Network.framework |
| WebRTC | Google WebRTC (via SPM) |
| Build | Xcode + SPM |

### Linux Receiver
| Componente | Tecnologia |
|-----------|-----------|
| Linguagem | Rust |
| Decoding | GStreamer + VAAPI/NVDEC |
| Rendering | wgpu ou GStreamer video sink |
| WebRTC | webrtc-rs ou GStreamer webrtcbin |
| UI | Tauri v2 |
| Build | Cargo |

### Shared Protocol
| Componente | Tecnologia |
|-----------|-----------|
| Serialização | Protocol Buffers |
| Discovery | mDNS (Bonjour/Avahi) |
| Signaling | WebSocket + JSON |
| Streaming | WebRTC (DTLS-SRTP) |

### DevOps
| Componente | Tecnologia |
|-----------|-----------|
| CI/CD | GitHub Actions |
| Containers | Docker (para testes Linux) |
| Docs | Markdown no repo |
| Versionamento | SemVer |

---

## Cronograma Visual

```
Semana  1  2  3  4  5  6  7  8  9  10  11  12  13  14  15  16
        ├──────────┤
         Fase 0: Research
                  ├───────────────┤
                   Fase 1: MVP Wi-Fi
                                    ├───────────────┤
                                     Fase 2: Extensão
                                                      ├──────────┤
                                                       Fase 3: USB
                                                                  ├──────┤
                                                                   Fase 4
```

---

## Próximo Passo Imediato

> **Começar pela Tarefa 0.1.1:** Pesquisar e prototipar `CGVirtualDisplay` no macOS.
> Este é o maior risco técnico e precisa ser validado antes de tudo.

---

*Documento criado em: 2026-02-20*
*Última atualização: 2026-02-20*
