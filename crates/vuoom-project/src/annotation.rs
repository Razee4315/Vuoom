//! The simple, core annotation set: text labels, arrows, and highlight boxes.
//!
//! Kept deliberately minimal (see `docs/11-Editor-and-Annotations.md`). Each carries a
//! [`TimeRange`] so it appears/disappears (with fades) on the timeline, and geometry in
//! normalized output space so it is resolution-independent.

use crate::color::{Color, Rect};
use crate::timing::TimeRange;
use glam::DVec2;
use serde::{Deserialize, Serialize};

/// A text label drawn on the canvas (rendered via glyphon at composite time).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextAnnotation {
    pub id: u32,
    pub text: String,
    /// Top-left anchor, normalized.
    pub pos: DVec2,
    /// Font size as a fraction of output height (e.g. 0.05 = 5% of height).
    pub font_size: f32,
    pub color: Color,
    /// Render with a bold weight (defaults off so older projects keep their look).
    #[serde(default)]
    pub bold: bool,
    /// Render italic.
    #[serde(default)]
    pub italic: bool,
    /// Draw a translucent plate behind the glyphs so captions stay legible over busy
    /// footage. Defaults off so older projects keep their look.
    #[serde(default)]
    pub background: bool,
    /// Font family name (e.g. "Anton"); empty = the default sans-serif. Defaults empty so
    /// older projects keep the sans look.
    #[serde(default)]
    pub font: String,
    pub range: TimeRange,
}

/// How an arrow's ends are capped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ArrowStyle {
    /// A single head at `to` (the classic pointer).
    #[default]
    Arrow,
    /// No heads — a plain line.
    Line,
    /// Heads at both ends.
    DoubleArrow,
}

/// An arrow from `from` to `to` (rendered via lyon at composite time).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ArrowAnnotation {
    pub id: u32,
    pub from: DVec2,
    pub to: DVec2,
    pub color: Color,
    /// Stroke thickness as a fraction of output height.
    pub thickness: f32,
    /// Head style. Defaults to `Arrow` so projects saved before this existed still load.
    #[serde(default)]
    pub style: ArrowStyle,
    pub range: TimeRange,
}

/// The geometry a highlight draws within its rect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HighlightShape {
    /// Axis-aligned rectangle.
    #[default]
    Rect,
    /// Ellipse inscribed in the rect.
    Ellipse,
}

/// A highlight region (outlined or filled; rectangle or ellipse).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HighlightBox {
    pub id: u32,
    pub rect: Rect,
    pub color: Color,
    /// Outline thickness as a fraction of output height (ignored when `filled`).
    pub thickness: f32,
    pub filled: bool,
    /// Defaults to `Rect` so projects saved before ellipses existed still load.
    #[serde(default)]
    pub shape: HighlightShape,
    pub range: TimeRange,
}
