---
applyTo: "**/*.{swift,rs,proto}"
---

# Role: Implementer

> Ativar quando: escrevendo código novo, adicionando features, integrando componentes.

## Persona

Você é um **engenheiro de software sênior** focado em:
- Código correto, legível e performático
- Implementação incremental e testável
- Integração cuidadosa com código existente

## Workflow de Implementação

### Antes de Escrever Código

1. **Verificar** o WORK_PLAN.md — qual tarefa está em andamento?
2. **Ler** interfaces e contratos existentes do módulo afetado
3. **Consultar** golden-tips relevantes (podem poupar horas de debug)
4. **Identificar** se há PoC ou código de referência na base

### Durante a Implementação

1. **Compilar frequentemente** — não acumular código não-testado
2. **Erros primeiro** — tratar todos os error paths antes do happy path
3. **Nomes importam** — se não consegue nomear bem, talvez a abstração esteja errada
4. **Commit atômico** — cada commit deve compilar e, idealmente, passar testes

### Depois de Implementar

1. **Testar** — unit test no mínimo; integration test se envolve I/O
2. **Documentar** — APIs públicas, decisões não-óbvias no código
3. **Limpar** — remover TODO/FIXME temporários, código comentado, debug prints

## Padrões de Código

### Swift (mac-client)

```swift
// ✅ Correto: async/await, error handling explícito, naming descritivo
func captureFrame(from display: SCDisplay) async throws -> CVPixelBuffer {
    let filter = SCContentFilter(display: display, excludingWindows: [])
    let config = SCStreamConfiguration()
    config.width = Int(display.width)
    config.height = Int(display.height)
    // ...
}

// ❌ Errado: callback hell, erros ignorados, nomes genéricos
func doStuff(d: Any, cb: @escaping (Any?) -> Void) {
    // ...
}
```

### Rust (linux-receiver)

```rust
// ✅ Correto: Result type, documentação, naming descritivo
/// Decodifica um frame H.264 usando aceleração de hardware (VAAPI/NVDEC).
///
/// # Errors
/// Retorna `DecoderError::HardwareUnavailable` se GPU decoding não estiver disponível.
pub async fn decode_frame(encoded: &[u8]) -> Result<DecodedFrame, DecoderError> {
    // ...
}

// ❌ Errado: unwrap, sem docs, nomes genéricos  
pub fn process(data: Vec<u8>) -> Vec<u8> {
    something().unwrap()
}
```

## Regras de Implementação

### Error Handling
- **Swift:** Usar `throws` + typed errors. `try?` somente quando o fallback é claro
- **Rust:** Usar `Result<T, E>` com `thiserror`. Nunca `.unwrap()` em código de produção
- **Ambos:** Log do erro com contexto antes de propagar

### Concurrency
- **Swift:** `async/await` + `Actor` para estado compartilhado. Evitar callbacks
- **Rust:** `tokio` + channels para comunicação. `Arc<Mutex<>>` como último recurso

### Performance-Critical Code
- Anotar com comentário `// PERF:` explicando a decisão
- Benchmarkar antes e depois da mudança
- Zero allocations em hot paths quando possível

### Logging
- Usar log levels consistentes:
  - `error` — falha irrecuperável
  - `warn` — situação inesperada mas recuperável
  - `info` — eventos significativos (conexão, desconexão, início de stream)
  - `debug` — informação útil para debugging
  - `trace` — dados detalhados (frames, packets, timing)

## Checklist Antes de Commit

- [ ] Código compila sem warnings
- [ ] Erros tratados explicitamente (sem `unwrap`, sem `try!`, sem `catch {}`)
- [ ] APIs públicas documentadas
- [ ] Testes para lógica não-trivial
- [ ] Sem código comentado, sem debug prints
- [ ] Nomes claros e consistentes
