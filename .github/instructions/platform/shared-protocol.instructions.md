---
applyTo: "linux-receiver/**,linux-sender/**,mac-client/**,windows-sender/**"
---

# Platform: Shared Protocol — DLNK

> Descreve o protocolo binário DLNK usado por todos os componentes DualLink.
> **Não usa WebRTC, WebSocket, SDP, ICE, ou Protocol Buffers.**

## Visão Geral

DualLink usa dois canais por display:

| Canal | Protocolo | Porta | Direção | Propósito |
|-------|-----------|-------|---------|-----------|
| Signaling | TLS 1.3 TCP | `7879 + 2n` | Sender → Receiver | Config, pairing, controle |
| Video | UDP (DLNK frames) | `7878 + 2n` | Sender → Receiver | Video H.264 |
| Input back-channel | TLS TCP (reuse signaling) | `7879 + 2n` | Receiver → Sender | Eventos teclado/mouse |

`n` = zero-based display index.

---

## mDNS Discovery

### Service Type

```
_duallink._tcp.local.
```

### TXT Record Keys

| Chave | Exemplo | Descrição |
|-------|---------|-----------|
| `version` | `1` | Versão do protocolo DLNK |
| `displays` | `2` | Número de displays disponíveis |
| `port` | `7879` | Porta TCP base do signaling |
| `host` | `192.168.1.42` | IP LAN do receiver |
| `fp` | `AA:BB:CC:...` | SHA-256 TLS fingerprint (TOFU) |

O receiver anuncia via `mdns-sd`. Senders fazem browse e exibem a lista no UI.

---

## Pairing — PIN + TLS TOFU

1. **Receiver gera** um PIN de 6 dígitos e o exibe na tela.
2. **Sender lê** o `fp` do TXT record mDNS.
3. **Sender conecta** via TLS ao receiver; verifica que o fingerprint do certificado
   apresentado coincide com `fp`.
4. **Sender envia** `ClientHello` com o PIN.
5. **Receiver valida** o PIN → aceita ou rejeita a conexão.
6. **Após aceito**, receiver armazena o fingerprint do sender para futuras re-conexões
   automáticas (TOFU).

---

## Signaling Messages (TLS TCP)

Mensagens são framed com um header de 4 bytes (length-prefixed, big-endian u32)
seguido de JSON payload.

### ClientHello  (Sender → Receiver)

```json
{
  "type": "hello",
  "version": 1,
  "pin": "123456",
  "display_index": 0
}
```

### StreamConfig  (Receiver → Sender, após aceitar)

```json
{
  "type": "config",
  "width": 1920,
  "height": 1080,
  "fps": 60,
  "bitrate_kbps": 8000,
  "display_index": 0
}
```

### Ack  (Sender → Receiver)

```json
{
  "type": "ack",
  "display_index": 0
}
```

### InputEvent (Receiver → Sender, back-channel)

```json
{
  "type": "input",
  "kind": "mouse_move",
  "dx": 10,
  "dy": -5,
  "button": null,
  "pressed": null,
  "key_code": null
}
```

---

## DLNK UDP Frame Header

Cada pacote UDP começa com um header binário de **18 bytes**:

```
Offset  Size  Field           Description
──────────────────────────────────────────────────
  0      4    magic           0x444C4E4B ("DLNK")
  4      4    sequence        Frame sequence number (u32 BE)
  8      8    timestamp_us    Capture timestamp us (u64 BE)
 16      1    flags           Bit 0 = keyframe
 17      1    display_index   Zero-based display index
──────────────────────────────────────────────────
 18     ...   H.264 NAL data  Remainder of UDP payload
```

### Regras

- **MTU:** Target 1400 bytes payload; fragmentar NAL units se necessário
- **Sequencing:** Receiver usa `sequence` para detectar perda e reordenar
- **Keyframe flag:** Quando `flags & 0x01 == 1`, este frame pode iniciar decode
- **display_index:** Permite multiplexar N displays num único pipeline receiver

---

## Evolução do Protocolo

1. Mudanças **backward-compatible**: adicionar campos JSON opcionais.
2. **Breaking changes**: incrementar `version` no `ClientHello` e no TXT record.
3. Documentar toda mudança neste arquivo + golden-tips se a mudança causou bugs.
4. Receivers devem rejeitar graciosamente versões desconhecidas com mensagem de erro legível.
