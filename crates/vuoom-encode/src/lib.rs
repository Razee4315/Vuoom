//! GIF export (v1's only output format).
//!
//! Composited RGBA frames are downscaled and frame-selected to the target fps, then encoded
//! by the pure-Rust `native` encoder (`gif` + `color_quant`): one global palette for the whole
//! clip, delta-encoded frames, streamed to disk to bound memory. This keeps Vuoom's own code
//! Apache-2.0 (nothing AGPL is linked). Size is estimated by sample-and-extrapolate.
//! See `docs/06-Export.md`.

mod error;
mod image;
mod native;
mod plan;
mod settings;

pub use error::EncodeError;
pub use image::{encode_png_to_vec, read_png, swizzle_rb, write_png, RgbaImage};
pub use native::{
    downscale_rgba, export_gif_native, export_gif_native_streaming, frame_delay_cs,
    quality_to_speed,
};
pub use plan::{estimate_delta_total_bytes, estimate_total_bytes, plan_frames, EmittedFrame};
pub use settings::GifSettings;
