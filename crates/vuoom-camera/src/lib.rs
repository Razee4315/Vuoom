//! Webcam for Vuoom recordings.
//!
//! - [`capture`]: Media Foundation capture on its own thread, timed by the same
//!   performance-counter clock as the screen frames.
//! - [`jpeg`]: JPEG encode and decode through the Windows Imaging Component.
//! - [`store`]: the timestamped frame track (`camera.vcam`) preview and export read from.
//!
//! See `docs/17-Camera.md`.

pub mod capture;
pub mod jpeg;
pub mod store;

pub use capture::{list_cameras, Camera, CameraRecorder, Clock};
pub use store::{TrackReader, TrackWriter};

/// The camera track's file name inside a take.
pub const FILE_NAME: &str = "camera.vcam";
