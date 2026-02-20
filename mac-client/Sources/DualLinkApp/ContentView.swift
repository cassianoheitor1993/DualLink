import SwiftUI
import DualLinkCore
import VirtualDisplay
import ScreenCapture

struct ContentView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        VStack(spacing: 0) {
            HeaderView()
            Divider()
            StatusView()
            Divider()
            ControlsView()
        }
        .frame(width: 380)
        .background(.ultraThinMaterial)
    }
}

// MARK: - HeaderView

private struct HeaderView: View {
    var body: some View {
        HStack {
            Image(systemName: "display.2")
                .font(.title2)
                .foregroundStyle(.blue)
            VStack(alignment: .leading, spacing: 2) {
                Text("DualLink")
                    .font(.headline)
                Text("Screen Mirroring")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            ConnectionBadge()
        }
        .padding()
    }
}

// MARK: - ConnectionBadge

private struct ConnectionBadge: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        HStack(spacing: 4) {
            Circle()
                .fill(badgeColor)
                .frame(width: 8, height: 8)
            Text(badgeLabel)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(.quaternary, in: Capsule())
    }

    private var badgeColor: Color {
        switch appState.connectionState {
        case .streaming:    return .green
        case .connecting:   return .yellow
        case .discovering:  return .blue
        case .error:        return .red
        default:            return .gray
        }
    }

    private var badgeLabel: String {
        switch appState.connectionState {
        case .idle:          return "Idle"
        case .discovering:   return "Searching..."
        case .connecting:    return "Connecting..."
        case .streaming:     return "Streaming"
        case .reconnecting:  return "Reconnecting..."
        case .error:         return "Error"
        }
    }
}

// MARK: - StatusView

private struct StatusView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if case .streaming(let session) = appState.connectionState {
                StreamingStatusRow(label: "Connected to", value: session.peer.name)
                StreamingStatusRow(label: "Resolution", value: "\(session.config.resolution.width)×\(session.config.resolution.height)")
                StreamingStatusRow(label: "FPS", value: String(format: "%.0f", appState.streamFPS))
                StreamingStatusRow(label: "Bitrate", value: "\(session.config.maxBitrateBps / 1_000_000) Mbps")
                StreamingStatusRow(label: "Transport", value: session.connectionMode.rawValue.uppercased())
            } else if let error = appState.lastError {
                HStack {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.red)
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
            } else {
                Text("Waiting to connect...")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                    .frame(maxWidth: .infinity, alignment: .center)
                    .padding(.vertical, 8)
            }
        }
        .padding()
    }
}

private struct StreamingStatusRow: View {
    let label: String
    let value: String

    var body: some View {
        HStack {
            Text(label)
                .font(.caption)
                .foregroundStyle(.secondary)
            Spacer()
            Text(value)
                .font(.caption.monospacedDigit())
                .fontWeight(.medium)
        }
    }
}

// MARK: - ControlsView

private struct ControlsView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        HStack(spacing: 8) {
            if appState.connectionState.isActive {
                Button(role: .destructive) {
                    Task { await appState.stopStreaming() }
                } label: {
                    Label("Stop", systemImage: "stop.fill")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .tint(.red)
            } else {
                Button {
                    // TODO: Fase 1 — iniciar discovery e conexão
                } label: {
                    Label("Start Mirroring", systemImage: "play.fill")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .disabled(appState.connectionState == .discovering)
            }

            Menu {
                Button("Settings...") {}
                Button("About DualLink") {}
                Divider()
                Button("Quit", role: .destructive) {
                    NSApplication.shared.terminate(nil)
                }
            } label: {
                Image(systemName: "ellipsis.circle")
            }
            .menuStyle(.borderlessButton)
            .frame(width: 32)
        }
        .padding()
    }
}

// MARK: - Preview

#Preview {
    ContentView()
        .environmentObject(AppState())
}
