//! GIF and animated WebP export.
//!
//! Composited RGBA frames are downscaled and frame-selected to the target fps, then encoded
//! by either the pure-Rust `native` GIF encoder (`gif` + `color_quant`) or Google `libwebp`
//! (`webp`) into compact animated WebP files, streamed frame-by-frame to bound memory. This
//! keeps Vuoom's own code Apache-2.0 (nothing AGPL is linked). Size is estimated by
//! sample-and-extrapolate. See `docs/06-Export.md`.

mod error;
mod image;
mod native;
mod plan;
mod settings;
mod webp;

pub use error::EncodeError;
pub use image::{
    encode_png_fast_to_vec, encode_png_to_vec, read_png, swizzle_rb, write_png, RgbaImage,
};
pub use native::{
    downscale_rgba, export_gif_native, export_gif_native_streaming, frame_delay_cs,
    quality_to_speed,
};
pub use plan::{estimate_delta_total_bytes, estimate_total_bytes, plan_frames, EmittedFrame};
pub use settings::{GifSettings, WebpSettings};
pub use webp::{encode_webp_to_vec, export_webp, export_webp_streaming};
