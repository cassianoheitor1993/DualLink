---
applyTo: "mac-client/**"
---

# Golden Tips — macOS

> Lições aprendidas ao trabalhar com macOS, Swift, ScreenCaptureKit, VideoToolbox, CGVirtualDisplay.
> Consultar ANTES de debugar qualquer problema no mac-client.

---

### GT-1001: CGVirtualDisplay — Precisa de .app bundle + NSApplication + CGVirtualDisplayMode com dimensões reais

- **Data:** 2026-02-20
- **Contexto:** Sprint 0.1 — PoC para criar display virtual usando CGVirtualDisplay (macOS 26.3)
- **Sintoma:** CLI script: displayID não-zero mas não aparece em CGGetActiveDisplayList. App bundle com modo vazio: idem. App bundle com modo real: ✅ funciona.
- **Causa raiz:** DUAS coisas eram necessárias simultaneamente:
  1. **NSApplication + WindowServer session** — `NSApplication.shared` precisa ser inicializado para o processo ter acesso ao WindowServer. CLI tools não têm essa conexão.
  2. **CGVirtualDisplayMode com dimensões reais** — `CGVirtualDisplaySettings` precisa conter um modo criado via `initWithWidth:height:refreshRate:` com valores válidos (ex: 1920, 1080, 30.0). Modo vazio (`init()` plain) não registra o display.
- **Solução:** Ver GT-1005 para o recipe completo.
- **Pista-chave:** Quando um display ID é válido mas não aparece no sistema, checar (1) se NSApplication está inicializado e (2) se o modo de display tem dimensões válidas.
- **Tags:** #CGVirtualDisplay #entitlements #display #windowserver #NSApplication #sprint-0.1

---

### GT-1002: CGVirtualDisplay — API completa descoberta via `class_copyMethodList`

- **Data:** 2026-02-20
- **Contexto:** Sprint 0.1 — API reverse engineered via runtime reflection
- **Sintoma:** `applySettings:displayModes:` não existe; `initWithDescriptor:` existe mas PoC usou `init()` sem argumento
- **Causa raiz:** PoC usou `alloc().init()` plain em vez de `initWithDescriptor:`, e seletor de settings estava errado
- **Solução (API completa):**
  ```
  CGVirtualDisplayDescriptor:
    init, setName:, setMaxPixelsWide:, setMaxPixelsHigh:,
    setSizeInMillimeters:, setProductID:, setVendorID:, setSerialNumber:,
    setTerminationHandler:, setDispatchQueue:, setBluePrimary:,
    setGreenPrimary:, setRedPrimary:, setWhitePoint:, setDisplayInfoValue:forKey:

  CGVirtualDisplay:
    initWithDescriptor:, applySettings:, displayID,
    terminationHandler, rotation

  CGVirtualDisplaySettings:
    init, setHiDPI:, setModes:, setRotation:, setIsReference:, setRefreshDeadline:

  CGVirtualDisplayMode:
    initWithWidth:height:refreshRate:
    initWithWidth:height:refreshRate:transferFunction:
  ```
- **Pista-chave:** Usar `swift -e 'let cls = NSClassFromString("CGVirtualDisplay")!; var n: UInt32=0; let m=class_copyMethodList(cls,&n)!; ...'` para dumpar API em qualquer macOS sem otool/class-dump
- **Tags:** #CGVirtualDisplay #selector #objc-runtime #api-surface #sprint-0.1

---

### GT-1003: ScreenCaptureKit — Formato NV12 é `0x34323066` (`420f`), não `0x34323076`

- **Data:** 2026-02-20
- **Contexto:** Sprint 0.2 — PoC ScreenCaptureKit validou captura 30fps
- **Sintoma:** `CVPixelBufferGetPixelFormatType` retorna `0x34323066`, não `0x34323076` como esperado
- **Causa raiz:** Confusão entre dois formatos YpCbCr NV12:
  - `0x34323066` = `"420f"` = `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange` ← o que ScreenCaptureKit entrega
  - `0x34323076` = `"420v"` = `kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange`
- **Solução:** Usar `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange` (Full Range) na `SCStreamConfiguration`. VideoToolbox aceita esse formato diretamente — pipeline zero-copy confirmado.
- **Pista-chave:** Sempre verificar o formato retornado COM o frame real, não assumir pelo que foi configurado. ScreenCaptureKit pode overridar o formato para `420f`.
- **Tags:** #ScreenCaptureKit #NV12 #pixel-format #zero-copy #sprint-0.2

