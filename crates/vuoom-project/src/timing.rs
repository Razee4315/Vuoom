//! Annotation timing: when an element is visible, and its fade in/out.
//!
//! Drives the per-frame opacity that the compositor feeds into glyph/shape alpha — the
//! CPU-side animation model from `docs/11-Editor-and-Annotations.md`.

use serde::{Deserialize, Serialize};

/// A visible time window with optional symmetric-ish fades.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TimeRange {
    /// Appears at `start` (s).
    pub start: f64,
    /// Disappears at `end` (s).
    pub end: f64,
    /// Fade-in duration (s); 0 = hard cut on.
    pub fade_in: f64,
    /// Fade-out duration (s); 0 = hard cut off.
    pub fade_out: f64,
}

impl TimeRange {
    /// A window with no fades.
    #[must_use]
    pub fn new(start: f64, end: f64) -> Self {
        Self {
            start,
            end,
            fade_in: 0.0,
            fade_out: 0.0,
        }
    }

    /// A window with equal fade-in and fade-out.
    #[must_use]
    pub fn with_fade(start: f64, end: f64, fade: f64) -> Self {
        Self {
            start,
            end,
            fade_in: fade,
            fade_out: fade,
        }
    }

    /// Whether `t` is within the visible window.
    #[must_use]
    pub fn contains(&self, t: f64) -> bool {
        (self.start..self.end).contains(&t)
    }

    /// Opacity at time `t` in `0.0..=1.0`, accounting for fades.
    #[must_use]
    pub fn opacity_at(&self, t: f64) -> f64 {
        if !self.contains(t) {
            return 0.0;
        }
        let mut o = 1.0_f64;
        if self.fade_in > 0.0 && t < self.start + self.fade_in {
            o = o.min((t - self.start) / self.fade_in);
        }
        if self.fade_out > 0.0 && t > self.end - self.fade_out {
            o = o.min((self.end - t) / self.fade_out);
        }
        o.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_outside_window_is_zero() {
        let r = TimeRange::new(1.0, 3.0);
        assert_eq!(r.opacity_at(0.5), 0.0);
        assert_eq!(r.opacity_at(3.0), 0.0);
        assert_eq!(r.opacity_at(2.0), 1.0);
    }

    #[test]
    fn fades_ramp_linearly() {
        let r = TimeRange::with_fade(0.0, 2.0, 0.5);
        assert!((r.opacity_at(0.25) - 0.5).abs() < 1e-9); // mid fade-in
        assert!((r.opacity_at(1.0) - 1.0).abs() < 1e-9); // fully on
        assert!((r.opacity_at(1.75) - 0.5).abs() < 1e-9); // mid fade-out
    }
}
