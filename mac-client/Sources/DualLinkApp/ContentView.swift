import SwiftUI
import AppKit
import DualLinkCore
import VirtualDisplay
import ScreenCapture
import Transport

/// User-facing transport selection (includes Auto mode).
enum TransportSelection: String, CaseIterable {
    case auto = "Auto"
    case usb  = "USB"
    case wifi = "Wi-Fi"
}

struct ContentView: View {
    @EnvironmentObject var appState: AppState
    @State private var receiverHost: String = "10.0.0.59"
    @State private var pairingPin: String = ""
    @State private var use60fps: Bool = true
    @State private var displayMode: DisplayMode = .extend
    @State private var selectedResolution: Resolution = .fhd
    @State private var selectedCodec: VideoCodec = .h264
    @State private var transportMode: TransportSelection = .auto

    var body: some View {
        VStack(spacing: 0) {
            HeaderView()
            Divider()
            StatusView()
            Divider()
            if !appState.connectionState.isActive {
                ConnectView(
                    receiverHost: $receiverHost,
                    pairingPin: $pairingPin,
                    use60fps: $use60fps,
                    displayMode: $displayMode,
                    selectedResolution: $selectedResolution,
                    selectedCodec: $selectedCodec,
                    transportMode: $transportMode
                )
                Divider()
            }
            ControlsView(
                receiverHost: receiverHost,
                pairingPin: pairingPin,
                use60fps: use60fps,
                displayMode: displayMode,
                selectedResolution: selectedResolution,
                selectedCodec: selectedCodec,
                transportMode: transportMode
            )
        }
        .frame(width: 380)
        .background(.ultraThinMaterial)
        .onAppear {
            // Make the window key so TextField accepts keyboard input
            NSApp.activate(ignoringOtherApps: true)
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
                NSApp.mainWindow?.makeKeyAndOrderFront(nil)
            }
        }
    }
}

// MARK: - ConnectView

private struct ConnectView: View {
    @Binding var receiverHost: String
    @Binding var pairingPin: String
    @Binding var use60fps: Bool
    @Binding var displayMode: DisplayMode
    @Binding var selectedResolution: Resolution
    @Binding var selectedCodec: VideoCodec
    @Binding var transportMode: TransportSelection
    @FocusState private var isFocused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            // Transport mode picker
            HStack(spacing: 8) {
                Image(systemName: "cable.connector")
                    .foregroundStyle(.blue)
                Picker("", selection: $transportMode) {
                    ForEach(TransportSelection.allCases, id: \.self) { mode in
                        Text(mode.rawValue).tag(mode)
                    }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
            }

            // Wi-Fi IP field (hidden when USB-only)
            if transportMode != .usb {
                Text("Linux Receiver IP")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                TextField("192.168.1.x", text: $receiverHost)
                    .textFieldStyle(.roundedBorder)
                    .font(.system(.body, design: .monospaced))
                    .focused($isFocused)
                    .onAppear { isFocused = true }
            }

            // Pairing PIN field
            HStack(spacing: 8) {
                Image(systemName: "lock.shield")
                    .foregroundStyle(.blue)
                VStack(alignment: .leading, spacing: 2) {
                    Text("Pairing PIN")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    TextField("6-digit PIN", text: $pairingPin)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(.body, design: .monospaced))
                        .frame(width: 120)
                }
            }

            // Display mode picker
            HStack(spacing: 8) {
                Image(systemName: displayMode == .extend ? "display.2" : "rectangle.on.rectangle")
                    .foregroundStyle(.blue)
                Picker("", selection: $displayMode) {
                    Text("Extend Display").tag(DisplayMode.extend)
                    Text("Mirror Display").tag(DisplayMode.mirror)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
            }

            // Resolution picker
            HStack(spacing: 8) {
                Image(systemName: "rectangle.arrowtriangle.2.outward")
                    .foregroundStyle(.blue)
                Picker("", selection: $selectedResolution) {
                    ForEach(Resolution.allPresets, id: \.width) { res in
                        Text(res.label).tag(res)
                    }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
            }

            // Codec picker
            HStack(spacing: 8) {
                Image(systemName: "video")
                    .foregroundStyle(.blue)
                Picker("", selection: $selectedCodec) {
                    Text("H.264").tag(VideoCodec.h264)
                    Text("H.265").tag(VideoCodec.h265)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
            }

            Toggle(isOn: $use60fps) {
                HStack(spacing: 4) {
                    Image(systemName: "speedometer")
                        .foregroundStyle(.blue)
                    Text(use60fps ? "60 fps" : "30 fps")
                        .font(.caption)
                        .monospacedDigit()
                }
            }
            .toggleStyle(.switch)
            .controlSize(.small)
        }
        .padding(.horizontal)
        .padding(.vertical, 10)
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
                Text("Wireless & USB Display")
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
                StreamingStatusRow(label: "Mode", value: session.connectionMode == .wifi ? "Wi-Fi" : "USB")
                StreamingStatusRow(label: "Resolution", value: "\(session.config.resolution.width)×\(session.config.resolution.height)")
                StreamingStatusRow(label: "FPS", value: String(format: "%.0f", appState.streamFPS))
                StreamingStatusRow(label: "Frames sent", value: "\(appState.framesSent)")
                StreamingStatusRow(label: "Bitrate", value: "\(session.config.maxBitrateBps / 1_000_000) Mbps")
                StreamingStatusRow(label: "Transport", value: session.connectionMode.rawValue.uppercased())
            } else if let error = appState.lastError {
                VStack(alignment: .leading, spacing: 6) {
                    HStack(alignment: .top, spacing: 6) {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .foregroundStyle(.red)
                        Text(error)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                            .fixedSize(horizontal: false, vertical: true)
                        Spacer()
                    }
                    Button {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(error, forType: .string)
                    } label: {
                        Label("Copy error", systemImage: "doc.on.doc")
                            .font(.caption2)
                    }
                    .buttonStyle(.borderless)
                    .foregroundStyle(.secondary)
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
    let receiverHost: String
    let pairingPin: String
    let use60fps: Bool
    let displayMode: DisplayMode
    let selectedResolution: Resolution
    let selectedCodec: VideoCodec
    let transportMode: TransportSelection

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
                    let fps = use60fps ? 60 : 30
                    let config = StreamConfig.recommended(resolution: selectedResolution, fps: fps, codec: selectedCodec)
                    let wifiHost = transportMode == .usb ? nil : (receiverHost.isEmpty ? nil : receiverHost)
                    let pin = pairingPin.isEmpty ? nil : pairingPin
                    Task {
                        await appState.connectAndStream(
                            config: config,
                            displayMode: displayMode,
                            transportMode: transportMode,
                            wifiHost: wifiHost,
                            pairingPin: pin
                        )
                    }
                } label: {
                    let label = displayMode == .extend ? "Start Extending" : "Start Mirroring"
                    Label(label, systemImage: "play.fill")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .disabled(
                    (transportMode != .usb && receiverHost.isEmpty) ||
                    pairingPin.isEmpty ||
                    appState.connectionState == .discovering
                )
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
