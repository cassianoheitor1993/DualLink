# PoC: CGVirtualDisplay

**Sprint:** 0.1 — Pesquisa de Virtual Display no macOS  
**Pergunta:** É possível criar um display virtual via `CGVirtualDisplay` no macOS 14+ que o sistema trate como monitor real?

## Setup

```bash
cd poc/poc-virtual-display
swift main.swift
```

> Requer macOS 14.0+ e Xcode Command Line Tools.

## O que este PoC testa

1. **Criação do display virtual** — `CGVirtualDisplay` com resolução 1920x1080
2. **Visibilidade no sistema** — display aparece em `CGGetActiveDisplayList`
3. **Ciclo de vida** — criar, usar, destruir sem memory leak
4. **Permissões necessárias** — quais entitlements/permissões são necessários

## Resultado Esperado

```
[OK] CGVirtualDisplay created with ID: 12345678
[OK] Display visible in CGGetActiveDisplayList: YES
[OK] Resolution: 1920x1080 @ 60Hz
[OK] Display destroyed cleanly
```

## Resultado Obtido

> ⬜ Pendente — preencher após execução

```
...
```

## Decisão

> ⬜ Pendente

- [ ] ✅ Viável — prosseguir com implementação
- [ ] ⚠️ Viável com ressalvas — documentar limitações
- [ ] ❌ Inviável — usar alternativa (DriverKit)

## Referências

- [CGVirtualDisplay — Apple Developer](https://developer.apple.com/documentation/coregraphics/cgvirtualdisplay)
- [WWDC 2021 — Bring your iPad apps to Mac](https://developer.apple.com/videos/play/wwdc2021/10114/) (menção a virtual displays)
- [ScreenCaptureKit and virtual displays](https://developer.apple.com/documentation/screencapturekit)
