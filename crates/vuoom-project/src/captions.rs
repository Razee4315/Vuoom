//! Captions: lines of spoken text timed to the recording, shown at the bottom (or top) of
//! the frame and exportable as a subtitle file. See `docs/18-Captions.md`.

use serde::{Deserialize, Serialize};

use crate::timing::TimeRange;

/// One caption cue: what was said and when (source time).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Caption {
    pub id: u32,
    pub text: String,
    pub range: TimeRange,
}

/// Where captions sit in the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptionPosition {
    #[default]
    Bottom,
    Top,
}

fn yes() -> bool {
    true
}

fn default_size() -> f32 {
    CaptionStyle::DEFAULT_SIZE
}

/// How captions look.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CaptionStyle {
    /// Whether captions show in the video (hiding keeps them, and the subtitle file).
    #[serde(default = "yes")]
    pub visible: bool,
    /// Text height as a fraction of the output height.
    #[serde(default = "default_size")]
    pub size: f32,
    #[serde(default)]
    pub position: CaptionPosition,
}

impl CaptionStyle {
    pub const DEFAULT_SIZE: f32 = 0.045;
    pub const MIN_SIZE: f32 = 0.025;
    pub const MAX_SIZE: f32 = 0.09;

    /// The same style with the size in range.
    #[must_use]
    pub fn clamped(self) -> Self {
        Self {
            size: self.size.clamp(Self::MIN_SIZE, Self::MAX_SIZE),
            ..self
        }
    }
}

impl Default for CaptionStyle {
    fn default() -> Self {
        Self {
            visible: true,
            size: Self::DEFAULT_SIZE,
            position: CaptionPosition::default(),
        }
    }
}

/// The caption on screen at source time `t`, if any (the later one where two overlap).
#[must_use]
pub fn caption_at(captions: &[Caption], t: f64) -> Option<&Caption> {
    captions.iter().rev().find(|c| c.range.contains(t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_take_defaults() {
        let s: CaptionStyle = serde_json::from_str("{}").unwrap();
        assert_eq!(s, CaptionStyle::default());
        let json = r#"{"position":"top","size":1}"#;
        let top: CaptionStyle = serde_json::from_str(json).unwrap();
        assert_eq!(top.position, CaptionPosition::Top);
        let size = top.clamped().size;
        assert!((size - CaptionStyle::MAX_SIZE).abs() < f32::EPSILON);
    }

    #[test]
    fn the_cue_on_screen_is_found() {
        let cue = |id: u32, start: f64, end: f64| Caption {
            id,
            text: format!("cue {id}"),
            range: TimeRange::new(start, end),
        };
        let caps = [cue(1, 0.0, 1.0), cue(2, 1.5, 3.0), cue(3, 2.5, 4.0)];
        assert_eq!(caption_at(&caps, 0.5).map(|c| c.id), Some(1));
        assert!(caption_at(&caps, 1.2).is_none());
        // Overlapping cues: the later one shows.
        assert_eq!(caption_at(&caps, 2.8).map(|c| c.id), Some(3));
        assert!(caption_at(&caps, 4.0).is_none());
    }
}
