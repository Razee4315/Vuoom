//! The `.vuoom` project model — the single, serializable source of truth for every
//! non-destructive edit.
//!
//! Rendering at any time `t` (scrubbing AND deterministic GIF export) reads this model;
//! nothing here touches the GPU or OS, so it is fully unit-testable. See
//! `docs/02-Architecture.md` and `docs/11-Editor-and-Annotations.md`.

mod annotation;
mod color;
mod frame;
mod timeline;
mod timing;

pub use annotation::{ArrowAnnotation, ArrowStyle, HighlightBox, HighlightShape, TextAnnotation};
pub use color::{Color, Rect};
pub use frame::{AspectRatio, Background, FrameStyle, Shadow};
pub use timeline::{output_duration, output_to_source};
pub use timing::TimeRange;

// Re-export the zoom types so a Project is self-describing from one crate.
pub use vuoom_zoom::{InputEvent, ZoomConfig, ZoomKeyframe, ZoomStyle};

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

/// A labeled key press (e.g. `"Ctrl+Shift+P"`) for the optional keystroke overlay.
/// Only shortcuts and special keys are recorded — never plain typed text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyTap {
    /// Press time in seconds from the start of the recording.
    pub t: f64,
    pub label: String,
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
    /// The normalized input log, persisted so a reopened project can re-simulate the
    /// camera — panning needs the cursor samples, not just the zoom keyframes.
    #[serde(default)]
    pub events: Vec<InputEvent>,
    pub texts: Vec<TextAnnotation>,
    pub arrows: Vec<ArrowAnnotation>,
    pub highlights: Vec<HighlightBox>,
    pub trim: Option<Trim>,
    pub speed_regions: Vec<SpeedRegion>,
    /// Source-time ranges removed from the output entirely (mistakes, dead ends).
    #[serde(default)]
    pub cuts: Vec<Trim>,
    /// Render an expanding ripple at every recorded mouse click (preview + export).
    #[serde(default)]
    pub show_clicks: bool,
    /// Shortcut/special key presses, captured at stop time for the keystroke overlay.
    #[serde(default)]
    pub key_taps: Vec<KeyTap>,
    /// Render the keystroke overlay (chips at the bottom of the frame).
    #[serde(default)]
    pub show_keys: bool,
    pub frame: FrameStyle,
    pub aspect: AspectRatio,
}

impl Project {
    /// Current manifest schema version (2 added the persisted input-event log).
    pub const SCHEMA: u32 = 2;

    /// A fresh project for a freshly captured recording, with sensible defaults.
    #[must_use]
    pub fn new(source: SourceInfo) -> Self {
        Self {
            schema: Self::SCHEMA,
            source,
            zoom_config: ZoomConfig::default(),
            zooms: Vec::new(),
            events: Vec::new(),
            texts: Vec::new(),
            arrows: Vec::new(),
            highlights: Vec::new(),
            trim: None,
            speed_regions: Vec::new(),
            cuts: Vec::new(),
            show_clicks: false,
            key_taps: Vec::new(),
            show_keys: false,
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

    /// Parse a `.vuoom` JSON manifest, migrating older schemas forward first.
    ///
    /// The document is read untyped, its `schema` is inspected (absent means the
    /// pre-schema-field v1 layout), [`migrate`] advances it one version at a time up
    /// to [`Self::SCHEMA`], and only then is it deserialized into a typed [`Project`].
    /// This gives every future schema bump a concrete place to transform old JSON
    /// instead of relying solely on `#[serde(default)]`.
    ///
    /// # Errors
    /// Returns a [`serde_json::Error`] if the JSON is malformed or mistyped, or if the
    /// manifest was written by a *newer* Vuoom (schema greater than [`Self::SCHEMA`]),
    /// which this build cannot safely interpret.
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        use serde::de::Error as _;

        let mut value: serde_json::Value = serde_json::from_str(s)?;

        // Bundles predating the `schema` field are v1 by definition.
        let schema = value
            .get("schema")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1) as u32;

        // A newer bundle may have renamed or retyped fields this build has never
        // seen; deserializing it would silently drop or mangle data, so refuse.
        if schema > Self::SCHEMA {
            return Err(serde_json::Error::custom(format!(
                "this project was saved by a newer Vuoom (schema {schema}, this build \
                 understands up to schema {}); please update Vuoom to open it",
                Self::SCHEMA
            )));
        }

        // Advance the raw document one version at a time until it matches SCHEMA.
        for from in schema..Self::SCHEMA {
            migrate(from, &mut value);
        }

        // Whatever the document claimed, the typed struct carries the current version.
        if let Some(obj) = value.as_object_mut() {
            obj.insert("schema".into(), serde_json::Value::from(Self::SCHEMA));
        }

        serde_json::from_value(value)
    }
}

