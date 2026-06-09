//! Pure-Rust animated-GIF encoding — the zero-dependency fallback when no gifski binary
//! is available.
//!
//! Quality is a notch below gifski (per-frame NeuQuant palettes via the `gif` crate, no
//! cross-frame temporal dithering), but it works on any machine with nothing to install.
//! `gif` is MIT/Apache, so unlike gifski it is safely *linked*. See `docs/06-Export.md`.

use crate::error::EncodeError;
use crate::image::RgbaImage;
use crate::settings::GifSettings;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

/// Box-filter downscale to `target_w`, preserving aspect ratio. Never upscales: returns a
/// clone when `target_w` is zero or already ≥ the source width.
#[must_use]
pub fn downscale_rgba(src: &RgbaImage, target_w: u32) -> RgbaImage {
    if target_w == 0 || target_w >= src.width || src.width == 0 || src.height == 0 {
        return src.clone();
    }
    let (sw, sh) = (src.width as usize, src.height as usize);
    let dw = target_w as usize;
    let dh = ((u64::from(src.height) * u64::from(target_w)) / u64::from(src.width)).max(1) as usize;
    let mut out = vec![0u8; dw * dh * 4];

    for dy in 0..dh {
        let sy0 = dy * sh / dh;
        let sy1 = (((dy + 1) * sh / dh).max(sy0 + 1)).min(sh);
        for dx in 0..dw {
            let sx0 = dx * sw / dw;
            let sx1 = (((dx + 1) * sw / dw).max(sx0 + 1)).min(sw);
            let (mut r, mut g, mut b, mut a, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
            for sy in sy0..sy1 {
                let row = sy * sw;
                for sx in sx0..sx1 {
                    let i = (row + sx) * 4;
                    r += u32::from(src.pixels[i]);
                    g += u32::from(src.pixels[i + 1]);
                    b += u32::from(src.pixels[i + 2]);
                    a += u32::from(src.pixels[i + 3]);
                    n += 1;
                }
            }
            let n = n.max(1);
            let o = (dy * dw + dx) * 4;
            out[o] = (r / n) as u8;
            out[o + 1] = (g / n) as u8;
            out[o + 2] = (b / n) as u8;
            out[o + 3] = (a / n) as u8;
        }
    }
    RgbaImage::new(dw as u32, dh as u32, out)
}

/// Map a gifski-style `quality` (0–100) to a `gif` encoder `speed` (1 = best/slowest,
/// 30 = worst/fastest).
#[must_use]
pub fn quality_to_speed(quality: u8) -> i32 {
    let q = i32::from(quality.min(100));
    (30 - (q * 29 / 100)).clamp(1, 30)
}

/// Per-frame GIF delay in centiseconds for `fps` (clamped to ≥1 fps, ≥2cs).
#[must_use]
pub fn frame_delay_cs(fps: u32) -> u16 {
    let fps = fps.max(1);
    (((100 + fps / 2) / fps) as u16).max(2)
}

/// Encode RGBA `frames` into an animated GIF at `out` using the pure-Rust `gif` encoder.
///
/// Honors `settings.fps` (frame delay), `settings.width` (downscale cap), and
/// `settings.quality` (quantization speed). Loops infinitely.
///
/// # Errors
/// Returns [`EncodeError`] if there are no frames or the file cannot be written/encoded.
pub fn export_gif_native(
    frames: &[RgbaImage],
    settings: &GifSettings,
    out: &Path,
) -> Result<(), EncodeError> {
    if frames.is_empty() {
        return Err(EncodeError::NoFrames);
    }

    let scaled: Vec<RgbaImage> = match settings.width {
        Some(w) => frames.iter().map(|f| downscale_rgba(f, w)).collect(),
        None => frames.to_vec(),
    };
    let (w, h) = (scaled[0].width as u16, scaled[0].height as u16);

    let file = File::create(out)?;
    let mut encoder = gif::Encoder::new(BufWriter::new(file), w, h, &[])
        .map_err(|e| EncodeError::Gif(e.to_string()))?;
    encoder
        .set_repeat(gif::Repeat::Infinite)
        .map_err(|e| EncodeError::Gif(e.to_string()))?;

    let delay = frame_delay_cs(settings.fps);
    let speed = quality_to_speed(settings.quality);
    for img in &scaled {
        let mut buf = img.pixels.clone();
        let mut frame =
            gif::Frame::from_rgba_speed(img.width as u16, img.height as u16, &mut buf, speed);
        frame.delay = delay;
        encoder
            .write_frame(&frame)
            .map_err(|e| EncodeError::Gif(e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> RgbaImage {
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            px.extend_from_slice(&rgba);
        }
        RgbaImage::new(w, h, px)
    }

    #[test]
    fn downscale_halves_dimensions_and_preserves_color() {
        let src = solid(4, 4, [10, 20, 30, 255]);
        let out = downscale_rgba(&src, 2);
        assert_eq!((out.width, out.height), (2, 2));
        assert!(out.is_valid());
        assert_eq!(&out.pixels[0..4], &[10, 20, 30, 255]);
    }

    #[test]
    fn downscale_never_upscales() {
        let src = solid(2, 2, [0, 0, 0, 255]);
        let out = downscale_rgba(&src, 8);
        assert_eq!((out.width, out.height), (2, 2));
    }

    #[test]
    fn quality_maps_to_valid_speed_range() {
        assert_eq!(quality_to_speed(100), 1);
        assert!((1..=30).contains(&quality_to_speed(80)));
        assert_eq!(quality_to_speed(0), 30);
    }

    #[test]
    fn delay_is_centiseconds_per_frame() {
        assert_eq!(frame_delay_cs(20), 5); // 100/20
        assert_eq!(frame_delay_cs(15), 7); // round(6.67)
        assert!(frame_delay_cs(1000) >= 2); // floor clamp
    }

    #[test]
    fn no_frames_is_an_error() {
        let err = export_gif_native(&[], &GifSettings::readme(), Path::new("x.gif"));
        assert!(matches!(err, Err(EncodeError::NoFrames)));
    }

    #[test]
    fn writes_a_real_gif_file() {
        let frames = vec![solid(8, 8, [255, 0, 0, 255]), solid(8, 8, [0, 255, 0, 255])];
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("o.gif");
        export_gif_native(&frames, &GifSettings::readme(), &out).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(&bytes[0..6], b"GIF89a");
    }
}
