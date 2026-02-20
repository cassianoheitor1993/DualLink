---
applyTo: "shared-protocol/**,**/*.proto"
---

# Design: API Contracts

> Define os contratos de comunicação entre mac-client e linux-receiver.
> Qualquer mudança aqui afeta ambas as plataformas.

## Princípios de API

1. **Contract-first:** Definir contrato ANTES de implementar
2. **Backward compatible:** Nunca quebrar clientes existentes
3. **Versionado:** Toda mensagem pertence a um package versionado
4. **Documentado:** Todo field com comentário explicando propósito
5. **Validado:** Ambos os lados validam mensagens recebidas

## Contratos Atuais

### 1. Discovery (mDNS)

```
Service Type: _duallink._tcp.local.
Port: 8443
TXT Records:
  - version=1
  - name=<device-name>
  - role=sender|receiver
  - capabilities=mirror,extend
```

### 2. Signaling (WebSocket → JSON)

**Endpoint:** `ws://<ip>:8443/signaling`

#### Messages

```typescript
// Base message
{
  "type": "offer" | "answer" | "ice-candidate" | "control" | "status",
  "payload": { ... }
}

// Session Offer (Mac → Linux)
{
  "type": "offer",
  "payload": {
    "session_id": "uuid",
    "sdp": "...",
    "capabilities": {
      "resolutions": ["1920x1080", "2560x1440"],
      "max_fps": 60,
      "codecs": ["h264", "h265"],
      "input_support": true
    }
  }
}

// Session Answer (Linux → Mac)
{
  "type": "answer",
  "payload": {
    "session_id": "uuid",
    "sdp": "...",
    "selected": {
      "resolution": "1920x1080",
      "fps": 30,
      "codec": "h264"
    }
  }
}

// ICE Candidate (bidirectional)
{
  "type": "ice-candidate",
  "payload": {
    "candidate": "...",
    "sdpMid": "...",
    "sdpMLineIndex": 0
  }
}
```

### 3. Control Channel (WebRTC DataChannel)

**Channel name:** `control`
**Serialization:** Protocol Buffers

```protobuf
message StreamControl {
  oneof command {
    PauseStream pause = 1;
    ResumeStream resume = 2;
    ChangeQuality quality = 3;
    RequestKeyframe keyframe = 4;
  }
}

message ChangeQuality {
  uint32 target_fps = 1;
  uint64 max_bitrate_bps = 2;
  Resolution resolution = 3;
}

message StatusReport {
  uint32 current_fps = 1;
  uint32 decode_latency_ms = 2;
  uint32 render_latency_ms = 3;
  uint64 frames_received = 4;
  uint64 frames_dropped = 5;
  float cpu_usage_percent = 6;
}
```

### 4. Input Channel (WebRTC DataChannel)

**Channel name:** `input`
**Serialization:** Protocol Buffers

```protobuf
message InputEvent {
  uint64 timestamp_us = 1;
  oneof event {
    MouseMove mouse_move = 2;
    MouseButton mouse_button = 3;
    MouseScroll mouse_scroll = 4;
    KeyEvent key = 5;
  }
}

message MouseMove {
  float x = 1;  // Normalized 0.0–1.0
  float y = 2;  // Normalized 0.0–1.0
}

message MouseButton {
  uint32 button = 1;  // 0=left, 1=right, 2=middle
  bool pressed = 2;
}

message KeyEvent {
  uint32 keycode = 1;  // Platform-independent keycode
  bool pressed = 2;
  uint32 modifiers = 3;  // Bitmask: shift, ctrl, alt, meta
}
```

### 5. Video Stream (WebRTC Media Track)

```
Codec: H.264 (Baseline Profile) — MVP
       H.265 (Main Profile) — Fase 2
       
RTP payload type: dynamic (negotiated via SDP)
Clock rate: 90000

Encoding parameters:
  - Real-time encoding
  - No B-frames (AllowFrameReordering: false)
  - Keyframe interval: 2 seconds
  - Bitrate: adaptive (1–20 Mbps)
```

## Regras de Evolução

1. **Adicionar fields** é backward compatible (campos novos = 0/empty por default)
2. **Remover fields** requer `reserved` no proto
3. **Mudar tipo de field** é BREAKING — requer novo field
4. **Bump de versão** para mudanças semânticas (mesmo formato, significado diferente)
5. **Testar serialização** em ambas as plataformas antes de merge
