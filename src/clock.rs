//! The two wall clocks (`usertime`, `realtime`), shimmed for wasm32.
//!
//! `Instant::now()` and `SystemTime::now()` panic on
//! wasm32-unknown-unknown (no clock without an imported host
//! function). In the browser both clocks read 0 — a documented Stage
//! 16 deviation: timing loops still run, they just measure nothing.

#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct Clock(std::time::Instant);

#[cfg(not(target_arch = "wasm32"))]
impl Clock {
    pub fn start() -> Self {
        Clock(std::time::Instant::now())
    }

    /// Milliseconds since this interpreter started (usertime).
    pub fn elapsed_ms(&self) -> i64 {
        self.0.elapsed().as_millis() as i64
    }

    /// Milliseconds since the Unix epoch (realtime).
    pub fn wall_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) struct Clock;

#[cfg(target_arch = "wasm32")]
impl Clock {
    pub fn start() -> Self {
        Clock
    }

    pub fn elapsed_ms(&self) -> i64 {
        0
    }

    pub fn wall_ms() -> i64 {
        0
    }
}
