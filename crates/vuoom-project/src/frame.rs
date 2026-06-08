//! "Make it look designed": background, padding, rounded corners, shadow, aspect ratio.
//!
//! These are the framing controls from the spec (§5.4) with strong defaults so a
//! recording looks intentional with zero configuration.

use crate::color::Color;
use glam::DVec2;
use serde::{Deserialize, Serialize};

/// The backdrop the recording sits on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Background {
    Solid(Color),
    Gradient {
        from: Color,
        to: Color,
        angle_deg: f64,
    },
    /// A wallpaper/image at `path`.
    Image {
        path: String,
    },
    /// A blurred copy of the recording itself, as its own backdrop.
    Blur {
        radius: f64,
    },
}

/// Drop shadow under the framed recording.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Shadow {
    /// Offset as a fraction of output height.
    pub offset: DVec2,
    /// Blur radius as a fraction of output height.
    pub blur: f64,
    pub color: Color,
    /// 0 = no shadow, 1 = full strength.
    pub strength: f64,
}

impl Default for Shadow {
    fn default() -> Self {
        Self {
            offset: DVec2::new(0.0, 0.012),
            blur: 0.03,
            color: Color::BLACK,
            strength: 0.35,
        }
    }
}

/// The full framing style.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameStyle {
    pub background: Background,
    /// Padding around the recording as a fraction of the smaller output dimension.
    pub padding: f64,
    /// Corner radius as a fraction of the smaller output dimension.
    pub corner_radius: f64,
    pub shadow: Shadow,
}

impl Default for FrameStyle {
    fn default() -> Self {
        // A tasteful indigo→violet gradient, soft padding, rounded corners, soft shadow.
        Self {
            background: Background::Gradient {
                from: Color::rgb(0.36, 0.40, 0.92),
                to: Color::rgb(0.55, 0.36, 0.86),
                angle_deg: 135.0,
            },
            padding: 0.06,
            corner_radius: 0.02,
            shadow: Shadow::default(),
        }
    }
}

/// Output aspect-ratio presets (spec §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AspectRatio {
    /// Keep the source aspect ratio.
    Original,
    /// 16:9 (YouTube / README).
    Widescreen,
    /// 9:16 (Shorts / Reels).
    Vertical,
    /// 1:1 (square social).
    Square,
}

impl AspectRatio {
    /// The width/height ratio, or `None` to keep the source ratio.
    #[must_use]
    pub fn ratio(self) -> Option<f64> {
        match self {
            AspectRatio::Original => None,
            AspectRatio::Widescreen => Some(16.0 / 9.0),
            AspectRatio::Vertical => Some(9.0 / 16.0),
            AspectRatio::Square => Some(1.0),
        }
    }

    /// Output dimensions (even numbers) for a given source size.
    #[must_use]
    pub fn output_dims(self, src_w: u32, src_h: u32) -> (u32, u32) {
        match self.ratio() {
            None => (make_even(src_w), make_even(src_h)),
            Some(r) => {
                // Anchor on the source height, derive width from the target ratio.
                let h = f64::from(src_h.max(2));
                let w = (h * r).round().max(2.0);
                (make_even(w as u32), make_even(h as u32))
            }
        }
    }
}

fn make_even(v: u32) -> u32 {
    let v = v.max(2);
    v & !1
}
