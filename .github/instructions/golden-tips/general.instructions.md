---
applyTo: "**"
---

# Golden Tips — General

> Lições aprendidas cross-platform: protocolo, build system, CI/CD, tooling.
> Consultar ANTES de debugar problemas que não se encaixam em uma plataforma específica.

---

*Nenhuma golden tip registrada ainda. Tips serão adicionadas conforme problemas forem resolvidos durante o desenvolvimento.*

*Formato de referência: ver `.github/instructions/golden-tips/_index.instructions.md`*

---

**Total de tips:** 0
**Última atualização:** 2026-02-20
**Economia estimada:** 0 horas

---

### GT-0001: mDNS UDP multicast bloqueado por firewall
- **Data:** 2026-05-20
- **Contexto:** mDNS discovery não encontrava receivers na mesma rede
- **Sintoma:** `mdns-sd` browser ficava sem resultados; `NWBrowser` Swift idem
- **Causa raiz:** Firewall bloqueando porta 5353/UDP multicast
- **Solução:** `sudo ufw allow 5353/udp` (Linux) ou desabilitar firewall temporariamente para confirmar
- **Pista-chave:** Testar primeiro com `dns-sd -B _duallink._tcp` (macOS) ou `avahi-browse -a` (Linux) antes de debugar código
- **Tags:** #mdns #discovery #firewall #networking

---

### GT-0002: detect_local_ip() — UDP probe trick para IP LAN
- **Data:** 2026-05-20
- **Contexto:** Precisava do IP LAN real do receiver para anunciar no TXT record mDNS
- **Sintoma:** `127.0.0.1` ou IPs Docker sendo anunciados em vez do IP LAN real
- **Causa raiz:** Usar `hostname -I` ou `getifaddrs` retorna múltiplos IPs sem clara prioridade
- **Solução:** `UdpSocket::bind("0.0.0.0:0")` + `connect("8.8.8.8:80")` + `local_addr()` — sem envio de pacotes, retorna o IP primário de roteamento
- **Pista-chave:** O socket UDP sem `send()` não transmite nada; é apenas uma consulta de rota ao kernel
- **Tags:** #networking #lan-ip #udp #mdns

---

### GT-0003: Arc<Notify> vs oneshot channel para parar pipelines
- **Data:** 2026-05-22
- **Contexto:** SenderPipeline precisava de mecanismo de stop limpo
- **Sintoma:** `oneshot::Receiver` dropped prematuramente causava panic ao verificar se o receiver ainda estava ativo
- **Causa raiz:** `oneshot::Receiver` é consumido ao ser awaited; se clonado erroneamente, ou descartado fora de ordem, panica
- **Solução:** `Arc<Notify>` — `notify_one()` para parar, `notified()` para esperar; pode ser clonado livremente e usado em múltiplos tasks
- **Pista-chave:** Para N pipelines paralelos com shared stop, `Arc<Notify>` ou `CancellationToken` (tokio-util) são mais ergonômicos que oneshot
- **Tags:** #rust #async #tokio #pipeline #cancel

---

### GT-0004: Pairing PIN — receiver gera, sender recebe; não ao contrário
- **Data:** 2026-05-15
- **Contexto:** Implementação inicial tinha sender gerando o PIN
- **Sintoma:** UX confusa — o usuário não sabe onde digitar, pois não tem tela no sender
- **Causa raiz:** Design invertido: o PIN deve aparecer no receiver (que tem tela grande), não no sender
- **Solução:** Receiver gera 6 dígitos aleatórios, exibe no UI/terminal; sender digita no campo da UI
- **Pista-chave:** O TLS fingerprint no TXT mDNS permite ao sender verificar que está conectando ao receiver certo ANTES de digitar o PIN
- **Tags:** #security #pairing #ux #tls #pin

---

### GT-0005: DLNK frame header deve ter exatamente 18 bytes
- **Data:** 2026-05-18
- **Contexto:** Receiver começou a ter decode errors intermitentes após mudança no protocolo
- **Sintoma:** `h264parse` reportava NAL unit inválida em ~30% dos frames
- **Causa raiz:** Header tinha sido redimensionado de 16 para 18 bytes sem atualizar o offset de leitura dos NAL data no receiver
- **Solução:** Garantir que sender e receiver concordam em `HEADER_SIZE = 18`; versionar esta constante em `duallink-core`
- **Pista-chave:** Sempre que `HEADER_SIZE` mudar, incrementar `version` no `ClientHello` para detectar incompatibilidade
- **Tags:** #protocol #dlnk #binary #header #decode-error
