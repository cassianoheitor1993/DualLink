---
applyTo: "mac-client/**"
---

# Platform: macOS (Swift)

> Carregado automaticamente ao editar arquivos em `mac-client/`.

## Ambiente

- **Linguagem:** Swift 5.9+
- **UI:** SwiftUI
- **Concurrency:** async/await, Actor
- **Min Deployment:** macOS 14.0 (Sonoma)
- **Build:** Xcode + Swift Package Manager

## Frameworks Core

| Componente | Framework | Uso |
|-----------|-----------|-----|
| Screen Capture | `ScreenCaptureKit` | Capturar frames do display |
| Video Encoding | `VideoToolbox` | H.264/H.265 hardware encoding |
| Virtual Display | `CoreGraphics` (CGVirtualDisplay) | Criar monitor virtual |
| Networking | `Network.framework` | TCP/UDP/QUIC connections |
| Media Management | `CoreMedia` | CMSampleBuffer, CVPixelBuffer |
| WebRTC | `WebRTC.framework` (Google) | Streaming |

## Estrutura do mac-client

```
mac-client/
├── Sources/
│   ├── App/                    # Entry point, App lifecycle
│   ├── ScreenCapture/          # ScreenCaptureKit wrapper
│   ├── VideoEncoder/           # VideoToolbox encoding
│   ├── VirtualDisplay/         # CGVirtualDisplay management
│   ├── Streaming/              # WebRTC sender
│   ├── Signaling/              # WebSocket signaling client
│   ├── Discovery/              # mDNS/Bonjour discovery
│   ├── Transport/              # Abstração USB/Wi-Fi
│   ├── InputInjection/         # CGEvent injection
│   └── UI/                     # SwiftUI views
├── Tests/
└── Package.swift
```

## Padrões Swift para Este Projeto

### Concurrency

```swift
// ✅ Usar Actor para estado compartilhado
actor StreamManager {
    private var isStreaming = false
    
    func startStream() async throws {
        guard !isStreaming else { return }
        isStreaming = true
        // ...
    }
}

// ❌ Evitar: DispatchQueue, callbacks, completion handlers
```

### Error Handling

```swift
// ✅ Typed errors com contexto
enum ScreenCaptureError: LocalizedError {
    case permissionDenied
    case displayNotFound(displayID: CGDirectDisplayID)
    case captureFailure(underlying: Error)
    
    var errorDescription: String? {
        switch self {
        case .permissionDenied:
            return "Screen recording permission not granted"
        case .displayNotFound(let id):
            return "Display \(id) not found"
        case .captureFailure(let error):
            return "Capture failed: \(error.localizedDescription)"
        }
    }
}
```

### Buffer Management

```swift
// ✅ Zero-copy quando possível — CVPixelBuffer backed by IOSurface
func processFrame(_ sampleBuffer: CMSampleBuffer) {
    guard let pixelBuffer = sampleBuffer.imageBuffer else { return }
    // CVPixelBuffer já está na GPU (IOSurface-backed via ScreenCaptureKit)
    // Passar diretamente para VideoToolbox — sem cópias
    encoder.encode(pixelBuffer, presentationTime: sampleBuffer.presentationTimeStamp)
}

// ❌ Evitar: copiar pixels para Data/Array desnecessariamente
```

### VideoToolbox Best Practices

```swift
// Configuração de baixa latência
let properties: [CFString: Any] = [
    kVTCompressionPropertyKey_RealTime: kCFBooleanTrue!,
    kVTCompressionPropertyKey_AllowFrameReordering: kCFBooleanFalse!,
    kVTCompressionPropertyKey_ProfileLevel: kVTProfileLevel_H264_Baseline_AutoLevel,
    kVTCompressionPropertyKey_MaxKeyFrameInterval: 60,
    kVTCompressionPropertyKey_AverageBitRate: 8_000_000, // 8 Mbps
]
```

## Permissões Necessárias

| Permissão | Motivo | Chave Info.plist |
|-----------|--------|-----------------|
| Screen Recording | ScreenCaptureKit | `NSScreenCaptureUsageDescription` |
| Network (Client) | WebRTC connections | `com.apple.security.network.client` |
| Network (Server) | Signaling server | `com.apple.security.network.server` |

## Armadilhas macOS Conhecidas

1. **ScreenCaptureKit requer permissão explícita** — avisar o usuário com UI clara
2. **CGVirtualDisplay pode não funcionar sandboxed** — testar sem sandbox primeiro
3. **VideoToolbox callbacks são em thread arbitrária** — dispatch para MainActor se atualizar UI
4. **CVPixelBuffer lock** — sempre usar `CVPixelBufferLockBaseAddress`/`Unlock` se acessar pixels diretamente
5. **CMSampleBuffer timing** — preservar timestamps para sync A/V
