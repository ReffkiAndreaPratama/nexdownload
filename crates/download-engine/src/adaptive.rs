use std::sync::Mutex as StdMutex;
use std::time::{Duration, Instant};

/// Token bucket used to cap engine throughput. A one-second burst is allowed.
pub struct BandwidthLimiter {
    rate: u64,
    state: StdMutex<BucketState>,
}

struct BucketState {
    tokens: f64,
    last: Instant,
}

impl BandwidthLimiter {
    pub fn new(bytes_per_second: u64) -> Self {
        let rate = bytes_per_second.max(1);
        Self {
            rate,
            state: StdMutex::new(BucketState {
                tokens: rate as f64,
                last: Instant::now(),
            }),
        }
    }

    pub async fn acquire(&self, bytes: u64) {
        let bytes = bytes.min(self.rate) as f64;
        loop {
            let wait = {
                let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                let now = Instant::now();
                let elapsed = now.duration_since(state.last).as_secs_f64();
                state.last = now;
                state.tokens = (state.tokens + elapsed * self.rate as f64).min(self.rate as f64);
                if state.tokens >= bytes {
                    state.tokens -= bytes;
                    return;
                }
                let missing = bytes - state.tokens;
                Duration::from_secs_f64((missing / self.rate as f64).clamp(0.001, 0.25))
            };
            tokio::time::sleep(wait).await;
        }
    }
}

/// Dynamically sized concurrency gate.
///
/// The adaptive controller can shrink or grow the limit at runtime. In-flight
/// transfers are never interrupted; the new limit applies to the next range a
/// worker picks up.
pub struct AdaptiveGate {
    inner: tokio::sync::Mutex<GateState>,
    notify: tokio::sync::Notify,
}

struct GateState {
    active: u32,
    limit: u32,
}

impl AdaptiveGate {
    pub fn new(limit: u32) -> Self {
        Self {
            inner: tokio::sync::Mutex::new(GateState {
                active: 0,
                limit: limit.max(1),
            }),
            notify: tokio::sync::Notify::new(),
        }
    }

    pub async fn acquire(&self) {
        loop {
            {
                let mut state = self.inner.lock().await;
                if state.active < state.limit {
                    state.active += 1;
                    return;
                }
            }
            self.notify.notified().await;
        }
    }

    pub async fn release(&self) {
        {
            let mut state = self.inner.lock().await;
            state.active = state.active.saturating_sub(1);
        }
        self.notify.notify_waiters();
    }

    pub async fn set_limit(&self, limit: u32) {
        {
            let mut state = self.inner.lock().await;
            state.limit = limit.max(1);
        }
        self.notify.notify_waiters();
    }

    pub async fn active(&self) -> u32 {
        self.inner.lock().await.active
    }
}

/// Throughput/error observer that explains every automatic adjustment.
pub struct AdaptiveController {
    max_connections: u32,
    limit: u32,
    last_error_count: u64,
    best_speed: f64,
    last_change: Instant,
}

#[derive(Debug, Clone)]
pub struct Adaptation {
    pub connections: u32,
    pub reason: Option<String>,
}

impl AdaptiveController {
    pub fn new(max_connections: u32) -> Self {
        let max_connections = max_connections.clamp(1, 32);
        Self {
            max_connections,
            limit: max_connections,
            last_error_count: 0,
            best_speed: 0.0,
            last_change: Instant::now(),
        }
    }

    pub fn on_sample(&mut self, speed_bps: f64, error_count: u64) -> Adaptation {
        let now = Instant::now();
        let since_change = now.duration_since(self.last_change);
        let new_errors = error_count.saturating_sub(self.last_error_count);
        self.last_error_count = error_count;

        if new_errors > 0 && self.limit > 1 && since_change >= Duration::from_secs(3) {
            let previous = self.limit;
            self.limit -= 1;
            self.last_change = now;
            return Adaptation {
                connections: self.limit,
                reason: Some(format!(
                    "Reduced connections from {previous} to {} after {new_errors} transfer error(s).",
                    self.limit
                )),
            };
        }

        if speed_bps > self.best_speed {
            self.best_speed = speed_bps;
        }

        if speed_bps > 64.0 * 1024.0
            && self.limit < self.max_connections
            && since_change >= Duration::from_secs(5)
        {
            self.limit += 1;
            self.last_change = now;
            return Adaptation {
                connections: self.limit,
                reason: Some(format!(
                    "Increased connections to {} because throughput is stable.",
                    self.limit
                )),
            };
        }

        Adaptation {
            connections: self.limit,
            reason: None,
        }
    }

    pub fn limit(&self) -> u32 {
        self.limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_reduces_after_errors() {
        let mut controller = AdaptiveController::new(4);
        // Move the clock back so a reduction is permitted.
        controller.last_change = Instant::now() - Duration::from_secs(10);
        let adaptation = controller.on_sample(1024.0, 2);
        assert_eq!(adaptation.connections, 3);
        assert!(adaptation.reason.is_some());
    }

    #[test]
    fn controller_never_drops_below_one() {
        let mut controller = AdaptiveController::new(1);
        controller.last_change = Instant::now() - Duration::from_secs(10);
        let adaptation = controller.on_sample(0.0, 5);
        assert_eq!(adaptation.connections, 1);
    }

    #[tokio::test]
    async fn gate_respects_limit_changes() {
        let gate = AdaptiveGate::new(1);
        gate.acquire().await;
        gate.set_limit(2).await;
        gate.acquire().await;
        assert_eq!(gate.active().await, 2);
    }
}
