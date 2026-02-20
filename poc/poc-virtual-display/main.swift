#!/usr/bin/swift
//
// CGVirtualDisplay PoC — Sprint 0.1.3
//
// Objetivo: Validar que CGVirtualDisplay pode criar um monitor virtual
// que o macOS trata como display real.
//
// Execução: swift main.swift
// Requer: macOS 14+, Xcode CLT
//

import CoreGraphics
import Foundation

// MARK: - Helper

func log(_ message: String, ok: Bool? = nil) {
    if let ok = ok {
        print(ok ? "[✅ OK]" : "[❌ FAIL]", message)
    } else {
        print("[--]   ", message)
    }
}

func check(_ condition: Bool, _ message: String) {
    log(message, ok: condition)
    if !condition {
        print("\n       ⚠️  Test failed — check output above")
    }
}

// MARK: - Step 1: listar displays antes

print("=== CGVirtualDisplay PoC ===\n")
print("macOS:", ProcessInfo.processInfo.operatingSystemVersionString)

var displayCount: UInt32 = 0
CGGetActiveDisplayList(0, nil, &displayCount)
log("Displays before: \(displayCount)")

// MARK: - Step 2: criar CGVirtualDisplay

// CGVirtualDisplay é uma classe disponível no macOS 14+.
// O código abaixo tenta usar a API pública. Se falhar, tentar via Objective-C runtime.

if #available(macOS 14.0, *) {
    log("macOS 14+ detected — testing CGVirtualDisplay API", ok: true)

    // --- Tentativa 1: API pública (se disponível) ---
    // A API foi tornada pública de forma gradual. Verificar se está acessível.

    // Verificar se CGVirtualDisplayDescriptor existe no runtime
    let descriptorClass: AnyClass? = NSClassFromString("CGVirtualDisplayDescriptor")
    check(descriptorClass != nil, "CGVirtualDisplayDescriptor class found in runtime")

    if descriptorClass != nil {
        // Tentar criar via NSObject / Objective-C bridge
        testVirtualDisplayCreation()
    } else {
        log("CGVirtualDisplayDescriptor not available — probing alternatives")
        log("Alternative: Try DriverKit virtual display extension")
        log("Alternative: Try dummy EDID injection via MonitorControl")
        log("Recommendation: Use CGVirtualDisplay when available in future macOS")
    }
} else {
    log("macOS 14+ required for CGVirtualDisplay", ok: false)
    log("Current version does not support this API")
}

// MARK: - Step 3: verificar exibição no sistema após destruição

var displayCountAfter: UInt32 = 0
CGGetActiveDisplayList(0, nil, &displayCountAfter)
log("Displays after destruction: \(displayCountAfter) (expected: same as before → \(displayCount))")
check(displayCountAfter == displayCount, "Display count restored after virtual display destroyed")

// MARK: - Test Function

