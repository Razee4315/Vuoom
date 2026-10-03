//! The re-drawn cursor's look, and how clicks are shown.

use serde::{Deserialize, Serialize};

fn default_size() -> f32 {
    CursorStyle::DEFAULT_SIZE
}

fn default_smoothing() -> f32 {
    CursorStyle::DEFAULT_SMOOTHING
}

fn one() -> f32 {
    1.0
}

fn yes() -> bool {
    true
}

/// `v` in 0 to 1 (NaN reads as 0).
fn clamp01(v: f32) -> f32 {
    if v.is_nan() {
        0.0
    } else {
        v.clamp(0.0, 1.0)
    }
}

/// The shape of the pointer: what the system showed at that moment of the take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerShape {
    /// The I-beam over text.
    Text,
    /// The pointing hand over a link.
    Hand,
    Cross,
    /// Resize arrows: left-right, up-down, and the two diagonals.
    ResizeH,
    ResizeV,
    ResizeNwse,
    ResizeNesw,
    /// The four-way move arrow.
    Move,
    /// The arrow. Also what any shape this build doesn't know reads as (so it is listed
    /// last: serde's catch-all must be).
    #[default]
    #[serde(other)]
    Arrow,
}

/// The pointer took `shape` at source time `t` (seconds), until the next change.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PointerShapeAt {
    pub t: f64,
    pub shape: PointerShape,
}

/// The pointer's shape at source time `t`, from a take's changes in time order: the last
/// one at or before `t` (the arrow before the first).
#[must_use]
pub fn pointer_shape_at(changes: &[PointerShapeAt], t: f64) -> PointerShape {
    let after = changes.partition_point(|c| c.t <= t);
    match after.checked_sub(1) {
        Some(i) => changes[i].shape,
        None => PointerShape::Arrow,
    }
}

/// What the re-drawn pointer looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerLook {
    /// A round dot centered on the point, like a presenter's laser.
    Dot,
    /// The user's own picture ([`CursorStyle::image`]).
    Image,
    /// Vuoom's pointer, taking the shape the real one had at each moment (arrow, hand,
    /// text beam...). Also what a look this build doesn't know reads as (listed last:
    /// serde's catch-all must be).
    #[default]
    #[serde(other)]
    Classic,
}

/// A picture used as the pointer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointerImage {
    /// The picture's file (a PNG with transparency works best).
    pub path: String,
    /// The point that clicks, as a fraction of the picture's width and height (0, 0 = its
    /// top-left corner, where an arrow's tip usually is).
    #[serde(default)]
    pub hotspot: [f32; 2],
}

/// A clean pointer drawn from the input log in place of the recorded one. Meant for takes
/// recorded with the pointer hidden (see [`crate::Project::pointer_captured`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// The pointer's own color, RGB 0 to 1. `None` (what older projects load as) is the
    /// classic white pointer. The outline turns light on a dark color so it stays visible.
    #[serde(default)]
    pub color: Option<[f32; 3]>,
    /// Draw a soft highlight disc under the pointer, so viewers never lose it.
    #[serde(default)]
    pub halo: bool,
    /// Vuoom's pointer, a dot, or the user's picture.
    #[serde(default)]
    pub look: PointerLook,
    /// The picture for [`PointerLook::Image`], kept while another look is chosen so
    /// switching back finds it again.
    #[serde(default)]
    pub image: Option<PointerImage>,
    /// How opaque the pointer is, 0 (invisible) to 1.
    #[serde(default = "one")]
    pub opacity: f32,
    /// A soft shadow under the pointer (on in every project made before it could be
    /// turned off).
    #[serde(default = "yes")]
    pub shadow: bool,
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
        let image = self.image.map(|i| PointerImage {
            hotspot: i.hotspot.map(clamp01),
            path: i.path,
        });
        Self {
            size: self.size.clamp(Self::MIN_SIZE, Self::MAX_SIZE),
            smoothing: self.smoothing.clamp(0.0, Self::MAX_SMOOTHING),
            hide_idle: self.hide_idle,
            color: self.color.map(|c| c.map(clamp01)),
            halo: self.halo,
            look: self.look,
            image,
            opacity: clamp01(self.opacity),
            shadow: self.shadow,
        }
    }

    /// The look to draw: the picture only when there is one.
    #[must_use]
    pub fn drawn_look(&self) -> PointerLook {
        match (self.look, &self.image) {
            (PointerLook::Image, None) => PointerLook::Classic,
            (look, _) => look,
        }
    }

    /// The picture's file, when the pointer is drawn as one.
    #[must_use]
    pub fn image_path(&self) -> Option<&str> {
        let image = self.image.as_ref()?;
        (self.look == PointerLook::Image).then_some(image.path.as_str())
    }
}

