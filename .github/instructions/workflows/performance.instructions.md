---
applyTo: "**"
---

# Workflow: Performance Optimization

> Ativar quando: otimizando latência, throughput, CPU usage, ou qualquer métrica de performance.

## Regra de Ouro

> **"Nunca otimizar sem medir. Nunca medir sem objetivo."**

## Processo de Otimização

### 1. Definir Objetivo

```
Métrica:    [O que melhorar — latência, CPU, fps, etc.]
Valor atual: [Medição baseline]
Target:     [Valor desejado]
Razão:      [Por que esse target — spec do projeto, experiência do usuário, etc.]
```

### 2. Medir Baseline

Antes de qualquer mudança, capturar:
- Valor atual da métrica
- Condições do teste (hardware, resolução, rede)
- Profiling detalhado (onde o tempo é gasto)

### 3. Identificar Bottleneck

Usar profiling adequado:

| Plataforma | Ferramenta | Uso |
|-----------|-----------|-----|
| macOS | Instruments (Time Profiler) | CPU profiling |
| macOS | Instruments (GPU) | GPU utilization |
| macOS | `os_signpost` | Medir intervalos custom |
| Linux | `perf record/report` | CPU profiling |
| Linux | `nvidia-smi` / `intel_gpu_top` | GPU utilization |
| Linux | `flamegraph` | Visualização de perf |
| Ambos | Timestamps nos frames | Latência end-to-end |

### 4. Otimizar

```
1. Mudar APENAS uma variável por vez
2. Medir após cada mudança
3. Se não melhorou, REVERTER
4. Se melhorou, documentar a mudança e o ganho
```

### 5. Documentar

```markdown
### Otimização: [Título]
- **Data:** YYYY-MM-DD
- **Métrica:** [O que foi medido]
- **Antes:** [Valor baseline]
- **Depois:** [Valor após otimização]
- **Ganho:** [Percentual ou absoluto]
- **Trade-off:** [Se houver — ex: mais memória]
- **Mudança:** [O que foi feito]
```

## Latency Budget — DualLink Pipeline

```
Total budget Wi-Fi: 80ms
Total budget USB:   40ms

┌─────────────────────┬───────────┬───────────┐
│ Etapa               │ Budget    │ Medido    │
├─────────────────────┼───────────┼───────────┤
│ Screen Capture      │ ≤ 3ms     │ ___ms     │
│ Encoding (H.264)    │ ≤ 5ms     │ ___ms     │
│ Network Transport   │ ≤ 20ms*   │ ___ms     │
│ Jitter Buffer       │ ≤ 30ms*   │ ___ms     │
│ Decoding            │ ≤ 5ms     │ ___ms     │
│ Rendering           │ ≤ 3ms     │ ___ms     │
│ Display scanout     │ ≤ 16ms    │ ___ms     │
├─────────────────────┼───────────┼───────────┤
│ TOTAL               │ ≤ 80ms    │ ___ms     │
└─────────────────────┴───────────┴───────────┘
* Wi-Fi. USB seria ~3ms total para transport.
```

## Técnicas de Otimização por Componente

### Screen Capture (macOS)
- Configurar `SCStreamConfiguration.minimumFrameInterval` corretamente
- Usar pixel format nativo (BGRA ou NV12) sem conversão
- Capturar apenas o display necessário (não toda a tela)

### Encoding (macOS)
- `kVTCompressionPropertyKey_RealTime: true` — essencial
- `kVTCompressionPropertyKey_AllowFrameReordering: false` — elimina latência de B-frames
- Profile Baseline (sem B-frames) para latência mínima
- Ajustar bitrate para bandwidth disponível

### Network
- WebRTC: minimizar jitter buffer (`googMinDelay`, `googMaxDelay`)
- Preferir UDP sobre TCP
- Para USB: zero jitter buffer (link direto)

### Decoding (Linux)
- Verificar que GPU decoding está ativo: `GST_DEBUG=2 | grep nvh264dec`
- Pipeline sin videoconvert (converter apenas se necessário)
- Usar formato nativo da GPU (NV12) até o rendering

### Rendering (Linux)
- Vsync alinhado com refresh rate do display
- Double buffering (não triple — adiciona latência)
- Apresentar frame imediatamente — não esperar batch

## Anti-patterns de Performance

- ❌ Copiar buffers de vídeo desnecessariamente (zero-copy sempre que possível)
- ❌ Conversão de pixel format no meio do pipeline
- ❌ Sleep/polling quando eventos assíncronos são disponíveis
- ❌ Alocar memória em hot paths (pre-allocate)
- ❌ Lock contention em caminho crítico (use channels)
- ❌ Otimizar sem medir (pode piorar ou não ter efeito)

## Regras

- **Toda regressão de performance é um bug** com prioridade alta
- **Rodar benchmarks** no CI para detectar regressões automaticamente
- **Anotar código performance-critical** com comentário `// PERF:`
- **Medir em hardware real** — simuladores não refletem performance real
