//! GIF export (v1's only output format).
//!
//! Composited RGBA frames are downscaled and frame-selected to the target fps, then fed
//! to **gifski**, which ships as a separate bundled binary invoked out-of-process so
//! Vuoom's own code stays Apache-2.0 (gifski is AGPL — never linked). An optional
//! gifsicle pass shrinks further. Size is estimated by sample-and-extrapolate.
//! See `docs/06-Export.md` and `docs/10-Licensing.md`.

mod error;
mod export;
mod gifski;
mod image;
mod plan;
mod settings;

pub use error::EncodeError;
pub use export::export_gif;
pub use gifski::{build_gifsicle_args, build_gifski_args, run_gifsicle, run_gifski};
pub use image::{write_png, RgbaImage};
pub use plan::{estimate_total_bytes, plan_frames, EmittedFrame};
pub use settings::GifSettings;
