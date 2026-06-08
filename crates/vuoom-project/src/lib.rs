//! The `.vuoom` project model: a serde manifest describing every non-destructive edit
//! (zoom keyframes, trims, speed regions, background/frame styling, text/annotations,
//! aspect ratio) plus a reference to the captured intermediate.
//!
//! Rendering at any time `t` — for scrubbing AND deterministic GIF export — reads this
//! model. See `docs/02-Architecture.md` and `docs/11-Editor-and-Annotations.md`.

// TODO(M3): ZoomKeyframe, TextAnnotation, ArrowAnnotation, HighlightBox, Trim, SpeedRegion,
// Background, FrameStyle, AspectRatio, and the top-level Project manifest.
