import Foundation
import CoreGraphics
import AppKit
import DualLinkCore
import VirtualDisplayObjC

// MARK: - VirtualDisplayManager

/// Manages the lifecycle of a virtual display on macOS via CGVirtualDisplay.
///
/// ## Requirements
/// - macOS 14+
/// - Must run in a GUI app (NSApplication connected to WindowServer)
/// - No special entitlements — ad-hoc signing sufficient (validated Sprint 0.1.4)
///
/// ## Validated Recipe (GT-1005)
/// 1. NSApplication.shared must be running (handled by SwiftUI host app)
/// 2. CGVirtualDisplayDescriptor with name + pixel dims + physical size
/// 3. CGVirtualDisplay(descriptor:) via ObjC runtime
/// 4. CGVirtualDisplayMode(width:height:refreshRate:) via DualLinkCreateDisplayMode()
/// 5. CGVirtualDisplaySettings.applySettings: → display in CGGetActiveDisplayList
///
/// ## Usage
/// ```swift
/// let manager = VirtualDisplayManager()
/// try await manager.create(resolution: .fhd, refreshRate: 60)
/// let id = manager.activeDisplayID   // use for ScreenCaptureKit filter
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

        public static func == (lhs: State, rhs: State) -> Bool {
            switch (lhs, rhs) {
            case (.idle, .idle), (.creating, .creating): return true
            case (.active(let a), .active(let b)): return a == b
            case (.error(let a), .error(let b)): return a == b
            default: return false
            }
        }
    }

    @Published public private(set) var state: State = .idle

    // MARK: - Private Storage

    /// Retains the CGVirtualDisplay object alive.
    /// Display is destroyed when this is nil (ARC dealloc → terminationHandler).
    private var _virtualDisplay: NSObject?

    // MARK: - Init

    public init() {}

    // MARK: - Public API

    /// Creates the virtual display.
    ///
    /// - Parameters:
    ///   - resolution: Target resolution (.fhd, .qhd, or .uhd).
    ///   - refreshRate: Target refresh rate in Hz (30 or 60).
    ///   - hiDPI: Enable HiDPI/Retina scaling (default: false).
    ///
    /// - Throws: `VirtualDisplayError` on failure.
    public func create(
        resolution: Resolution,
        refreshRate: Int = 60,
        hiDPI: Bool = false
    ) async throws {
        guard state == .idle else { return }
        state = .creating

        do {
            let (display, id) = try buildVirtualDisplay(
                resolution: resolution,
                refreshRate: refreshRate,
                hiDPI: hiDPI
            )
            _virtualDisplay = display
            state = .active(displayID: id)
        } catch {
            state = .error(error.localizedDescription)
            throw error
        }
    }

    /// Destroys the virtual display and releases WindowServer resources.
    public func destroy() async {
        _virtualDisplay = nil   // ARC → terminationHandler → WindowServer cleanup
        state = .idle
    }

    /// The CGDirectDisplayID of the active virtual display, or nil.
    public var activeDisplayID: CGDirectDisplayID? {
        if case .active(let id) = state { return id }
        return nil
    }

    // MARK: - Implementation

    private func buildVirtualDisplay(
        resolution: Resolution,
        refreshRate: Int,
        hiDPI: Bool
    ) throws -> (display: NSObject, displayID: CGDirectDisplayID) {

        // ── Step 1: Descriptor ──────────────────────────────────────────────
        guard let descClass = NSClassFromString("CGVirtualDisplayDescriptor") as? NSObject.Type else {
            throw VirtualDisplayError.apiUnavailable("CGVirtualDisplayDescriptor not found")
        }
        let descriptor = descClass.init()
        descriptor.setValue("DualLink", forKey: "name")
        descriptor.setValue(UInt32(resolution.width), forKey: "maxPixelsWide")
        descriptor.setValue(UInt32(resolution.height), forKey: "maxPixelsHigh")
        descriptor.setValue(UInt32(0x1AB7), forKey: "productID")
        descriptor.setValue(UInt32(0xFFFF), forKey: "vendorID")
        descriptor.setValue(UInt32(20260220), forKey: "serialNum")
        descriptor.setValue(NSValue(size: physicalSizeMM(for: resolution)), forKey: "sizeInMillimeters")

        // terminationHandler: fired by WindowServer when display is unregistered externally.
        let termBlock: @convention(block) () -> Void = { [weak self] in
            Task { @MainActor [weak self] in
                self?._virtualDisplay = nil
                self?.state = .idle
            }
        }
        descriptor.setValue(termBlock as AnyObject, forKey: "terminationHandler")

        // ── Step 2: Display ─────────────────────────────────────────────────
        guard let dispClass = NSClassFromString("CGVirtualDisplay") as? NSObject.Type else {
            throw VirtualDisplayError.apiUnavailable("CGVirtualDisplay not found")
        }
        let initSel = NSSelectorFromString("initWithDescriptor:")
        guard dispClass.instancesRespond(to: initSel) else {
            throw VirtualDisplayError.apiUnavailable("initWithDescriptor: not available")
        }
        let allocSel = NSSelectorFromString("alloc")
        guard let raw = dispClass.perform(allocSel)?.takeUnretainedValue() as? NSObject,
              let display = raw.perform(initSel, with: descriptor)?.takeRetainedValue() as? NSObject else {
            throw VirtualDisplayError.creationFailed("CGVirtualDisplay init returned nil")
        }

        // ── Step 3: Mode ────────────────────────────────────────────────────
        // DualLinkCreateDisplayMode uses objc_msgSend to pass primitive args
        // (UInt, UInt, Double) to initWithWidth:height:refreshRate: (GT-1002).
        guard let mode = DualLinkCreateDisplayMode(
            UInt(resolution.width),
            UInt(resolution.height),
            Double(refreshRate)
        ) else {
            throw VirtualDisplayError.creationFailed("CGVirtualDisplayMode creation failed")
        }

        // ── Step 4: Settings + apply ────────────────────────────────────────
        guard let settClass = NSClassFromString("CGVirtualDisplaySettings") as? NSObject.Type else {
            throw VirtualDisplayError.apiUnavailable("CGVirtualDisplaySettings not found")
        }
        let settings = settClass.init()
        settings.setValue(hiDPI, forKey: "hiDPI")
        settings.setValue([mode], forKey: "modes")  // non-nil mode is CRITICAL (GT-1005)

        let applySel = NSSelectorFromString("applySettings:")
        guard display.responds(to: applySel) else {
            throw VirtualDisplayError.apiUnavailable("applySettings: not available")
        }
        display.perform(applySel, with: settings)

        // ── Step 5: Verify ──────────────────────────────────────────────────
        Thread.sleep(forTimeInterval: 0.1)  // give WindowServer time to register

        guard let displayID = display.value(forKey: "displayID") as? CGDirectDisplayID,
              displayID != kCGNullDirectDisplay else {
            throw VirtualDisplayError.displayIDNotFound
        }

        var ids = [CGDirectDisplayID](repeating: 0, count: 16)
        var count: UInt32 = 0
        CGGetActiveDisplayList(16, &ids, &count)
        guard ids.prefix(Int(count)).contains(displayID) else {
            throw VirtualDisplayError.creationFailed(
                "displayID \(displayID) not in CGGetActiveDisplayList — " +
                "ensure NSApplication is running and display mode has valid dimensions"
            )
        }

        return (display, displayID)
    }

    /// Approximate physical display size at 96 DPI in millimeters.
    private func physicalSizeMM(for resolution: Resolution, dpi: Double = 96) -> CGSize {
        let mmPerInch = 25.4
        return CGSize(
            width:  Double(resolution.width)  / dpi * mmPerInch,
            height: Double(resolution.height) / dpi * mmPerInch
        )
    }
}

// MARK: - VirtualDisplayError

public enum VirtualDisplayError: LocalizedError {
    case apiUnavailable(String)
    case creationFailed(String)
    case displayIDNotFound
    case unsupportedOSVersion(minimum: String, current: String)

    public var errorDescription: String? {
        switch self {
        case .apiUnavailable(let detail):
            return "CGVirtualDisplay API unavailable: \(detail)"
        case .creationFailed(let reason):
            return "Virtual display creation failed: \(reason)"
        case .displayIDNotFound:
            return "Virtual display was created but display ID could not be obtained"
        case .unsupportedOSVersion(let min, let current):
            return "Virtual display requires \(min). Current: \(current)"
        }
    }
}
