# DualLink — Copilot Instructions (Entry Point)

> Este arquivo é carregado automaticamente em toda sessão do Copilot.
> Ele funciona como **router** — contém apenas o mínimo necessário e aponta para módulos especializados.

---

## 🧭 Projeto

**DualLink** — App cross-platform que transforma um laptop Linux em monitor externo para macOS (espelhamento + extensão de tela) via USB-C ou Wi-Fi.

- Documentação completa: `docs/WORK_PLAN.md`, `docs/MILESTONES.md`, `docs/TECHNICAL_RESEARCH.md`
- Roadmap: Fase 0 (Research) → Fase 1 (MVP Wi-Fi) → Fase 2 (Extensão 60fps) → Fase 3 (USB-C) → Fase 4 (Polish)

---

## 📦 Sistema de Instruções Modulares

As instruções estão organizadas em módulos em `.github/instructions/`. **Carregue apenas o módulo necessário para a tarefa atual** — isso economiza tokens e mantém o foco.

### Estrutura

```
.github/instructions/
├── roles/                    # COMO executar (persona e abordagem)
│   ├── architect.instructions.md      # Decisões de arquitetura
│   ├── implementer.instructions.md    # Implementação de features
│   ├── debugger.instructions.md       # Diagnóstico e fix de bugs
│   └── reviewer.instructions.md       # Code review
├── platform/                 # ONDE executar (regras por plataforma)
│   ├── macos-swift.instructions.md    # macOS client (Swift)
│   ├── linux-rust.instructions.md     # Linux receiver (Rust)
│   └── shared-protocol.instructions.md # Protocolo compartilhado
├── workflows/                # O QUE fazer (processos)
│   ├── research.instructions.md       # Pesquisa e PoC
│   ├── testing.instructions.md        # Testes e QA
│   └── performance.instructions.md    # Otimização de performance
├── design/                   # POR QUÊ (regras de design)
│   ├── architecture.instructions.md   # Princípios arquiteturais
│   ├── patterns.instructions.md       # Design patterns do projeto
│   └── api-contracts.instructions.md  # Contratos entre módulos
└── golden-tips/              # LIÇÕES APRENDIDAS (auto-documentado)
    ├── _index.instructions.md         # Como usar golden tips
    ├── macos.instructions.md          # Tips macOS
    ├── linux.instructions.md          # Tips Linux
    ├── webrtc.instructions.md         # Tips WebRTC/streaming
    └── general.instructions.md        # Tips gerais
```

### Regras de Carregamento

Os módulos usam a extensão `.instructions.md` e têm cabeçalhos `applyTo` para carregamento automático baseado em glob patterns. Além disso:

1. **Sempre carregados** (via este arquivo): contexto mínimo do projeto, mapa de módulos
2. **Por tipo de tarefa**: carregar o role adequado (architect, implementer, debugger, reviewer)
3. **Por plataforma**: carregar automaticamente ao editar arquivos da plataforma correspondente
4. **Golden tips**: consultar ANTES de iniciar qualquer debug ou investigação técnica
5. **Design rules**: consultar ao criar novos módulos ou APIs

---

## 🔄 Protocolo de Golden Tips

> Golden Tips são lições aprendidas durante debugging e resolução de problemas que DEVEM ser documentadas automaticamente.

### Quando registrar um Golden Tip

Registre SEMPRE que:
- Um bug levou **mais de 2 tentativas** para ser resolvido
- A causa raiz era **diferente da hipótese inicial**
- Descobriu uma **particularidade de API** não óbvia na documentação
- Encontrou um **workaround** para limitação de plataforma
- Identificou uma **sequência de diagnóstico** eficaz

### Como registrar

Adicionar ao arquivo golden-tips correspondente (`macos.instructions.md`, `linux.instructions.md`, etc.) no formato:

```markdown
### GT-XXXX: [Título curto e descritivo]
- **Data:** YYYY-MM-DD
- **Contexto:** [Qual tarefa/feature/bug]
- **Sintoma:** [O que estava acontecendo]
- **Causa raiz:** [Por que acontecia]
- **Solução:** [O que resolveu]
- **Pista-chave:** [O que deveria ter sido checado primeiro]
- **Tags:** #componente #api #tipo-problema
```

### Como consultar

Antes de iniciar um debug:
1. Identificar o componente afetado
2. Ler as golden tips do arquivo correspondente
3. Verificar se há tips com tags relacionadas ao problema
4. Usar as "pistas-chave" como primeiro checklist de diagnóstico

---

## ⚡ Regras Globais (Mínimas)

### Linguagens
- **macOS:** Swift 5.9+, SwiftUI, async/await
- **Linux:** Rust 2021 edition, tokio, modular crates
- **Protocolo:** Protocol Buffers

### Commits
- Conventional commits: `feat:`, `fix:`, `chore:`, `docs:`, `perf:`, `test:`
- Mensagens em inglês
- Branch naming: `feature/<fase>-<descricao>`, `fix/<descricao>`, `research/<descricao>`

### Qualidade
- Nunca silenciar erros — tratar ou propagar explicitamente
- Documentar APIs públicas
- Testes para toda lógica não-trivial
- Benchmark antes de otimizar

### Eficiência de Tokens
- Não repetir contexto que já está em módulos carregados
- Respostas diretas e concisas
- Código completo (não parcial) em edits
- Se precisar de contexto, ler o módulo específico em vez de adivinhar

---

## 🗺️ Mapa Rápido do Projeto

| Diretório | Conteúdo | Linguagem |
|-----------|----------|-----------|
| `mac-client/` | App sender macOS | Swift |
| `linux-receiver/` | App receiver Linux | Rust |
| `shared-protocol/` | Definições de protocolo | Protobuf |
| `docs/` | Documentação técnica | Markdown |
| `infra/` | CI/CD, Docker, scripts | YAML/Shell |
| `.github/instructions/` | Instruções modulares Copilot | Markdown |
| `.github/instructions/golden-tips/` | Lições aprendidas | Markdown |