impl Default for CursorStyle {
    fn default() -> Self {
        Self {
            size: Self::DEFAULT_SIZE,
            smoothing: Self::DEFAULT_SMOOTHING,
            hide_idle: false,
            color: None,
            halo: false,
            look: PointerLook::Classic,
            image: None,
            opacity: 1.0,
            shadow: true,
        }
    }
}

/// The animation played where the mouse clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClickEffect {
    /// Two rings, the second just behind the first.
    Rings,
    /// A filled disc that swells and fades.
    Pulse,
    /// Dots bursting outward.
    Burst,
    /// One expanding ring. Also what an effect this build doesn't know reads as (listed
    /// last: serde's catch-all must be).
    #[default]
    #[serde(other)]
    Ripple,
}

fn default_click_duration() -> f32 {
    ClickStyle::DEFAULT_DURATION
}

fn default_click_opacity() -> f32 {
    ClickStyle::DEFAULT_OPACITY
}

/// How clicks are shown when [`crate::Project::show_clicks`] is on.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ClickStyle {
    #[serde(default)]
    pub effect: ClickEffect,
    /// RGB 0 to 1; `None` is white.
    #[serde(default)]
    pub color: Option<[f32; 3]>,
    /// Size multiplier (1.0 = the classic ripple).
    #[serde(default = "one")]
    pub size: f32,
    /// How long one click's animation plays, in seconds.
    #[serde(default = "default_click_duration")]
    pub duration: f32,
    /// Opacity at the start of the animation, 0 to 1.
    #[serde(default = "default_click_opacity")]
    pub opacity: f32,
}

impl ClickStyle {
    pub const DEFAULT_DURATION: f32 = 0.45;
    pub const DEFAULT_OPACITY: f32 = 0.9;
    pub const MIN_SIZE: f32 = 0.5;
    pub const MAX_SIZE: f32 = 3.0;
    pub const MIN_DURATION: f32 = 0.2;
    pub const MAX_DURATION: f32 = 1.5;

    /// The same style with every value in range.
    #[must_use]
    pub fn clamped(self) -> Self {
        let size = if self.size.is_nan() { 1.0 } else { self.size };
        let duration = if self.duration.is_nan() {
            Self::DEFAULT_DURATION
        } else {
            self.duration
        };
        Self {
            effect: self.effect,
            color: self.color.map(|c| c.map(clamp01)),
            size: size.clamp(Self::MIN_SIZE, Self::MAX_SIZE),
            duration: duration.clamp(Self::MIN_DURATION, Self::MAX_DURATION),
            opacity: clamp01(self.opacity),
        }
    }
}

