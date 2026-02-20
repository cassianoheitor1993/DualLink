// PoCVirtualDisplayApp — Sprint 0.1.4
//
// PoC: CGVirtualDisplay in a proper .app bundle
//
// Hypothesis: CGVirtualDisplay fails to register in CGGetActiveDisplayList
// when run as a CLI tool because it lacks a WindowServer GUI session.
// A proper .app bundle (connected to WindowServer) should allow the
// virtual display to be recognized by the system.
//
// Build & Run:
//   ./build_and_run.sh
//   (or see README.md for manual steps)

import AppKit
import CoreGraphics
import Foundation
import VirtualDisplayHelper  // ObjC helper: DualLinkCreateVirtualDisplayMode()

// MARK: - App Setup

// Connect to WindowServer by creating an NSApplication.
// This is the critical difference vs a CLI script.
let app = NSApplication.shared
app.setActivationPolicy(.accessory)  // No dock icon / no menu bar — background app

// Run display test after a short delay (give WindowServer time to connect)
DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
    runVirtualDisplayTest()
    DispatchQueue.main.asyncAfter(deadline: .now() + 3.0) {
        app.terminate(nil)
    }
}

app.run()

// MARK: - Test

func runVirtualDisplayTest() {
    print("=== CGVirtualDisplay App Bundle PoC ===")
    print("macOS:", ProcessInfo.processInfo.operatingSystemVersionString)
    print("Bundle ID:", Bundle.main.bundleIdentifier ?? "(none)")
    print("Is GUI app:", NSRunningApplication.current.activationPolicy != .prohibited)
    print("")

    // Baseline display count
    var countBefore: UInt32 = 0
    CGGetActiveDisplayList(0, nil, &countBefore)
    print("Displays before:", countBefore)
    print("")

    // MARK: Create Descriptor

    guard let descClass = NSClassFromString("CGVirtualDisplayDescriptor") as? NSObject.Type else {
        print("[❌] CGVirtualDisplayDescriptor not found in runtime")
        return
    }

    let desc = descClass.init()
    desc.setValue("DualLink PoC", forKey: "name")
    desc.setValue(UInt32(1920), forKey: "maxPixelsWide")
    desc.setValue(UInt32(1080), forKey: "maxPixelsHigh")
    desc.setValue(UInt32(0x1AB7), forKey: "productID")
    desc.setValue(UInt32(0xFFFF), forKey: "vendorID")
    desc.setValue(UInt32(202602), forKey: "serialNum")

    let size = NSValue(size: CGSize(width: 527, height: 297))  // 24" @96dpi
    desc.setValue(size, forKey: "sizeInMillimeters")

    let termBlock: @convention(block) () -> Void = {
        print("[--] CGVirtualDisplay terminationHandler called")
    }
    desc.setValue(termBlock as AnyObject, forKey: "terminationHandler")
    print("[✅] CGVirtualDisplayDescriptor configured")

    // MARK: Create Display

    guard let dispClass = NSClassFromString("CGVirtualDisplay") as? NSObject.Type else {
        print("[❌] CGVirtualDisplay not found in runtime")
        return
    }

    let initSel = NSSelectorFromString("initWithDescriptor:")
    guard dispClass.instancesRespond(to: initSel) else {
        print("[❌] initWithDescriptor: not available")
        return
    }

    let allocSel = NSSelectorFromString("alloc")
    guard let allocated = dispClass.perform(allocSel)?.takeUnretainedValue() as? NSObject else {
        print("[❌] alloc failed")
        return
    }

    guard let display = allocated.perform(initSel, with: desc)?.takeRetainedValue() as? NSObject else {
        print("[❌] initWithDescriptor: returned nil")
        return
    }
    print("[✅] CGVirtualDisplay instantiated via initWithDescriptor:")

    // MARK: Get Display ID

    guard let displayID = display.value(forKey: "displayID") as? CGDirectDisplayID else {
        print("[❌] Could not read displayID")
        return
    }

    let isValid = displayID != kCGNullDirectDisplay
    print(isValid ? "[✅]" : "[❌]", "displayID:", displayID, isValid ? "(valid)" : "(null — kCGNullDirectDisplay)")

    // MARK: Create Mode via ObjC helper (handles primitive args)

    guard let mode = DualLinkCreateVirtualDisplayMode(1920, 1080, 30.0) else {
        print("[❌] DualLinkCreateVirtualDisplayMode returned nil")
        return
    }
    print("[✅] CGVirtualDisplayMode created: 1920×1080 @ 30fps")

    // MARK: Apply Settings

    guard let settClass = NSClassFromString("CGVirtualDisplaySettings") as? NSObject.Type else {
        print("[❌] CGVirtualDisplaySettings not found")
        return
    }

    let sett = settClass.init()
    sett.setValue(false, forKey: "hiDPI")
    sett.setValue([mode], forKey: "modes")  // Pass the actual 1920×1080@30 mode

    let applySelector = NSSelectorFromString("applySettings:")
    if display.responds(to: applySelector) {
        display.perform(applySelector, with: sett)
        print("[✅] applySettings: called")
    } else {
        print("[❌] applySettings: not available")
    }

    // MARK: Check System Display Lists

    // Give WindowServer ~200ms to process the new display
    Thread.sleep(forTimeInterval: 0.2)

    var activeIDs = [CGDirectDisplayID](repeating: 0, count: 16)
    var activeCount: UInt32 = 0
    CGGetActiveDisplayList(16, &activeIDs, &activeCount)
    let activeList = activeIDs.prefix(Int(activeCount)).map { $0 }

    var onlineIDs = [CGDirectDisplayID](repeating: 0, count: 16)
    var onlineCount: UInt32 = 0
    CGGetOnlineDisplayList(16, &onlineIDs, &onlineCount)
    let onlineList = onlineIDs.prefix(Int(onlineCount)).map { $0 }

    print("")
    print("Active displays:", activeList)
    print("Online displays:", onlineList)
    print("")

    let inActive = activeList.contains(displayID)
    let inOnline = onlineList.contains(displayID)

    print(inActive ? "[✅]" : "[❌]", "Virtual display (ID:\(displayID)) in CGGetActiveDisplayList")
    print(inOnline ? "[✅]" : "[❌]", "Virtual display (ID:\(displayID)) in CGGetOnlineDisplayList")

    if inActive || inOnline {
        print("")
        print("🎉 SUCCESS: CGVirtualDisplay working in .app bundle!")
        print("   → VirtualDisplayManager can be implemented in mac-client")
        print("   → Required entitlements identified")
    } else {
        print("")
        print("⚠️  Display not in system lists.")
        print("   Possible remaining issues:")
        print("   1. Mode config needed: try initWithWidth:height:refreshRate: with valid values")
        print("   2. Missing entitlement: check with `codesign -d --ent - <binary>`")
        print("   3. Need App Sandbox / hardened runtime settings")
    }

    // Keep alive so user can check System Settings > Displays
    print("")
    print("Keeping display alive 3s — check System Settings > Displays now...")
}
