---
applyTo: "**/Tests/**,**/tests/**,**/benches/**,**/*test*"
---

# Workflow: Testing & QA

> Carregado automaticamente ao editar arquivos de teste.

## Estratégia de Testes

### Pirâmide de Testes para DualLink

```
          ╱╲
         ╱  ╲         E2E Tests (poucos, lentos)
        ╱    ╲        Mac → Linux stream funcional
       ╱──────╲
      ╱        ╲      Integration Tests (moderados)
     ╱          ╲     Módulos interagindo (encoder → decoder)
    ╱────────────╲
   ╱              ╲   Unit Tests (muitos, rápidos)
  ╱                ╲  Lógica isolada, sem I/O
 ╱──────────────────╲
```

### O Que Testar

| Componente | Unit Tests | Integration | E2E |
|-----------|:---------:|:-----------:|:---:|
| Protocol serialization | ✅ | — | — |
| Video encoder config | ✅ | — | — |
| Signaling messages | ✅ | ✅ | — |
| mDNS discovery | — | ✅ | — |
| Full pipeline (capture→render) | — | — | ✅ |
| Reconnection logic | ✅ | ✅ | — |
| Input event mapping | ✅ | — | — |

### O Que NÃO Testar (Neste Projeto)

- GStreamer internals (testado pelo projeto GStreamer)
- WebRTC protocol (testado pelo framework)
- VideoToolbox encoding correctness (testado pela Apple)
- **Foco:** testar NOSSO código, não o código das dependências

## Padrões por Plataforma

### Swift (XCTest)

```swift
import XCTest
@testable import DualLinkEncoder

final class VideoEncoderConfigTests: XCTestCase {
    
    func test_lowLatencyConfig_disablesFrameReordering() {
        let config = VideoEncoderConfig.lowLatency(resolution: .fhd)
        
        XCTAssertFalse(config.allowFrameReordering)
        XCTAssertTrue(config.realTime)
        XCTAssertEqual(config.profile, .h264Baseline)
    }
    
    func test_bitrateForResolution_scalesCorrectly() {
        let fhd = VideoEncoderConfig.suggestedBitrate(for: .fhd, fps: 30)
        let uhd = VideoEncoderConfig.suggestedBitrate(for: .uhd, fps: 30)
        
        XCTAssertGreaterThan(uhd, fhd)
        XCTAssertEqual(fhd, 8_000_000) // 8 Mbps
    }
}
```

### Rust (#[cfg(test)])

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn encoded_frame_preserves_timestamp() {
        let frame = EncodedFrame {
            data: &[0x00, 0x00, 0x01],
            timestamp_us: 16_667, // ~60fps
            is_keyframe: true,
        };
        
        assert_eq!(frame.timestamp_us, 16_667);
        assert!(frame.is_keyframe);
    }
    
    #[tokio::test]
    async fn decoder_pipeline_processes_frame() {
        let (tx, rx) = mpsc::channel(16);
        let (decoded_tx, mut decoded_rx) = mpsc::channel(16);
        
        let mut pipeline = DecoderPipeline::new(rx, decoded_tx);
        
        // Send test frame
        tx.send(test_h264_frame()).await.unwrap();
        drop(tx); // Close channel
        
        pipeline.run().await.unwrap();
        
        let decoded = decoded_rx.recv().await.unwrap();
        assert_eq!(decoded.width, 1920);
        assert_eq!(decoded.height, 1080);
    }
}
```

## Testes de Performance (Benchmarks)

### Métricas Obrigatórias

Antes de qualquer otimização, medir:

| Métrica | Como medir | Target |
|---------|-----------|--------|
| Encoding latency | Timestamp before/after VTCompressionSession | < 5ms |
| Decoding latency | Timestamp before/after GStreamer pipeline | < 5ms |
| End-to-end latency | Timestamp no frame vs display time | < 80ms Wi-Fi |
| CPU usage | Instruments (Mac) / `perf` (Linux) | < 25% |
| Memory usage | Activity Monitor / `heaptrack` | Estável |
| Frame drops | Counter de frames pulados | < 1% |

### Rust Benchmarks

```rust
// benches/decoder_bench.rs
use criterion::{criterion_group, criterion_main, Criterion};

fn decode_frame_benchmark(c: &mut Criterion) {
    let frame = include_bytes!("fixtures/test_frame.h264");
    
    c.bench_function("decode_h264_frame", |b| {
        b.iter(|| {
            decode_frame(frame).unwrap()
        })
    });
}

criterion_group!(benches, decode_frame_benchmark);
criterion_main!(benches);
```

## Regras

- **Não mergear** código sem testes para lógica não-trivial
- **Nomear testes** descritivamente: `test_<unidade>_<cenário>_<esperado>`
- **Testes rápidos** — unit tests devem rodar em < 1s total
- **Fixtures** — usar arquivos de teste reais (H.264 frames, etc.) em `tests/fixtures/`
- **CI** — todos os testes devem rodar no GitHub Actions
