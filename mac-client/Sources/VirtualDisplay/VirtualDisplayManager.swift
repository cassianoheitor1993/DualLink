import Foundation
import CoreGraphics
import DualLinkCore

// MARK: - VirtualDisplayManager

/// Gerencia o ciclo de vida de um display virtual no macOS.
///
/// Usa `CGVirtualDisplay` (disponível macOS 14+) para criar um monitor
/// virtual que o sistema trata como um display físico real.
///
/// ## Uso
/// ```swift
/// let manager = VirtualDisplayManager()
/// try await manager.create(resolution: .fhd, refreshRate: 60)
/// // ... usar o display ...
/// await manager.destroy()
/// ```
@MainActor
public final class VirtualDisplayManager: ObservableObject {

    // MARK: - State

    public enum State: Equatable {
        case idle
        case creating
        case active(displayID: CGDirectDisplayID)
        case error(String)
    }

    @Published public private(set) var state: State = .idle

    // MARK: - Private

    private var virtualDisplay: AnyObject?  // CGVirtualDisplay — tipo opaque para evitar erro em macOS < 14
    private var displayID: CGDirectDisplayID?

    // MARK: - Init

    public init() {}

    // MARK: - Public API

    /// Cria o display virtual com a resolução e refresh rate especificados.
    /// - Parameters:
    ///   - resolution: Resolução desejada.
    ///   - refreshRate: Taxa de atualização em Hz (default: 60).
    ///   - hiDPI: Habilitar modo HiDPI/Retina (default: false).
    public func create(
        resolution: Resolution,
        refreshRate: Int = 60,
        hiDPI: Bool = false
    ) async throws {
        guard state == .idle else { return }
        state = .creating

        do {
            let id = try createCGVirtualDisplay(
                resolution: resolution,
                refreshRate: refreshRate,
                hiDPI: hiDPI
            )
            displayID = id
            state = .active(displayID: id)
        } catch {
            state = .error(error.localizedDescription)
            throw error
        }
    }

    /// Destrói o display virtual e libera recursos.
    public func destroy() async {
        virtualDisplay = nil
        displayID = nil
        state = .idle
    }

    /// Retorna o CGDirectDisplayID do display virtual, se ativo.
    public var activeDisplayID: CGDirectDisplayID? {
        if case .active(let id) = state { return id }
        return nil
    }

    // MARK: - Private Implementation

    private func createCGVirtualDisplay(
        resolution: Resolution,
        refreshRate: Int,
        hiDPI: Bool
    ) throws -> CGDirectDisplayID {
        // CGVirtualDisplay API disponível em macOS 14+
        // Verificar availability em runtime para compatibilidade
        if #available(macOS 14.0, *) {
            return try createVirtualDisplay_macOS14(
                resolution: resolution,
                refreshRate: refreshRate,
                hiDPI: hiDPI
            )
        } else {
            throw VirtualDisplayError.unsupportedOSVersion(
                minimum: "macOS 14.0",
                current: ProcessInfo.processInfo.operatingSystemVersionString
            )
        }
    }

    @available(macOS 14.0, *)
    private func createVirtualDisplay_macOS14(
        resolution: Resolution,
        refreshRate: Int,
        hiDPI: Bool
    ) throws -> CGDirectDisplayID {
        // NOTA: CGVirtualDisplay é uma API que está sendo tornada pública no macOS 14.
        // Durante o PoC (Sprint 0.1), validar exatamente quais classes/métodos estão disponíveis.
        //
        // Referências para investigar:
        //   - CGVirtualDisplayDescriptor
        //   - CGVirtualDisplay
        //   - CGVirtualDisplaySettings
        //   - CGVirtualDisplayMode
        //
        // O código abaixo é um template baseado na API esperada.
        // TODO: Validar e ajustar durante Sprint 0.1.3

        // Step 1: Criar descriptor
        // let descriptor = CGVirtualDisplayDescriptor()
        // descriptor.name = "DualLink Display"
        // descriptor.maxPixelsWide = UInt32(resolution.width)
        // descriptor.maxPixelsHigh = UInt32(resolution.height)
        // descriptor.sizeInMillimeters = physicalSize(for: resolution)

        // Step 2: Criar display
        // guard let display = CGVirtualDisplay(descriptor: descriptor) else {
        //     throw VirtualDisplayError.creationFailed("CGVirtualDisplay returned nil")
        // }
        // self.virtualDisplay = display

        // Step 3: Aplicar settings/modes
        // let settings = CGVirtualDisplaySettings()
        // settings.hiDPI = hiDPI
        // let mode = CGVirtualDisplayMode(
        //     width: UInt32(resolution.width),
        //     height: UInt32(resolution.height),
        //     refreshRate: Double(refreshRate)
        // )
        // display.apply(settings, displayModes: [mode])

        // Step 4: Obter displayID
        // return display.displayID

        // PLACEHOLDER até PoC ser validado
        throw VirtualDisplayError.creationFailed(
            "CGVirtualDisplay implementation pending PoC validation (Sprint 0.1)"
        )
    }

    /// Calcula o tamanho físico aproximado do display em milímetros
    /// baseado em um DPI de 96 (padrão).
    private func physicalSize(for resolution: Resolution, dpi: Double = 96) -> CGSize {
        let mmPerInch = 25.4
        let widthMM = Double(resolution.width) / dpi * mmPerInch
        let heightMM = Double(resolution.height) / dpi * mmPerInch
        return CGSize(width: widthMM, height: heightMM)
    }
}

// MARK: - VirtualDisplayError

public enum VirtualDisplayError: LocalizedError {
    case unsupportedOSVersion(minimum: String, current: String)
    case creationFailed(String)
    case displayIDNotFound
    case settingsApplicationFailed(String)

    public var errorDescription: String? {
        switch self {
        case .unsupportedOSVersion(let min, let current):
            return "Virtual display requires \(min). Current: \(current)"
        case .creationFailed(let reason):
            return "Failed to create virtual display: \(reason)"
        case .displayIDNotFound:
            return "Could not obtain display ID from virtual display"
        case .settingsApplicationFailed(let reason):
            return "Failed to apply display settings: \(reason)"
        }
    }
}
