//! The webcam bubble: where it sits over the recording and how it looks.

use serde::{Deserialize, Serialize};

/// Which corner of the framed recording the bubble sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

/// The bubble's outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CameraShape {
    #[default]
    Circle,
    /// A square with rounded corners.
    Square,
    /// A 16:9 rectangle with rounded corners.
    Wide,
}

impl CameraShape {
    /// Width over height.
    #[must_use]
    pub fn aspect(self) -> f64 {
        match self {
            Self::Circle | Self::Square => 1.0,
            Self::Wide => 16.0 / 9.0,
        }
    }

    /// Corner radius as a fraction of the bubble's height (0.5 on a square = a circle).
    #[must_use]
    pub fn radius(self) -> f64 {
        match self {
            Self::Circle => 0.5,
            Self::Square => 0.18,
            Self::Wide => 0.1,
        }
    }
}

fn yes() -> bool {
    true
}

fn default_size() -> f32 {
    CameraOverlay::DEFAULT_SIZE
}

/// A recorded webcam shown as a bubble over the recording. A project has one when its take
/// recorded the camera (`camera.vcam` beside the frames).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraOverlay {
    /// Whether the bubble shows (hiding keeps the recording and the settings).
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub corner: Corner,
    /// Bubble height as a fraction of the output height.
    #[serde(default = "default_size")]
    pub size: f32,
    #[serde(default)]
    pub shape: CameraShape,
    /// Show the camera mirrored, the way people see themselves.
    #[serde(default = "yes")]
    pub mirror: bool,
    /// Source time, in seconds, of the camera track's time zero: 0 for a normal take, below
    /// zero for a recovered take whose timeline starts after its recording clock did.
    #[serde(default)]
    pub offset: f64,
}

impl CameraOverlay {
    pub const DEFAULT_SIZE: f32 = 0.28;
    pub const MIN_SIZE: f32 = 0.12;
    pub const MAX_SIZE: f32 = 0.6;
    /// Gap between the bubble and the edges of the framed recording, as a fraction of the
    /// output height.
    pub const MARGIN: f64 = 0.035;

    /// The same overlay with the size in range.
    #[must_use]
    pub fn clamped(self) -> Self {
        Self {
            size: self.size.clamp(Self::MIN_SIZE, Self::MAX_SIZE),
            ..self
        }
    }
}

impl Default for CameraOverlay {
    fn default() -> Self {
        Self {
            visible: true,
            corner: Corner::default(),
            size: Self::DEFAULT_SIZE,
            shape: CameraShape::default(),
            mirror: true,
            offset: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_take_defaults() {
        let o: CameraOverlay = serde_json::from_str("{}").unwrap();
        assert_eq!(o, CameraOverlay::default());
        let json = r#"{"corner":"top-left","shape":"wide","size":9}"#;
        let wide: CameraOverlay = serde_json::from_str(json).unwrap();
        assert_eq!(wide.corner, Corner::TopLeft);
        assert_eq!(wide.shape, CameraShape::Wide);
        let size = wide.clamped().size;
        assert!((size - CameraOverlay::MAX_SIZE).abs() < f32::EPSILON);
    }
}
