---
applyTo: "**/*.{swift,rs}"
---

# Design: Patterns do Projeto

> Design patterns específicos utilizados no DualLink.

## 1. Pipeline Pattern

O padrão central do projeto. Cada estágio é um componente independente conectado por channels.

### Rust

```rust
/// Cada estágio do pipeline implementa esta trait
pub trait PipelineStage {
    type Input;
    type Output;
    type Error;
    
    /// Processa um item e produz output
    async fn process(&mut self, input: Self::Input) -> Result<Self::Output, Self::Error>;
}

/// Conexão entre estágios via channels
pub struct Pipeline<A, B> {
    stage_a: A,
    stage_b: B,
    channel: mpsc::Channel<A::Output>,
}
```

### Swift

```swift
/// Protocolo para estágios do pipeline
protocol PipelineStage {
    associatedtype Input
    associatedtype Output
    
    func process(_ input: Input) async throws -> Output
}
```

## 2. Transport Abstraction

Interface comum para diferentes transportes (Wi-Fi, USB).

```rust
#[async_trait]
pub trait Transport: Send + Sync {
    /// Envia dados para o outro lado
    async fn send(&self, data: &[u8]) -> Result<(), TransportError>;
    
    /// Recebe dados do outro lado
    async fn recv(&self) -> Result<Vec<u8>, TransportError>;
    
    /// Informações sobre a conexão
    fn connection_info(&self) -> ConnectionInfo;
    
    /// Fecha a conexão
    async fn close(&self) -> Result<(), TransportError>;
}

pub struct ConnectionInfo {
    pub mode: ConnectionMode, // WiFi, USB
    pub latency_ms: u32,
    pub bandwidth_mbps: f64,
}
```

## 3. Configuration with Defaults

Toda configuração tem um default seguro e pode ser overridden.

```rust
pub struct StreamConfig {
    pub resolution: Resolution,
    pub target_fps: u32,
    pub max_bitrate_bps: u64,
    pub codec: VideoCodec,
    pub low_latency: bool,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            resolution: Resolution::FHD, // 1920x1080
            target_fps: 30,
            max_bitrate_bps: 8_000_000, // 8 Mbps
            codec: VideoCodec::H264,
            low_latency: true,
        }
    }
}
```

## 4. Event-Driven State Machine

Para gerenciar ciclo de vida de conexão:

```
         ┌──────────┐
         │ Idle     │
         └────┬─────┘
              │ discover()
         ┌────▼─────┐
         │Discovering│
         └────┬─────┘
              │ found(peer)
         ┌────▼─────┐
         │Connecting │──── timeout ──→ Idle
         └────┬─────┘
              │ connected()
         ┌────▼─────┐
         │Streaming  │──── error ───→ Reconnecting
         └────┬─────┘                     │
              │ stop()               auto-retry
         ┌────▼─────┐                     │
         │ Idle     │◄────────────────────┘
         └──────────┘         (max 3 retries)
```

```rust
pub enum ConnectionState {
    Idle,
    Discovering,
    Connecting { peer: PeerInfo, attempt: u32 },
    Streaming { session: SessionInfo },
    Reconnecting { peer: PeerInfo, attempt: u32 },
    Error { reason: String },
}

pub enum ConnectionEvent {
    StartDiscovery,
    PeerFound(PeerInfo),
    Connected(SessionInfo),
    StreamStarted,
    Disconnected(DisconnectReason),
    Error(ConnectionError),
    Stop,
}
```

## 5. Metrics Collector

Coletar métricas de performance em todos os estágios:

```rust
pub struct PipelineMetrics {
    pub capture_latency_us: AtomicU64,
    pub encode_latency_us: AtomicU64,
    pub transport_latency_us: AtomicU64,
    pub decode_latency_us: AtomicU64,
    pub render_latency_us: AtomicU64,
    pub frames_sent: AtomicU64,
    pub frames_dropped: AtomicU64,
    pub current_fps: AtomicU32,
    pub current_bitrate_bps: AtomicU64,
}
```

## Regras de Aplicação

- Usar Pipeline Pattern para fluxo de dados (vídeo, input)
- Usar Transport Abstraction ao adicionar novo modo de conexão  
- Usar Configuration with Defaults em todo módulo configurável
- Usar State Machine para lifecycle de conexão
- Usar Metrics Collector em cada estágio do pipeline
- **Não criar patterns novos** sem justificativa documentada
