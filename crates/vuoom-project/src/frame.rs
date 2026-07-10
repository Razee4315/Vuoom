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

impl Background {
    /// The named backdrop presets offered by the editor's background picker (Screen
    /// Studio-style gradients behind a framed recording), in display order.
    ///
    /// Kept monochrome-friendly and tasteful; no purple (a Vuoom design ban). Colors are
    /// stored straight (the compositor mixes in the same space it writes), and the diagonal
    /// angle runs top-left → bottom-right for a soft studio falloff.
    pub const PRESET_NAMES: &'static [&'static str] = &[
        "graphite", "slate", "teal", "dusk", "paper", "midnight", "solid",
    ];

    /// Resolve a preset name to its [`Background`], or `None` for an unknown name.
    #[must_use]
    pub fn preset(name: &str) -> Option<Background> {
        // Diagonal by default (top-left light → bottom-right shadow) so the lighter stop
        // reads as a soft key light. See `background_fill` for the angle→direction mapping.
        const A: f64 = 45.0;
        let grad = |from: Color, to: Color| Background::Gradient {
            from,
            to,
            angle_deg: A,
        };
        Some(match name {
            // Dark neutral gray → near-black.
            "graphite" => grad(Color::rgb(0.16, 0.16, 0.17), Color::rgb(0.04, 0.04, 0.05)),
            // Cool blue-gray.
            "slate" => grad(Color::rgb(0.20, 0.24, 0.30), Color::rgb(0.07, 0.09, 0.12)),
            // Deep teal.
            "teal" => grad(Color::rgb(0.06, 0.20, 0.21), Color::rgb(0.02, 0.08, 0.09)),
            // Indigo-gray → charcoal (dusk, deliberately not purple-heavy).
            "dusk" => grad(Color::rgb(0.17, 0.19, 0.26), Color::rgb(0.06, 0.06, 0.10)),
            // Warm off-white paper.
            "paper" => grad(Color::rgb(0.96, 0.95, 0.92), Color::rgb(0.85, 0.83, 0.78)),
            // Near-black with a faint blue lift.
            "midnight" => grad(Color::rgb(0.06, 0.07, 0.10), Color::rgb(0.01, 0.01, 0.02)),
            // Flat neutral dark (no gradient).
            "solid" => Background::Solid(Color::rgb(0.09, 0.09, 0.10)),
            _ => return None,
        })
    }

    /// The preset name matching `self` exactly, if any — lets the UI round-trip the current
    /// backdrop back to a selected swatch.
    #[must_use]
    pub fn preset_name(&self) -> Option<&'static str> {
        Self::PRESET_NAMES
            .iter()
            .copied()
            .find(|n| Self::preset(n).as_ref() == Some(self))
    }
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
        // No framing by default: the export is exactly the recording, edge to edge.
        // (The border/padding feature was removed from the product — a non-zero default
        // here would silently bake a border into every export.)
        Self {
            background: Background::Solid(Color::BLACK),
            padding: 0.0,
            corner_radius: 0.0,
            shadow: Shadow {
                strength: 0.0,
                ..Shadow::default()
            },
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
