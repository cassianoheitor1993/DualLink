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

// MARK: - Step 3: verificar exibição no sistema após criação

var displayCountAfter: UInt32 = 0
CGGetActiveDisplayList(0, nil, &displayCountAfter)
log("Displays after: \(displayCountAfter)")
check(displayCountAfter > displayCount, "Virtual display added to active display list")

// MARK: - Test Function

func testVirtualDisplayCreation() {
    // Tentar criar CGVirtualDisplayDescriptor via Objective-C runtime
    guard let descriptorClass = NSClassFromString("CGVirtualDisplayDescriptor") as? NSObject.Type else {
        log("Cannot instantiate CGVirtualDisplayDescriptor", ok: false)
        return
    }

    let descriptor = descriptorClass.init()

    // Configurar propriedades via setValue:forKey: (KVC)
    descriptor.setValue("DualLink Test Display", forKey: "name")
    descriptor.setValue(UInt32(1920), forKey: "maxPixelsWide")
    descriptor.setValue(UInt32(1080), forKey: "maxPixelsHigh")

    // Tamanho físico em mm (597x336 ≈ 27" @ 96dpi)
    let sizeValue = NSValue(size: CGSize(width: 597, height: 336))
    descriptor.setValue(sizeValue, forKey: "sizeInMillimeters")

    log("CGVirtualDisplayDescriptor configured: 1920x1080", ok: true)

    // Verificar se CGVirtualDisplay pode ser instanciado com o descriptor
    guard let displayClass = NSClassFromString("CGVirtualDisplay") as? NSObject.Type else {
        log("CGVirtualDisplay class not found in runtime", ok: false)
        log("⚠️  This API may not be publicly available yet")
        log("   Try: Private framework or DriverKit extension")
        return
    }

    // Tentar criar — espera um seletor init(descriptor:)
    let displayInit = NSSelectorFromString("initWithDescriptor:")
    guard displayClass.instancesRespond(to: displayInit) else {
        log("CGVirtualDisplay does not respond to init(descriptor:)", ok: false)
        return
    }

    let display = displayClass.init()
    log("CGVirtualDisplay instantiated", ok: true)

    // Tentar obter displayID
    if let displayID = display.value(forKey: "displayID") as? CGDirectDisplayID {
        log("Display ID obtained: \(displayID)", ok: true)
        check(displayID != kCGNullDirectDisplay, "Display ID is valid (non-null)")
    } else {
        log("Could not obtain displayID from virtual display", ok: false)
    }

    // Configurar modos de display
    guard let settingsClass = NSClassFromString("CGVirtualDisplaySettings") as? NSObject.Type else {
        log("CGVirtualDisplaySettings not found — skipping settings test")
        return
    }

    let settings = settingsClass.init()
    settings.setValue(false, forKey: "hiDPI")
    log("CGVirtualDisplaySettings configured (hiDPI: false)", ok: true)

    // Tentar aplicar as settings
    let applySelector = NSSelectorFromString("applySettings:displayModes:")
    if display.responds(to: applySelector) {
        log("applySettings:displayModes: selector found", ok: true)
        // Aplicar via performSelector — cuidado com retenção
    } else {
        log("applySettings:displayModes: selector not found", ok: false)
        log("   The API shape may differ — investigate display mode setup")
    }

    // Manter vivo por 2 segundos para verificar no Activity Monitor / System Info
    log("Keeping display alive for 2 seconds — check System Preferences > Displays")
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
