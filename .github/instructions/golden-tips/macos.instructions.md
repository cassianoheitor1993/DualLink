---
applyTo: "mac-client/**"
---

# Golden Tips — macOS

> Lições aprendidas ao trabalhar com macOS, Swift, ScreenCaptureKit, VideoToolbox, CGVirtualDisplay.
> Consultar ANTES de debugar qualquer problema no mac-client.

---

### GT-1001: CGVirtualDisplay — Cria display com ID válido mas não aparece em CGGetActiveDisplayList/CGGetOnlineDisplayList

- **Data:** 2026-02-20
- **Contexto:** Sprint 0.1 — PoC para criar display virtual usando CGVirtualDisplay (macOS 26.3 / Build 25D125)
- **Sintoma:** `initWithDescriptor:` → displayID retorna valor não-zero (7, 8, etc.) e `applySettings:` é chamado com sucesso. Mas `CGGetActiveDisplayList` e `CGGetOnlineDisplayList` não incluem o displayID — o display não aparece no sistema. Display ID incrementa a cada execução (6, 7, 8...) sugerindo que CG rastreia as alocações.
- **Causa raiz:** Duas hipóteses prováveis (a serem validadas em Sprint 0.1.4):
  1. **Entitlement faltando** — CGVirtualDisplay provavelmente requer `com.apple.developer.CoreGraphics.virtual-display` ou similar. Scripts CLI sem app bundle assinado não recebem essa autorização do WindowServer.
  2. **Modo de display inválido** — `CGVirtualDisplayMode` foi criado via `init()` sem dimensions. Pode ser necessário passar modo com `initWithWidth:height:refreshRate:` para o display ser registrado.
- **Solução:** Para validar completamente:
  1. Criar `.app` bundle com `Info.plist` e entitlements
  2. Usar `CGVirtualDisplayMode initWithWidth:height:refreshRate:` com dimensões reais — chamar via Objective-C wrapper ou extension
  3. Verificar em Xcode signed app se display aparece em System Settings > Displays
- **Pista-chave:** O display ID não-zero indica que a **API funciona** — o problema é de autorização/configuração, não de ausência de API. Próximo passo: app bundle assinado.
- **Tags:** #CGVirtualDisplay #entitlements #display #windowserver #sprint-0.1

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

**Total de tips:** 4
**Última atualização:** 2026-02-20
**Economia estimada:** 5h (entitlements, selectors, pixel format, performance baseline)
