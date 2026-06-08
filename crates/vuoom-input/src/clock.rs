//! The master clock: `QueryPerformanceCounter`.
//!
//! One QPC axis is shared by input events and capture frames so auto-zoom lands on the
//! right frame (see `docs/02-Architecture.md`, `docs/04-Input-and-AutoZoom.md`). The
//! frequency is read once and cached.

/// A cached high-resolution performance counter.
#[derive(Debug, Clone, Copy)]
pub struct Clock {
    freq: i64,
}

impl Clock {
    /// Read and cache the performance-counter frequency.
    #[must_use]
    pub fn new() -> Self {
        Self { freq: query_freq() }
    }

    /// The counter frequency (ticks per second).
    #[must_use]
    pub fn freq(&self) -> i64 {
        self.freq
    }

    /// Current counter value (ticks).
    #[must_use]
    pub fn now(&self) -> i64 {
        query_counter()
    }

    /// Seconds between two counter values.
    #[must_use]
    pub fn seconds_between(&self, start: i64, end: i64) -> f64 {
        (end - start) as f64 / self.freq as f64
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(windows)]
fn query_freq() -> i64 {
    use windows::Win32::System::Performance::QueryPerformanceFrequency;
    let mut f = 0i64;
    // SAFETY: `f` is a valid, writable i64 for the duration of the call.
    unsafe {
        let _ = QueryPerformanceFrequency(&mut f);
    }
    if f == 0 {
        1
    } else {
        f
    }
}

#[cfg(windows)]
fn query_counter() -> i64 {
    use windows::Win32::System::Performance::QueryPerformanceCounter;
    let mut c = 0i64;
    // SAFETY: `c` is a valid, writable i64 for the duration of the call.
    unsafe {
        let _ = QueryPerformanceCounter(&mut c);
    }
    c
}

// Non-Windows fallbacks keep the crate building on CI doc/check for other targets.
#[cfg(not(windows))]
fn query_freq() -> i64 {
    1_000_000_000
}

#[cfg(not(windows))]
fn query_counter() -> i64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freq_is_positive() {
        assert!(Clock::new().freq() > 0);
    }

    #[test]
    fn seconds_between_uses_frequency() {
        let c = Clock { freq: 1000 };
        assert!((c.seconds_between(0, 500) - 0.5).abs() < 1e-9);
    }
}