---

### GT-1004: ScreenCaptureKit — IOSurface-backed confirmado, 29fps @ 30fps target em macOS 26.3

- **Data:** 2026-02-20
- **Contexto:** Sprint 0.2 — PoC performance benchmark
- **Sintoma:** N/A (resultado positivo)
- **Causa raiz:** N/A
- **Solução:** `SCStreamConfiguration` com `pixelFormat = kCVPixelFormatType_420YpCbCr8BiPlanarFullRange` + `minimumFrameInterval = CMTime(1, 30)` entrega:
  - 29fps (target 30) ✅
  - IOSurface-backed = YES ✅ (zero-copy para VideoToolbox)
  - Frame size `1920x1080` mesmo quando display fonte é diferente
  - Max frame time ~41ms (1.25x frame interval) — aceitável para CLI script, app terá melhor consistência
- **Pista-chave:** Validar com `CVPixelBufferGetIOSurface(pixelBuffer) != nil` — se nil, a pipeline sofrerá uma cópia cara para VideoToolbox.
- **Tags:** #ScreenCaptureKit #performance #IOSurface #30fps #sprint-0.2

---

### GT-1005: CGVirtualDisplay — Recipe completo e validado (macOS 26.3)

- **Data:** 2026-02-20
- **Contexto:** Sprint 0.1.4 — CGVirtualDisplay confirmado funcional ✅
- **Recipe validado:**

  ```swift
  // 1. Inicializar NSApplication (OBRIGATÓRIO — cria sessão WindowServer)
  let app = NSApplication.shared
  app.setActivationPolicy(.accessory)

  // 2. Criar descriptor
  let desc = NSClassFromString("CGVirtualDisplayDescriptor")!.init() as! NSObject
  desc.setValue("DualLink", forKey: "name")
  desc.setValue(UInt32(1920), forKey: "maxPixelsWide")
  desc.setValue(UInt32(1080), forKey: "maxPixelsHigh")
  desc.setValue(NSValue(size: CGSize(width: 527, height: 297)), forKey: "sizeInMillimeters")
  // terminationHandler é importante para cleanup correto
  let term: @convention(block) () -> Void = { /* cleanup */ }
  desc.setValue(term as AnyObject, forKey: "terminationHandler")

  // 3. Criar display com initWithDescriptor:
  let dispClass = NSClassFromString("CGVirtualDisplay")! as! NSObject.Type
  let alloc = dispClass.perform(NSSelectorFromString("alloc"))!.takeUnretainedValue() as! NSObject
  let display = alloc.perform(NSSelectorFromString("initWithDescriptor:"), with: desc)!.takeRetainedValue() as! NSObject

  // 4. Criar modo de display via ObjC (não pode ser feito em Swift puro — args primitivos)
  // Usar helper ObjC: DualLinkCreateVirtualDisplayMode(1920, 1080, 30.0)
  // OU via objc_msgSend direto com types: (id, SEL, NSUInteger, NSUInteger, double)

  // 5. Aplicar settings
  let sett = NSClassFromString("CGVirtualDisplaySettings")!.init() as! NSObject
  sett.setValue(false, forKey: "hiDPI")
  sett.setValue([mode], forKey: "modes")  // OBRIGATÓRIO: modo com dimensões reais
  display.perform(NSSelectorFromString("applySettings:"), with: sett)

  // 6. Rodar app runloop (DisplayID só aparece no sistema após runloop iniciar)
  app.run()
  ```

- **Sem entitlements especiais necessários** — ad-hoc signing (`codesign --sign -`) é suficiente
- **Info.plist mínimo:** `CFBundleIdentifier`, `LSUIElement: true` (background app), `NSPrincipalClass: NSApplication`
- **Resultado em macOS 26.3:** display aparece em `CGGetActiveDisplayList` + `CGGetOnlineDisplayList` ✅
- **Pista-chave:** `sett.setValue([mode], forKey: "modes")` com modo real é o passo crítico. Modo vazio não registra o display.
- **Tags:** #CGVirtualDisplay #recipe #validated #NSApplication #sprint-0.1

---

