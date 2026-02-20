---
applyTo: "shared-protocol/**"
---

# Platform: Shared Protocol

> Carregado automaticamente ao editar arquivos em `shared-protocol/`.

## Visão Geral

O shared-protocol define os **contratos de comunicação** entre mac-client e linux-receiver. Ambos os lados dependem deste módulo — mudanças aqui afetam ambas as plataformas.

## Estrutura

```
shared-protocol/
├── proto/                      # Definições Protocol Buffers
│   ├── signaling.proto         # Mensagens de signaling (SDP, ICE)
│   ├── control.proto           # Mensagens de controle (resolução, fps)
│   ├── discovery.proto         # Service advertisement
│   └── input.proto             # Eventos de input (mouse, teclado)
├── src/                        # Código gerado (Rust) — para referência
└── docs/
    └── PROTOCOL.md             # Documentação do protocolo
```

## Protocol Buffers

### Convenções

```protobuf
// Usar proto3
syntax = "proto3";

// Package com domínio do projeto
package duallink.signaling.v1;

// Naming: PascalCase para messages, SCREAMING_SNAKE para enums
message SessionOffer {
    string session_id = 1;
    string sdp = 2;
    Resolution resolution = 3;
    uint32 target_fps = 4;
}

enum ConnectionMode {
    CONNECTION_MODE_UNSPECIFIED = 0;
    CONNECTION_MODE_WIFI = 1;
    CONNECTION_MODE_USB = 2;
}
```

### Regras

1. **Versionamento:** Prefixar packages com `v1`, `v2`, etc.
2. **Backward compatibility:** Nunca remover ou renumerar campos — marcar como `reserved`
3. **Defaults claros:** Enums devem ter valor 0 como UNSPECIFIED
4. **Documentação:** Comentar every message e field com `//`
5. **Tamanhos:** Mensagens de controle < 1KB; video frames via stream separado

## Mensagens Core

### Signaling Flow

```
Mac                              Linux
 │                                  │
 │──── ServiceAdvertisement ──────>│   (mDNS)
 │<─── ConnectionRequest ─────────│
 │──── SessionOffer (SDP) ──────->│
 │<─── SessionAnswer (SDP) ───────│
 │<──> ICECandidate ──────────────│   (trickle ICE)
 │                                  │
 │==== WebRTC Media Stream =======>│   (DTLS-SRTP)
 │<==== Input Events ==============│   (DataChannel)
 │                                  │
 │──── StreamControl ────────────>│   (pause, resume, quality)
 │<─── StatusReport ──────────────│   (fps, latency, errors)
```

### Mensagens Definidas

| Mensagem | Direção | Canal | Frequência |
|----------|---------|-------|-----------|
| `ServiceAdvertisement` | Mac → broadcast | mDNS | Periódico |
| `ConnectionRequest` | Linux → Mac | WebSocket | 1x |
| `SessionOffer` | Mac → Linux | WebSocket | 1x |
| `SessionAnswer` | Linux → Mac | WebSocket | 1x |
| `ICECandidate` | Bidirecional | WebSocket | Múltiplos |
| `StreamControl` | Mac → Linux | DataChannel | On demand |
| `StatusReport` | Linux → Mac | DataChannel | Periódico (1/s) |
| `InputEvent` | Linux → Mac | DataChannel | Contínuo |

## Regras de Evolução do Protocolo

1. Toda mudança no protocolo deve ser documentada em PROTOCOL.md
2. Manter compatibilidade backward — clientes antigos devem funcionar
3. Testar serialização/deserialização em ambas as plataformas
4. Mudanças breaking requerem bump de versão do package
