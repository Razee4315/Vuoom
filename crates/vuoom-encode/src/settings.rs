//! GIF export settings and the two headline presets.

use serde::{Deserialize, Serialize};

/// User-facing GIF export settings. Defaults to the README preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GifSettings {
    /// Output frames per second (GIFs look best at 15–24).
    pub fps: u32,
    /// Output width cap in pixels; height follows aspect. `None` = keep source width.
    pub width: Option<u32>,
    /// gifski quality, 1–100.
    pub quality: u8,
    /// Optional gifsicle lossy second-pass strength (1–200); `None` = skip the pass.
    pub lossy: Option<u8>,
}

impl GifSettings {
    /// Small + good — the default for README/Slack/Discord demo GIFs.
    #[must_use]
    pub fn readme() -> Self {
        Self {
            fps: 15,
            width: Some(1000),
            quality: 80,
            lossy: Some(80),
        }
    }

    /// Higher quality, larger file.
    #[must_use]
    pub fn high_quality() -> Self {
        Self {
            fps: 24,
            width: Some(1280),
            quality: 95,
            lossy: None,
        }
    }
}

impl Default for GifSettings {
    fn default() -> Self {
        Self::readme()
    }
}