func testVirtualDisplayCreation() {
    // Tentar criar CGVirtualDisplayDescriptor via Objective-C runtime
    guard let descriptorClass = NSClassFromString("CGVirtualDisplayDescriptor") as? NSObject.Type else {
        log("Cannot instantiate CGVirtualDisplayDescriptor", ok: false)
        return
    }

    let descriptor = descriptorClass.init()

    // Configurar propriedades via setValue:forKey: (KVC)
    // API verificada via class_copyMethodList
    descriptor.setValue("DualLink Test Display", forKey: "name")
    descriptor.setValue(UInt32(1920), forKey: "maxPixelsWide")
    descriptor.setValue(UInt32(1080), forKey: "maxPixelsHigh")
    descriptor.setValue(UInt32(0x1234), forKey: "productID")
    descriptor.setValue(UInt32(0x5678), forKey: "vendorID")
    descriptor.setValue(UInt32(42), forKey: "serialNum")

    // Tamanho físico em mm (597x336 ≈ 27" @ 96dpi)
    let sizeValue = NSValue(size: CGSize(width: 597, height: 336))
    descriptor.setValue(sizeValue, forKey: "sizeInMillimeters")

    // Terminação handler — essencial para liberar o display corretamente
    let terminationBlock: @convention(block) () -> Void = {
        print("[--]    CGVirtualDisplay terminationHandler fired")
    }
    descriptor.setValue(terminationBlock as AnyObject, forKey: "terminationHandler")

    log("CGVirtualDisplayDescriptor configured: 1920x1080", ok: true)

    // Verificar se CGVirtualDisplay pode ser instanciado com o descriptor
    guard let displayClass = NSClassFromString("CGVirtualDisplay") as? NSObject.Type else {
        log("CGVirtualDisplay class not found in runtime", ok: false)
        log("⚠️  This API may not be publicly available yet")
        return
    }

    // Usar initWithDescriptor: (confirmed via class_copyMethodList)
    let displayInit = NSSelectorFromString("initWithDescriptor:")
    guard displayClass.instancesRespond(to: displayInit) else {
        log("CGVirtualDisplay does not respond to initWithDescriptor:", ok: false)
        return
    }

    // Perform initWithDescriptor: via ObjC runtime bridging
    // Swift blocks alloc() — use perform on class to send alloc, then perform initWithDescriptor:
    let allocSel = NSSelectorFromString("alloc")
    guard let allocated = displayClass.perform(allocSel)?.takeUnretainedValue() as? NSObject else {
        log("CGVirtualDisplay alloc failed", ok: false)
        return
    }
    guard let display = allocated.perform(displayInit, with: descriptor)?.takeRetainedValue() as? NSObject else {
        log("CGVirtualDisplay initWithDescriptor: returned nil", ok: false)
        return
    }
    log("CGVirtualDisplay instantiated via initWithDescriptor:", ok: true)

    // Tentar obter displayID
    if let displayID = display.value(forKey: "displayID") as? CGDirectDisplayID {
        log("Display ID obtained: \(displayID)", ok: true)
        check(displayID != kCGNullDirectDisplay, "Display ID is valid (non-null)")
        if displayID == kCGNullDirectDisplay {
            log("   → Likely missing entitlement: com.apple.developer.CoreGraphics.virtual-display")
            log("   → CLI scripts cannot receive WindowServer authorization without a signed .app bundle")
        }
    } else {
        log("Could not obtain displayID from virtual display", ok: false)
    }

    // Configurar modos de display
    guard let modeClass = NSClassFromString("CGVirtualDisplayMode") as? NSObject.Type else {
        log("CGVirtualDisplayMode not found — skipping mode test")
        return
    }

    let modeInit = NSSelectorFromString("initWithWidth:height:refreshRate:")
    guard modeClass.instancesRespond(to: modeInit) else {
        log("CGVirtualDisplayMode does not respond to expected initializer", ok: false)
        return
    }

    // Create 1920x1080@30 mode
    // initWithWidth:height:refreshRate: takes UInt32,UInt32,Double — can't bridge via perform()
    // Use plain init() and probe properties separately
    let mode = modeClass.init()
    // Can't set width/height/refreshRate via KVC on CGVirtualDisplayMode (readonly props)
    // The mode created via init() may be empty but enough to test applySettings: call
    log("CGVirtualDisplayMode created (empty — initWithWidth:height:refreshRate: requires direct ObjC)", ok: true)

    guard let settingsClass = NSClassFromString("CGVirtualDisplaySettings") as? NSObject.Type else {
        log("CGVirtualDisplaySettings not found — skipping settings test")
        return
    }

    let settings = settingsClass.init()
    settings.setValue(false, forKey: "hiDPI")
    settings.setValue([mode], forKey: "modes")
    log("CGVirtualDisplaySettings configured (hiDPI: false, 1 mode)", ok: true)

    // Aplicar settings — seletor correto é applySettings: (sem displayModes:)
    let applySelector = NSSelectorFromString("applySettings:")
    if display.responds(to: applySelector) {
        log("applySettings: selector found ✅", ok: true)
        display.perform(applySelector, with: settings)
        log("applySettings: called — checking displayID again...")
        if let displayID = display.value(forKey: "displayID") as? CGDirectDisplayID {
            check(displayID != kCGNullDirectDisplay, "Display ID valid after applySettings:")
        }
    } else {
        log("applySettings: selector not found", ok: false)
    }

    // Manter vivo por 2 segundos para verificar no Activity Monitor / System Info
    log("Keeping display alive for 2 seconds — check System Preferences > Displays")

    // *** Verificar display lists enquanto o virtual display ainda está vivo ***
    var activeWhileAlive = [CGDirectDisplayID](repeating: 0, count: 16)
    var activeCountWhileAlive: UInt32 = 0
    CGGetActiveDisplayList(16, &activeWhileAlive, &activeCountWhileAlive)
    let activeList = activeWhileAlive.prefix(Int(activeCountWhileAlive)).map { $0 }
    log("Active displays while alive: \(activeList)")

    var onlineWhileAlive = [CGDirectDisplayID](repeating: 0, count: 16)
    var onlineCountWhileAlive: UInt32 = 0
    CGGetOnlineDisplayList(16, &onlineWhileAlive, &onlineCountWhileAlive)
    let onlineList = onlineWhileAlive.prefix(Int(onlineCountWhileAlive)).map { $0 }
    log("Online displays while alive: \(onlineList)")

    if let displayID = display.value(forKey: "displayID") as? CGDirectDisplayID {
        let inActive = activeList.contains(displayID)
        let inOnline = onlineList.contains(displayID)
        check(inActive, "Virtual display (ID:\(displayID)) in CGGetActiveDisplayList")
        check(inOnline, "Virtual display (ID:\(displayID)) in CGGetOnlineDisplayList")
        if !inActive && !inOnline {
            log("   → Display ID exists but not in system lists")
            log("   → Possible cause: missing entitlement or mode configuration required")
        }
    }

    Thread.sleep(forTimeInterval: 2.0)

    // Destruir o display
    // display = nil (ARC dealloca automaticamente quando sai do escopo)
    log("Virtual display destroyed (ARC dealloc)")
}

// MARK: - Diagnóstico Final

print("\n=== Diagnosis ===\n")

// Listar todas as classes CG disponíveis no runtime (para investigação)
let cgClasses = [
    "CGVirtualDisplay",
    "CGVirtualDisplayDescriptor",
    "CGVirtualDisplaySettings",
    "CGVirtualDisplayMode",
    "CGDisplayStream",
    "CGDirectDisplay",
].map { name -> String in
    let found = NSClassFromString(name) != nil
    return "\(found ? "✅" : "❌") \(name)"
}

print("CoreGraphics runtime classes:")
cgClasses.forEach { print("  ", $0) }

print("\n=== PoC Complete ===")
print("\nNext steps:")
print("  - Update README.md with actual results")
print("  - If CGVirtualDisplay works: implement VirtualDisplayManager in mac-client")
print("  - If CGVirtualDisplay unavailable: evaluate DriverKit extension")
