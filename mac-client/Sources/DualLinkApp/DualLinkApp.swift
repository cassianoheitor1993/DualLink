import SwiftUI
import DualLinkCore
import VirtualDisplay
import ScreenCapture

@main
struct DualLinkApp: App {
    @StateObject private var appState = AppState()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(appState)
        }
        .windowStyle(.hiddenTitleBar)
        .windowResizability(.contentSize)
        .commands {
            CommandGroup(replacing: .newItem) {}
        }
    }
}

// MARK: - AppState

/// Estado global da aplicação — gerencia ciclo de vida de todos os managers.
@MainActor
final class AppState: ObservableObject {
    @Published var connectionState: ConnectionState = .idle
    @Published var streamFPS: Double = 0
    @Published var lastError: String?

    let virtualDisplayManager = VirtualDisplayManager()
    let screenCaptureManager = ScreenCaptureManager()

    /// Inicia o pipeline completo: display virtual → captura → encoding → streaming
    func startStreaming(to peer: PeerInfo, config: StreamConfig) async {
        do {
            // 1. Criar display virtual
            try await virtualDisplayManager.create(resolution: config.resolution, refreshRate: config.targetFPS)

            guard let displayID = virtualDisplayManager.activeDisplayID else {
                throw DualLinkError.streamError("Virtual display ID unavailable after creation")
            }

            // 2. Iniciar captura do display virtual
            try await screenCaptureManager.startCapture(displayID: displayID, config: config) { frame in
                // TODO: Fase 1 — encodar e enviar via WebRTC
                _ = frame
            }

            connectionState = .streaming(session: SessionInfo(
                sessionID: UUID().uuidString,
                peer: peer,
                config: config,
                connectionMode: .wifi
            ))
        } catch {
            lastError = error.localizedDescription
            connectionState = .error(reason: error.localizedDescription)
        }
    }

    /// Para o streaming e destrói o display virtual.
    func stopStreaming() async {
        try? await screenCaptureManager.stopCapture()
        await virtualDisplayManager.destroy()
        connectionState = .idle
    }
}
