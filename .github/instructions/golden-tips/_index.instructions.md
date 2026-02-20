---
applyTo: "**"
---

# Golden Tips — Sistema de Lições Aprendidas

> Este arquivo é o **índice e manual** do sistema de Golden Tips.
> Golden Tips são automaticamente consultadas antes de debugging e investigação técnica.

## O Que São Golden Tips

Golden Tips são **lições aprendidas documentadas** durante resolução de problemas que:
- Encurtam o tempo de diagnóstico em problemas futuros
- Evitam repetir investigações já feitas
- Acumulam conhecimento tácito sobre APIs e plataformas
- Servem como **primeiro checklist** antes de qualquer debugging

## Arquivos de Tips

| Arquivo | Conteúdo | Quando consultar |
|---------|----------|-----------------|
| `macos.instructions.md` | Armadilhas macOS, Swift, ScreenCaptureKit, VideoToolbox | Bugs no mac-client |
| `linux.instructions.md` | Armadilhas Linux, Rust, GStreamer, VAAPI/NVDEC | Bugs no linux-receiver |
| `webrtc.instructions.md` | WebRTC, latência, conexão, streaming | Problemas de comunicação |
| `general.instructions.md` | Cross-platform, protocolo, build, CI | Problemas gerais |

## Quando Registrar

Registrar um Golden Tip **SEMPRE** que:

| Critério | Exemplo |
|----------|---------|
| Bug levou > 2 tentativas para resolver | "Achei que era encoding, era signaling" |
| Causa raiz ≠ hipótese inicial | "Parecia crash no decoder, era race condition no channel" |
| Comportamento não-documentado de API | "CGVirtualDisplay precisa de X que não está na doc" |
| Workaround para limitação de plataforma | "GStreamer NVDEC não funciona com formato Y" |
| Sequência de diagnóstico eficaz | "Primeiro checar X, depois Y, então Z" |

## Formato Obrigatório

```markdown
### GT-XXXX: [Título curto e descritivo]
- **Data:** YYYY-MM-DD
- **Contexto:** [Qual tarefa/feature/bug levou a esta descoberta]
- **Sintoma:** [O que o desenvolvedor via/experienciava]
- **Hipótese inicial:** [O que parecia ser o problema]
- **Causa raiz:** [O que realmente era o problema]
- **Solução:** [Código ou configuração que resolveu]
- **Pista-chave:** [O que deveria ter sido checado PRIMEIRO — o item mais valioso]
- **Tags:** #componente #api #tipo-problema
```

### Numeração

- macOS tips: GT-1xxx (ex: GT-1001, GT-1002)
- Linux tips: GT-2xxx (ex: GT-2001, GT-2002)
- WebRTC tips: GT-3xxx (ex: GT-3001, GT-3002)
- General tips: GT-9xxx (ex: GT-9001, GT-9002)

## Como Consultar (Protocolo de Debug)

**ANTES** de iniciar qualquer investigação:

```
1. Identificar o componente afetado (macOS? Linux? WebRTC? Geral?)
2. Abrir o arquivo de golden tips correspondente
3. Procurar tips com tags relacionadas ao sintoma
4. Usar as "pistas-chave" como primeiro checklist
5. Se encontrou tip relevante, aplicar a solução documentada
6. Se não encontrou, prosseguir com debugging normal
7. Ao resolver, avaliar se deve criar novo Golden Tip
```

## Métricas do Sistema

Manter atualizado no final de cada arquivo de tips:

```markdown
---
**Total de tips:** X
**Última atualização:** YYYY-MM-DD
**Economia estimada:** ~Y horas (baseado em reuso de tips)
```