/// Transform a raw `.vuoom` document in place from schema `from` to schema `from + 1`.
///
/// [`Project::from_json`] calls this sequentially (`from`, `from + 1`, …) until the
/// document reaches [`Project::SCHEMA`], so each arm only has to advance a single
/// version. This is the hook the next schema bump plugs into: when a field is renamed,
/// retyped, or split, add the matching arm here to rewrite the JSON so the typed
/// deserialization that follows still succeeds on old bundles.
///
/// # Migration steps
/// - `1 => 2`: schema 2 added the persisted `events` log alongside `cuts`,
///   `show_clicks`, `key_taps`, and `show_keys`. Every one of those is
///   `#[serde(default)]`, so a v1 document deserializes unchanged — this step is an
///   intentional no-op, kept to document the transition and anchor the loop above.
fn migrate(from: u32, value: &mut serde_json::Value) {
    // 1 -> 2: purely additive, all new fields covered by serde defaults — no
    // transform needed. When a step does need one, match on `from` here and
    // rewrite `value`; the typed deserialization in `from_json` remains the
    // final gate on validity.
    let _ = (from, value);
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
            bold: true,
            italic: false,
            background: false,
            font: String::new(),
            range: TimeRange::with_fade(1.0, 4.0, 0.3),
        });
        p.aspect = AspectRatio::Widescreen;

        let json = p.to_json().expect("serialize");
        let back = Project::from_json(&json).expect("deserialize");
        assert_eq!(p, back);
    }

    #[test]
    fn legacy_v1_bundle_migrates_and_round_trips() {
        // Simulate a schema-1 bundle: strip the `schema` tag (v1 predates the field)
        // and every field schema 2 introduced, leaving only the original v1 shape.
        let p = Project::new(sample_source());
        let mut v: serde_json::Value = serde_json::from_str(&p.to_json().unwrap()).unwrap();
        let obj = v.as_object_mut().unwrap();
        obj.remove("schema");
        obj.remove("events");
        obj.remove("cuts");
        obj.remove("show_clicks");
        obj.remove("key_taps");
        obj.remove("show_keys");
        let legacy = serde_json::to_string(&v).unwrap();

        let back = Project::from_json(&legacy).expect("v1 bundle should migrate + parse");
        // Migration stamps the current schema; serde defaults fill the new fields.
        assert_eq!(back.schema, Project::SCHEMA);
        assert!(back.events.is_empty());
        assert!(back.cuts.is_empty());
        assert!(!back.show_clicks);
        assert!(back.key_taps.is_empty());
        assert!(!back.show_keys);
        assert_eq!(back, p);
    }

    #[test]
    fn newer_schema_is_rejected() {
        let p = Project::new(sample_source());
        let mut v: serde_json::Value = serde_json::from_str(&p.to_json().unwrap()).unwrap();
        v.as_object_mut()
            .unwrap()
            .insert("schema".into(), serde_json::Value::from(99));
        let doc = serde_json::to_string(&v).unwrap();

        let err = Project::from_json(&doc).expect_err("a newer schema must be rejected");
        assert!(
            err.to_string().contains("newer Vuoom"),
            "unexpected error message: {err}"
        );
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
