//! Recorded audio tracks attached to a project.

use serde::{Deserialize, Serialize};

/// Where a track was recorded from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioKind {
    /// A microphone (narration).
    Mic,
    /// Everything the computer played (app sounds, a video being demoed).
    System,
}

impl AudioKind {
    /// The WAV file name a track of this kind is stored under.
    #[must_use]
    pub fn file_name(self) -> &'static str {
        match self {
            Self::Mic => "mic.wav",
            Self::System => "system.wav",
        }
    }
}

fn unity() -> f32 {
    1.0
}

/// One recorded track. The audio itself lives in a WAV next to the frames (see
/// [`AudioKind::file_name`]); the project only holds how it plays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioTrack {
    pub kind: AudioKind,
    /// Source time, in seconds, of the track's first sample (0 for a normal take).
    #[serde(default)]
    pub offset: f64,
    /// Linear volume, `0.0..=MAX_GAIN` (1.0 = as recorded).
    #[serde(default = "unity")]
    pub gain: f32,
    #[serde(default)]
    pub muted: bool,
}

impl AudioTrack {
    /// Loudest allowed volume (+12 dB).
    pub const MAX_GAIN: f32 = 4.0;

    #[must_use]
    pub fn new(kind: AudioKind) -> Self {
        Self {
            kind,
            offset: 0.0,
            gain: 1.0,
            muted: false,
        }
    }

    /// The gain actually applied: zero when muted.
    #[must_use]
    pub fn effective_gain(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.gain.clamp(0.0, Self::MAX_GAIN)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_manifests_default_to_unity_gain() {
        let t: AudioTrack = serde_json::from_str(r#"{"kind":"mic"}"#).unwrap();
        assert_eq!(t, AudioTrack::new(AudioKind::Mic));
    }

    #[test]
    fn muting_and_clamping() {
        let mut t = AudioTrack::new(AudioKind::System);
        t.gain = 9.0;
        assert!((t.effective_gain() - AudioTrack::MAX_GAIN).abs() < f32::EPSILON);
        t.muted = true;
        assert!(t.effective_gain().abs() < f32::EPSILON);
        assert_eq!(AudioKind::System.file_name(), "system.wav");
    }
}
