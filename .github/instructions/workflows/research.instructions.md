---
applyTo: "docs/**"
---

# Workflow: Research & PoC

> Ativar quando: investigando tecnologias, criando provas de conceito, validando viabilidade.

## Quando Usar

- Fase 0 do projeto (Research & Validação)
- Antes de implementar qualquer componente novo
- Quando encontrar um risco técnico não validado

## Processo de Pesquisa

### 1. Definir a Pergunta

Toda pesquisa deve começar com uma **pergunta clara e testável**:

```
❌ "Como funciona ScreenCaptureKit?"
✅ "É possível capturar frames de um CGVirtualDisplay via ScreenCaptureKit a 60fps com latência < 5ms?"
```

### 2. Pesquisa Documental (30 min max)

1. Documentação oficial da API
2. WWDC sessions / talks relevantes
3. Projetos open-source que usam a mesma API
4. Stack Overflow / forums com problemas relatados
5. Golden tips existentes no projeto

**Output:** Resumo de 1 parágrafo com links

### 3. PoC Mínima

Criar PoC isolada que responde **apenas** a pergunta definida:

```
poc/
├── poc-virtual-display/       # Uma PoC por pergunta
│   ├── README.md              # Pergunta, setup, resultado
│   └── ...código...
```

**Regras da PoC:**
- Mínimo código possível (< 200 linhas idealmente)
- Sem UI (apenas console output ou medições)
- Sem error handling elaborado (pode usar force unwrap / `.unwrap()`)
- Medir o que importa (latência, CPU, throughput)
- Documentar resultado no README

### 4. Documentar Resultado

Adicionar ao `docs/TECHNICAL_RESEARCH.md`:

```markdown
### Pesquisa: [Título]
- **Data:** YYYY-MM-DD
- **Pergunta:** [A pergunta testável]
- **Resultado:** ✅ Viável | ❌ Inviável | ⚠️ Viável com ressalvas
- **Evidência:** [Dados medidos, screenshots, logs]
- **Decisão:** [O que fazer com este resultado]
- **Código:** [Link para PoC se existir]
```

### 5. Go/No-Go Decision

Após pesquisa, decidir:
- **Go:** Viável, prosseguir com implementação
- **Pivot:** Inviável, usar alternativa documentada
- **More research:** Inconclusivo, definir próximo experimento

## Regras de Eficiência

- Máximo **4 horas** por pesquisa antes de decidir
- Se não funcionar em 4 horas, documentar blocker e considerar alternativa
- Não over-engineer a PoC — o objetivo é aprender, não implementar
- Preservar PoCs funcionais — podem ser reutilizadas como referência
