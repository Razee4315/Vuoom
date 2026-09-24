//! The re-drawn cursor's look.

use serde::{Deserialize, Serialize};

fn default_size() -> f32 {
    CursorStyle::DEFAULT_SIZE
}

fn default_smoothing() -> f32 {
    CursorStyle::DEFAULT_SMOOTHING
}

/// A clean pointer drawn from the input log in place of the recorded one. Meant for takes
/// recorded with the pointer hidden (see [`crate::Project::pointer_captured`]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CursorStyle {
    /// Size multiplier (1.0 = about the size of a real pointer on the recorded screen).
    #[serde(default = "default_size")]
    pub size: f32,
    /// Path smoothing: the standard deviation, in seconds, of the centered blur applied
    /// to the pointer's path (0 = the raw path).
    #[serde(default = "default_smoothing")]
    pub smoothing: f32,
    /// Fade the pointer out while it rests, and back in just before it moves.
    #[serde(default)]
    pub hide_idle: bool,
}

impl CursorStyle {
    pub const DEFAULT_SIZE: f32 = 1.5;
    pub const DEFAULT_SMOOTHING: f32 = 0.05;
    pub const MIN_SIZE: f32 = 0.5;
    pub const MAX_SIZE: f32 = 3.0;
    pub const MAX_SMOOTHING: f32 = 0.2;

    /// The same style with every value in range.
    #[must_use]
    pub fn clamped(self) -> Self {
        Self {
            size: self.size.clamp(Self::MIN_SIZE, Self::MAX_SIZE),
            smoothing: self.smoothing.clamp(0.0, Self::MAX_SMOOTHING),
            hide_idle: self.hide_idle,
        }
    }
}

impl Default for CursorStyle {
    fn default() -> Self {
        Self {
            size: Self::DEFAULT_SIZE,
            smoothing: Self::DEFAULT_SMOOTHING,
            hide_idle: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_take_defaults_and_values_clamp() {
        let s: CursorStyle = serde_json::from_str("{}").unwrap();
        assert_eq!(s, CursorStyle::default());
        let wild = CursorStyle {
            size: 9.0,
            smoothing: -1.0,
            hide_idle: true,
        }
        .clamped();
        assert!((wild.size - CursorStyle::MAX_SIZE).abs() < f32::EPSILON);
        assert!(wild.smoothing.abs() < f32::EPSILON);
        assert!(wild.hide_idle);
    }
}
