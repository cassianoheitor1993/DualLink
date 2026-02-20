---
applyTo: "**"
---

# Design: Architecture Principles

> Princípios arquiteturais que guiam todas as decisões de design do DualLink.

## Princípios Fundamentais

### 1. Pipeline First

O DualLink é fundamentalmente um **pipeline de processamento de vídeo**. Toda decisão deve preservar a eficiência do pipeline.

```
Capture → Encode → Transport → Decode → Render
```

**Regras do pipeline:**
- Cada estágio tem **uma responsabilidade** e **uma interface clara**
- Dados fluem em **uma direção** (exceto controle/input que flui ao contrário)
- **Zero-copy** entre estágios quando possível (passar referências, não cópias)
- Cada estágio pode ser **testado e benchmarkado** isoladamente

### 2. Abstraction at Boundaries

Abstrair nos pontos de variabilidade, ser concreto nos internos:

```
Abstrato (interface):          Concreto (implementação):
├── TransportLayer             ├── WebRTCTransport
│                              ├── USBTransport
├── VideoDecoder               ├── NVDECDecoder
│                              ├── VAAPIDecoder
│                              ├── SoftwareDecoder
├── Renderer                   ├── WaylandRenderer
│                              └── X11Renderer
```

### 3. Graceful Degradation

Sempre ter um fallback funcional:

| Componente | Preferido | Fallback |
|-----------|-----------|----------|
| Transport | USB-C | Wi-Fi |
| Encoding | H.265 (Hardware) | H.264 (Hardware) |
| Decoding | NVDEC | VAAPI → Software |
| Display | Wayland | X11 |
| Resolução | 4K | 1440p → 1080p |
| FPS | 60 | 30 |

### 4. Fail Fast, Recover Gracefully

- **Detectar erros** o mais cedo possível
- **Propagar erros** com contexto — nunca silenciar
- **Recuperar** automaticamente quando possível (reconexão, re-encoding)
- **Informar o usuário** quando recoverypermanente não é possível

### 5. Measure Everything

Instrumentar o pipeline para sempre saber:
- Latência de cada estágio
- FPS efetivo
- CPU/GPU usage
- Frame drops
- Qualidade de conexão

## Boundaries entre Módulos

### macOS Client — Módulos

```
┌─────────────────────────────────────────────┐
│ App (Lifecycle, UI)                         │
├─────────────┬───────────┬───────────────────┤
│  Discovery  │ Signaling │   VirtualDisplay  │
├─────────────┴───────────┴───────────────────┤
│ ScreenCapture → VideoEncoder → Transport    │
├─────────────────────────────────────────────┤
│ InputInjection (← recebe do Linux)          │
└─────────────────────────────────────────────┘
```

### Linux Receiver — Módulos

```
┌─────────────────────────────────────────────┐
│ App (Lifecycle, UI — Tauri)                 │
├─────────────┬───────────────────────────────┤
│  Discovery  │         Signaling             │
├─────────────┴───────────────────────────────┤
│ Transport → VideoDecoder → Renderer         │
├─────────────────────────────────────────────┤
│ InputCapture (→ envia para macOS)           │
└─────────────────────────────────────────────┘
```

### Comunicação entre Módulos

- **Dentro de um app:** Mensagens via channels/async (sem shared mutable state)
- **Entre apps:** Protocol Buffers via WebRTC DataChannel (controle) + Media Track (vídeo)

## Regras de Dependência

```
                    shared-protocol
                     ↗          ↖
              mac-client    linux-receiver
              
- mac-client NÃO depende de linux-receiver
- linux-receiver NÃO depende de mac-client
- Ambos dependem de shared-protocol
- shared-protocol NÃO depende de nenhum dos dois
```

## Anti-patterns Proibidos

- ❌ **Circular dependency** entre módulos
- ❌ **God object** que sabe de tudo
- ❌ **Shared mutable state** entre threads/tasks
- ❌ **Leaky abstraction** — transport layer vazando para video pipeline
- ❌ **Premature abstraction** — não abstrair antes de ter 2+ implementações
- ❌ **Buffer copy chain** — frame copiado 3+ vezes no pipeline
