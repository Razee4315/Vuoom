//! The `.vuoom` project model — the single, serializable source of truth for every
//! non-destructive edit.
//!
//! Rendering at any time `t` (scrubbing AND deterministic GIF export) reads this model;
//! nothing here touches the GPU or OS, so it is fully unit-testable. See
//! `docs/02-Architecture.md` and `docs/11-Editor-and-Annotations.md`.

mod annotation;
mod color;
mod frame;
mod timing;

pub use annotation::{ArrowAnnotation, HighlightBox, TextAnnotation};
pub use color::{Color, Rect};
pub use frame::{AspectRatio, Background, FrameStyle, Shadow};
pub use timing::TimeRange;

// Re-export the zoom types so a Project is self-describing from one crate.
pub use vuoom_zoom::{ZoomConfig, ZoomKeyframe};

use serde::{Deserialize, Serialize};

/// Metadata about the captured intermediate the project edits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Path to the near-lossless captured intermediate.
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// Recording duration in seconds.
    pub duration: f64,
}

/// Trim the clip to `[start, end]` (seconds).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Trim {
    pub start: f64,
    pub end: f64,
}

/// Play `[start, end]` at `factor`× speed (e.g. 4.0 to skim dead time).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpeedRegion {
    pub start: f64,
    pub end: f64,
    pub factor: f64,
}

/// The whole editable project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// Manifest schema version (for forward migration).
    pub schema: u32,
    pub source: SourceInfo,
    /// The tunables used to plan zooms (so re-planning is reproducible).
    pub zoom_config: ZoomConfig,
    pub zooms: Vec<ZoomKeyframe>,
    pub texts: Vec<TextAnnotation>,
    pub arrows: Vec<ArrowAnnotation>,
    pub highlights: Vec<HighlightBox>,
    pub trim: Option<Trim>,
    pub speed_regions: Vec<SpeedRegion>,
    pub frame: FrameStyle,
    pub aspect: AspectRatio,
}

impl Project {
    /// Current manifest schema version.
    pub const SCHEMA: u32 = 1;

    /// A fresh project for a freshly captured recording, with sensible defaults.
    #[must_use]
    pub fn new(source: SourceInfo) -> Self {
        Self {
            schema: Self::SCHEMA,
            source,
            zoom_config: ZoomConfig::default(),
            zooms: Vec::new(),
            texts: Vec::new(),
            arrows: Vec::new(),
            highlights: Vec::new(),
            trim: None,
            speed_regions: Vec::new(),
            frame: FrameStyle::default(),
            aspect: AspectRatio::Original,
        }
    }

    /// Output dimensions for the chosen aspect ratio.
    #[must_use]
    pub fn output_dims(&self) -> (u32, u32) {
        self.aspect
            .output_dims(self.source.width, self.source.height)
    }

    /// The effective time window after trimming.
    #[must_use]
    pub fn active_range(&self) -> (f64, f64) {
        match self.trim {
            Some(t) => (t.start, t.end),
            None => (0.0, self.source.duration),
        }
    }

    /// Serialize to a pretty `.vuoom` JSON manifest.
    ///
    /// # Errors
    /// Returns a [`serde_json::Error`] if serialization fails.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Parse a `.vuoom` JSON manifest.
    ///
    /// # Errors
    /// Returns a [`serde_json::Error`] if the JSON is malformed or mistyped.
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_source() -> SourceInfo {
        SourceInfo {
            path: "capture.mkv".into(),
            width: 2560,
            height: 1440,
            fps: 60.0,
            duration: 12.0,
        }
    }

    #[test]
    fn new_project_has_defaults() {
        let p = Project::new(sample_source());
        assert_eq!(p.schema, Project::SCHEMA);
        assert!(p.zooms.is_empty());
        assert_eq!(p.aspect, AspectRatio::Original);
        assert_eq!(p.active_range(), (0.0, 12.0));
    }

    #[test]
    fn json_round_trip_is_lossless() {
        let mut p = Project::new(sample_source());
        p.texts.push(TextAnnotation {
            id: 1,
            text: "Hello, README!".into(),
            pos: glam::DVec2::new(0.1, 0.1),
            font_size: 0.05,
            color: Color::WHITE,
            range: TimeRange::with_fade(1.0, 4.0, 0.3),
        });
        p.aspect = AspectRatio::Widescreen;

        let json = p.to_json().expect("serialize");
        let back = Project::from_json(&json).expect("deserialize");
        assert_eq!(p, back);
    }

    #[test]
    fn widescreen_output_dims_are_even_and_16_9() {
        let p = Project {
            aspect: AspectRatio::Widescreen,
            ..Project::new(sample_source())
        };
        let (w, h) = p.output_dims();
        assert_eq!(w % 2, 0);
        assert_eq!(h % 2, 0);
        // 1440 * 16/9 = 2560
        assert_eq!((w, h), (2560, 1440));
    }
}
