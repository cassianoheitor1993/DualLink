// PoC: ScreenCaptureKit — Sprint 0.2
//
// Pergunta: É possível capturar frames do display principal via ScreenCaptureKit
// com latência < 3ms e 30fps+ usando hardware acceleration?
//
// Execução:
//   swift run PoCScreenCapture
//   (ou: swift package resolve && swift build && .build/debug/PoCScreenCapture)
//
// IMPORTANTE: Requer permissão de Screen Recording.
//   System Settings → Privacy & Security → Screen Recording → adicionar Terminal

import ScreenCaptureKit
import CoreMedia
import CoreVideo
import VideoToolbox
import Foundation

// MARK: - Output Handler

class CaptureOutput: NSObject, SCStreamOutput {
    var frameCount = 0
    var firstFrameTime: Date?
    var lastTimestamp: CMTime?
    var latencies: [Double] = []
    var captureGroup: DispatchGroup

    init(group: DispatchGroup) {
        self.captureGroup = group
        super.init()
    }

    func stream(
        _ stream: SCStream,
        didOutputSampleBuffer sampleBuffer: CMSampleBuffer,
        of outputType: SCStreamOutputType
    ) {
        guard outputType == .screen else { return }

        let now = Date()

        if firstFrameTime == nil {
            firstFrameTime = now
            print("[PoC] First frame received!")
        }

        frameCount += 1

        // Medir latência de captura (timestamp do buffer vs agora)
        let presentationTime = sampleBuffer.presentationTimeStamp
        if let last = lastTimestamp {
            let diff = CMTimeGetSeconds(CMTimeSubtract(presentationTime, last)) * 1000
            latencies.append(diff)
        }
        lastTimestamp = presentationTime

        // Verificar formato do pixel buffer
        guard let pixelBuffer = sampleBuffer.imageBuffer else { return }
        let pixelFormat = CVPixelBufferGetPixelFormatType(pixelBuffer)
        let width = CVPixelBufferGetWidth(pixelBuffer)
        let height = CVPixelBufferGetHeight(pixelBuffer)

        if frameCount == 1 {
            let formatStr = String(format: "0x%08X", pixelFormat)
            print("[PoC] Frame format: \(formatStr)")
            print("[PoC] Frame size: \(width)x\(height)")
            print("[PoC] IOSurface-backed:", CVPixelBufferGetIOSurface(pixelBuffer) != nil ? "YES ✅" : "NO ❌")
        }

        // Parar após 90 frames (3 segundos @30fps)
        if frameCount >= 90 {
            captureGroup.leave()
        }
    }
}

// MARK: - Main

@main
struct PoCScreenCapture {
    static func main() async {
        print("=== ScreenCaptureKit PoC ===\n")

        // Step 1: Verificar permissão
        print("[1/4] Requesting screen capture permission...")
        do {
            _ = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
            print("[✅ OK] Permission granted\n")
        } catch {
            print("[❌ FAIL] Permission denied: \(error)")
            print("\n  → Go to: System Settings > Privacy & Security > Screen Recording")
            print("  → Add Terminal (or your app) to the list")
            return
        }

        // Step 2: Listar displays disponíveis
        print("[2/4] Listing available displays...")
        let content = try! await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: false)

        for display in content.displays {
            print("  Display: \(display.displayID) — \(display.width)×\(display.height)")
        }

        guard let mainDisplay = content.displays.first else {
            print("[❌ FAIL] No displays found")
            return
        }

        print("[✅ OK] Using display: \(mainDisplay.displayID)\n")

        // Step 3: Configurar stream
        print("[3/4] Configuring capture stream (1920x1080 @ 30fps, NV12)...")

        let filter = SCContentFilter(display: mainDisplay, excludingWindows: [])

        let config = SCStreamConfiguration()
        config.width = 1920
        config.height = 1080

        // NV12 = formato ideal para H.264 encoding (sem conversão de cor)
        config.pixelFormat = kCVPixelFormatType_420YpCbCr8BiPlanarFullRange

        // 30fps
        config.minimumFrameInterval = CMTime(value: 1, timescale: 30)
        config.showsCursor = false

        let group = DispatchGroup()
        group.enter()

        let outputHandler = CaptureOutput(group: group)

        let stream = SCStream(filter: filter, configuration: config, delegate: nil)
        do {
            try stream.addStreamOutput(
                outputHandler,
                type: .screen,
                sampleHandlerQueue: .global(qos: .userInteractive)
            )
        } catch {
            print("[❌ FAIL] addStreamOutput failed: \(error)")
            return
        }

        print("[✅ OK] Stream configured\n")

        // Step 4: Capturar 90 frames e medir performance
        print("[4/4] Capturing 90 frames (3 seconds)...")
        print("      Watch for:")
        print("      - Actualfps close to 30")
        print("      - IOSurface-backed = YES (zero-copy path)")
        print("      - Pixel format NV12 (0x34323076)\n")

        let captureStart = Date()

        do {
            try await stream.startCapture()
        } catch {
            print("[❌ FAIL] startCapture failed: \(error)")
            return
        }

        // Aguardar 90 frames ou timeout de 10s
        let timeout = DispatchTime.now() + .seconds(10)
        let result = group.wait(timeout: timeout)

        try? await stream.stopCapture()

        // MARK: - Resultados

        let elapsed = Date().timeIntervalSince(captureStart)
        let fps = Double(outputHandler.frameCount) / elapsed

        print("\n=== Results ===\n")

        let avgLatency = outputHandler.latencies.isEmpty ? 0 :
            outputHandler.latencies.reduce(0, +) / Double(outputHandler.latencies.count)
        let maxLatency = outputHandler.latencies.max() ?? 0
        let minLatency = outputHandler.latencies.min() ?? 0

        print(String(format: "Frames captured: %d", outputHandler.frameCount))
        print(String(format: "Elapsed time:    %.2fs", elapsed))
        print(String(format: "Actual FPS:      %.1f fps  %@", fps, fps >= 28 ? "✅" : "⚠️"))
        print(String(format: "Avg frame time:  %.2fms", avgLatency))
        print(String(format: "Min frame time:  %.2fms", minLatency))
        print(String(format: "Max frame time:  %.2fms  %@", maxLatency, maxLatency < 10 ? "✅" : "⚠️"))

        if result == .timedOut {
            print("\n⚠️  Timed out waiting for frames")
        }

        print("\n=== Decisions ===\n")
        print(fps >= 28 ? "✅ ScreenCaptureKit can sustain 30fps" : "⚠️  FPS below target — investigate")
        print("→ Next: Test with virtual display (CGVirtualDisplay)")
        print("→ Next: Measure VideoToolbox encoding latency")
    }
}