### GT-1006: CGVirtualDisplay — macOS display count limit with external monitors

- **Data:** 2026-02-20
- **Contexto:** Sprint 2.1 — Extend mode virtual display creation failing
- **Sintoma:** `displayID 40 not in CGGetActiveDisplayList after 5s` — virtual display creates successfully (valid ID) but never appears in the active or online display list
- **Causa raiz:** macOS has a hard limit on concurrent displays. With 2 external monitors already connected, the virtual display could not be registered by WindowServer despite getting a valid display ID.
- **Solução:** Disconnect one or more external monitors before creating the virtual display. The display count limit varies by GPU/machine — Apple Silicon typically supports up to 2-3 external displays.
- **Pista-chave:** If the virtual display ID is non-zero but never appears in `CGGetActiveDisplayList` or `CGDisplayIsOnline`, check how many displays are already connected. `CGGetOnlineDisplayList` will show the current count.
- **Tags:** #CGVirtualDisplay #display-limit #external-monitors #sprint-2.1

---

### GT-1007: InputInjection — Cursor jumps 2-4 inches after mouse clicks (dual cause)

- **Data:** 2026-02-23
- **Contexto:** Sprint 2.3 — input forwarding from Linux receiver back to Mac
- **Sintoma:** After every mouse click (specifically on button release) on any display, the macOS cursor visibly jumps 2-4 inches in a consistent direction. The offset grows toward the edges of the screen (zero at center).
- **Causa raiz:** Two independent causes combine:
  1. **CGEvent click snap (Causa 1):** `CGEvent(mouseType: .leftMouseDown/Up, mouseCursorPosition: point)` causes macOS to physically move the cursor to `point` as a side effect of the click event. Any coordinate rounding error in `point` becomes a visible cursor snap. Fix: track `lastKnownCursorPoint` in `injectMouseMove` and use it for all click events instead of re-mapping the click's coordinates.
  2. **Aspect ratio letterboxing (Causa 2):** When `config.resolution` AR ≠ virtual display AR, ScreenCaptureKit (`scalesToFit = true`) letterboxes the content with black bars. The Linux side normalises pointer coordinates by the full frame dimensions (including bars). This causes a systematic offset (`x = px / frameWidth` instead of `px / contentWidth`) that grows at the display edges, producing ~2-4 inches of error. Fix: in `buildStreamConfig`, compute output dimensions that exactly preserve the display's AR (fit within the requested resolution bounding box), so there are never any black bars.
- **Solução:** See `InputInjection.swift` (`lastKnownCursorPoint` tracking) and `ScreenCaptureManager.swift` (`buildStreamConfig` AR-aware size calculation).
- **Pista-chave:** If clicks snap the cursor, check: (1) does the CGEvent carry `mouseCursorPosition`? (2) does the stream resolution match the display AR? Both must be correct simultaneously.
- **Tags:** #input-injection #CGEvent #coordinates #click-snap #aspect-ratio #letterboxing #sprint-2.3

---

### GT-1008: macOS "Full Keyboard Access" causes cursor warp to focused UI elements

- **Data:** 2026-02-23
- **Contexto:** Debugging cursor jump during/after DualLink screen sharing
- **Sintoma:** Cursor jumps to UI focus targets (e.g., VS Code sidebar items, buttons) after any click or keyboard navigation. Happens even with DualLink closed. Looks like a DualLink input injection bug but is not.
- **Causa raiz:** macOS System Settings → Keyboard → **"Full Keyboard Access"** (a.k.a. Keyboard Navigation) was enabled. This setting makes macOS warp the mouse cursor to follow keyboard focus changes — including programmatic focus shifts triggered by CGEvent injection.
- **Solução:** Disable "Full Keyboard Access" in System Settings → Keyboard. The cursor stops warping immediately.
- **Pista-chave:** If cursor warps to UI elements (not random positions), and the behavior persists even without DualLink running, it's a macOS Accessibility/Keyboard setting — NOT a code bug. Check "Full Keyboard Access" first, then "Shake mouse to locate."
- **Tags:** #accessibility #keyboard-navigation #cursor-warp #macos-settings #false-positive

---

**Total de tips:** 8
**Última atualização:** 2026-02-23
**Economia estimada:** 11h (entitlements, selectors, pixel format, app bundle setup, display mode config, display limit, click-snap, false-positive cursor warp)
