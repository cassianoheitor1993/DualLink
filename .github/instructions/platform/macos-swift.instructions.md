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
| Transport | `Network.framework` | TLS TCP (7879+2n) + UDP (7878+2n) DLNK |
| mDNS | `Network.framework` `NWBrowser` | Browse `_duallink._tcp.local.` receivers |

## Estrutura do mac-client

```
mac-client/
├── Sources/
│   ├── DualLinkApp/             # Entry point, App lifecycle, ContentView, DisplayStreamPipeline
│   ├── DualLinkCore/            # Shared models: StreamConfig, DisplayInfo, errors
│   ├── ScreenCapture/           # ScreenCaptureKit wrapper (ScreenCaptureManager)
│   ├── VideoEncoder/            # VideoToolbox H.264 encoding
│   ├── VirtualDisplay/          # CGVirtualDisplay management
│   ├── Streaming/               # DLNK stream sender (TLS signaling + UDP video)
│   ├── Signaling/               # TLS TCP signaling client (ClientHello, Config, Ack)
│   ├── Discovery/               # NWBrowser for _duallink._tcp.local. + NWTXTRecord parse
│   ├── Transport/               # TransportDiscovery (USB Ethernet detection), TransportBenchmark
│   └── InputInjection/          # CGEvent injection (from receiver back-channel)
├── Tests/
└── Package.swift
```

### run_app.sh

O executável deve ser lançado via `run_app.sh` (bundle wrapper) porque `CGVirtualDisplay`
exige que o processo rode dentro de um `.app` bundle com entitlements assinados.
Usar `swift run` diretamente NÃO funciona para virtual display.

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
| Network (Client) | DLNK TLS/UDP outbound | `com.apple.security.network.client` |
| Network (Server) | DLNK signaling listener | `com.apple.security.network.server` |

## mDNS Discovery — NWBrowser Pattern

```swift
// Browse para receivers DualLink na rede local
let browser = NWBrowser(
    for: .bonjourWithTXTRecord(type: "_duallink._tcp", domain: "local."),
    using: .tcp
)
browser.browseResultsChangedHandler = { results, _ in
    for result in results {
        if case let .service(name, _, _, _) = result.endpoint,
           case let .bonjour(txt) = result.metadata {
            let host = txt.dictionary["host"] ?? ""
            let fp   = txt.dictionary["fp"]   ?? ""
            // Exibir receiver na lista UI
        }
    }
}
browser.start(queue: .main)
```

## Armadilhas macOS Conhecidas

1. **ScreenCaptureKit requer permissão explícita** — avisar o usuário com UI clara
2. **CGVirtualDisplay NÃO funciona sandboxed nem via `swift run`** — usar `run_app.sh` (bundle)
3. **VideoToolbox callbacks são em thread arbitrária** — dispatch para `MainActor` se atualizar UI
4. **CVPixelBuffer lock** — sempre usar `CVPixelBufferLockBaseAddress`/`Unlock` ao acessar pixels
5. **CMSampleBuffer timing** — preservar timestamps para sync correto
6. **NWBrowser requer Network entitlement** — sem `com.apple.security.network.client` o browse retorna vazio silenciosamente
