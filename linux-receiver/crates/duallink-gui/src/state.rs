use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

// ── Phase ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Starting,
    WaitingForClient,
    Connected { peer_name: String, peer_addr: String },
    Streaming  { peer_name: String, peer_addr: String },
    Error(String),
}

impl Default for Phase {
    fn default() -> Self {
        Self::Starting
    }
}

impl Phase {
    pub fn label(&self) -> &str {
        match self {
            Phase::Starting            => "Starting…",
            Phase::WaitingForClient    => "Waiting for client",
            Phase::Connected   { .. } => "Client connected",
            Phase::Streaming   { .. } => "Streaming",
            Phase::Error       ( _ )  => "Error",
        }
    }

    pub fn color(&self) -> egui::Color32 {
        match self {
            Phase::Starting          => egui::Color32::from_rgb(160, 160, 160),
            Phase::WaitingForClient  => egui::Color32::from_rgb(230, 185, 50),
            Phase::Connected   { .. } => egui::Color32::from_rgb(50, 180, 230),
            Phase::Streaming   { .. } => egui::Color32::from_rgb(60, 200, 80),
            Phase::Error       ( _ )  => egui::Color32::from_rgb(220, 60, 60),
        }
    }

    /// Extract peer name if available.
    pub fn peer_name(&self) -> Option<&str> {
        match self {
            Phase::Connected { peer_name, .. } | Phase::Streaming { peer_name, .. } => {
                Some(peer_name.as_str())
            }
            _ => None,
        }
    }

    /// Extract peer address if available.
    pub fn peer_addr(&self) -> Option<&str> {
        match self {
            Phase::Connected { peer_addr, .. } | Phase::Streaming { peer_addr, .. } => {
                Some(peer_addr.as_str())
            }
            _ => None,
        }
    }
}

// ── GuiState ──────────────────────────────────────────────────────────────────

pub struct GuiState {
    pub phase:            Phase,
    pub pairing_pin:      String,
    pub tls_fingerprint:  String,
    pub fps:              f64,
    pub frames_received:  u64,
    pub frames_decoded:   u64,
    pub bitrate_mbps:     f64,
    pub transport:        String,
    pub logs:             VecDeque<String>,
    /// LAN IPv4 address shown in the PIN card so users know where to connect.
    pub lan_ip:           String,
    /// Whether mDNS advertising is active (set after `DualLinkAdvertiser::register` succeeds).
    pub mdns_active:      bool,
    /// Number of display streams bound (1 unless `DUALLINK_DISPLAY_COUNT` > 1).
    pub display_count:    u8,
    // Rolling-window helpers (private)
    last_frame_times:  VecDeque<Instant>,
    last_byte_amounts: VecDeque<(Instant, u64)>,
}

impl Default for GuiState {
    fn default() -> Self {
        Self {
            phase:           Phase::default(),
            pairing_pin:     String::new(),
            tls_fingerprint: String::new(),
            fps:             0.0,
            frames_received: 0,
            frames_decoded:  0,
            bitrate_mbps:    0.0,
            transport:       "detecting…".into(),
            logs:            VecDeque::new(),
            lan_ip:          String::new(),
            mdns_active:     false,
            display_count:   1,
            last_frame_times:  VecDeque::new(),
            last_byte_amounts: VecDeque::new(),
        }
    }
}

impl GuiState {
    /// Append a line to the circular log buffer (max 300 entries).
    pub fn push_log(&mut self, line: impl Into<String>) {
        let line = line.into();
        tracing::debug!("[GUI log] {}", line);
        if self.logs.len() >= 300 {
            self.logs.pop_front();
        }
        self.logs.push_back(line);
    }

    /// Call once per decoded frame to update FPS / bitrate rolling windows.
    pub fn tick_frame(&mut self, byte_count: usize) {
        let now = Instant::now();
        self.frames_decoded += 1;
        self.last_frame_times.push_back(now);
        self.last_byte_amounts.push_back((now, byte_count as u64));

        // Evict entries older than 1 second
        while self
            .last_frame_times
            .front()
            .map_or(false, |t| now.duration_since(*t).as_secs_f64() > 1.0)
        {
            self.last_frame_times.pop_front();
        }
        while self
            .last_byte_amounts
            .front()
            .map_or(false, |(t, _)| now.duration_since(*t).as_secs_f64() > 1.0)
        {
            self.last_byte_amounts.pop_front();
        }

        self.fps = self.last_frame_times.len() as f64;
        let bytes: u64 = self.last_byte_amounts.iter().map(|(_, b)| b).sum();
        self.bitrate_mbps = (bytes as f64 * 8.0) / 1_000_000.0;
    }

    /// Reset streaming counters / rolling windows (between sessions).
    pub fn reset_stats(&mut self) {
        self.fps             = 0.0;
        self.frames_received = 0;
        self.frames_decoded  = 0;
        self.bitrate_mbps    = 0.0;
        self.last_frame_times.clear();
        self.last_byte_amounts.clear();
    }
}

/// Shared handle passed between the GUI thread and the async receiver task.
pub type SharedState = Arc<Mutex<GuiState>>;
