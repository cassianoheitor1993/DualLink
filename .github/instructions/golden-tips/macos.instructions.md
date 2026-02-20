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

**Total de tips:** 5
**Última atualização:** 2026-02-20
**Economia estimada:** 6h (entitlements, selectors, pixel format, app bundle setup, display mode config)