impl Default for ClickStyle {
    fn default() -> Self {
        Self {
            effect: ClickEffect::Ripple,
            color: None,
            size: 1.0,
            duration: Self::DEFAULT_DURATION,
            opacity: Self::DEFAULT_OPACITY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pointer_shape_holds_until_the_next_change() {
        let at = |t: f64, shape: PointerShape| PointerShapeAt { t, shape };
        let changes = [at(1.0, PointerShape::Text), at(2.5, PointerShape::Hand)];
        assert_eq!(pointer_shape_at(&changes, 0.5), PointerShape::Arrow);
        assert_eq!(pointer_shape_at(&changes, 1.0), PointerShape::Text);
        assert_eq!(pointer_shape_at(&changes, 2.4), PointerShape::Text);
        assert_eq!(pointer_shape_at(&changes, 9.0), PointerShape::Hand);
        assert_eq!(pointer_shape_at(&[], 1.0), PointerShape::Arrow);
        // Saved by name; a name this build doesn't know is an arrow, not an error.
        let json = serde_json::to_string(&PointerShape::ResizeNwse).unwrap();
        assert_eq!(json, "\"resize_nwse\"");
        let unknown: PointerShape = serde_json::from_str("\"sparkles\"").unwrap();
        assert_eq!(unknown, PointerShape::Arrow);
    }

    #[test]
    fn missing_fields_take_defaults_and_values_clamp() {
        let s: CursorStyle = serde_json::from_str("{}").unwrap();
        assert_eq!(s, CursorStyle::default());
        let wild = CursorStyle {
            size: 9.0,
            smoothing: -1.0,
            hide_idle: true,
            color: Some([2.0, 0.5, -1.0]),
            halo: true,
            look: PointerLook::Image,
            image: Some(PointerImage {
                path: "p.png".into(),
                hotspot: [1.5, -0.2],
            }),
            opacity: 4.0,
            shadow: false,
        }
        .clamped();
        assert!((wild.size - CursorStyle::MAX_SIZE).abs() < f32::EPSILON);
        assert!(wild.smoothing.abs() < f32::EPSILON);
        assert!(wild.hide_idle);
        assert_eq!(wild.color, Some([1.0, 0.5, 0.0]));
        assert!(wild.halo);
        assert!((wild.opacity - 1.0).abs() < f32::EPSILON);
        assert!(!wild.shadow);
        let hotspot = wild.image.as_ref().map(|i| i.hotspot);
        assert_eq!(hotspot, Some([1.0, 0.0]));
        assert_eq!(wild.image_path(), Some("p.png"));
    }

    #[test]
    fn older_pointers_keep_their_look() {
        // Saved before looks, opacity and the shadow switch: the classic pointer, opaque,
        // with its shadow.
        let json = r#"{"size":2.0,"halo":true}"#;
        let s: CursorStyle = serde_json::from_str(json).unwrap();
        assert_eq!(s.look, PointerLook::Classic);
        assert!((s.opacity - 1.0).abs() < f32::EPSILON);
        assert!(s.shadow);
        // A look this build doesn't know is the classic pointer.
        let s: CursorStyle = serde_json::from_str(r#"{"look":"sparkles"}"#).unwrap();
        assert_eq!(s.look, PointerLook::Classic);
    }

    #[test]
    fn a_picture_pointer_needs_a_picture() {
        let mut s = CursorStyle {
            look: PointerLook::Image,
            ..CursorStyle::default()
        };
        assert_eq!(s.drawn_look(), PointerLook::Classic);
        assert_eq!(s.image_path(), None);
        s.image = Some(PointerImage {
            path: "c.png".into(),
            hotspot: [0.5, 0.5],
        });
        assert_eq!(s.drawn_look(), PointerLook::Image);
        assert_eq!(s.image_path(), Some("c.png"));
        // Kept, but not drawn, under another look.
        s.look = PointerLook::Dot;
        assert_eq!(s.image_path(), None);
        assert_eq!(s.drawn_look(), PointerLook::Dot);
    }

    #[test]
    fn click_styles_default_to_the_classic_ripple_and_clamp() {
        let s: ClickStyle = serde_json::from_str("{}").unwrap();
        assert_eq!(s, ClickStyle::default());
        assert_eq!(s.effect, ClickEffect::Ripple);
        let wild = ClickStyle {
            effect: ClickEffect::Burst,
            color: Some([3.0, 0.2, -2.0]),
            size: 40.0,
            duration: 0.0,
            opacity: f32::NAN,
        }
        .clamped();
        assert_eq!(wild.effect, ClickEffect::Burst);
        assert_eq!(wild.color, Some([1.0, 0.2, 0.0]));
        assert!((wild.size - ClickStyle::MAX_SIZE).abs() < f32::EPSILON);
        let d = wild.duration - ClickStyle::MIN_DURATION;
        assert!(d.abs() < f32::EPSILON);
        assert!(wild.opacity.abs() < f32::EPSILON);
        let unknown: ClickEffect = serde_json::from_str("\"confetti\"").unwrap();
        assert_eq!(unknown, ClickEffect::Ripple);
    }
}
