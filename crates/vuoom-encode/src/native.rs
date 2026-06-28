//! Pure-Rust animated-GIF encoding — the zero-dependency fallback when no gifski binary
//! is available.
//!
//! Optimized for screen recordings: ONE global NeuQuant palette for the whole clip
//! (no per-frame palette flicker or overhead), every frame after the first is a
//! **delta rectangle** (only the changed region is stored; unchanged pixels inside it are
//! transparent, disposal `Keep`), and identical consecutive frames collapse into a longer
//! delay on the previous frame. On mostly-static product/UI clips this is typically
//! 5–10× smaller than full-frame encoding at identical visual quality.
//!
//! `gif`/`color_quant` are MIT/Apache, so unlike gifski they are safely *linked*.
//! See `docs/06-Export.md`.

use crate::error::EncodeError;
use crate::image::RgbaImage;
use crate::settings::GifSettings;
use color_quant::NeuQuant;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

/// Palette index reserved for "pixel unchanged from the previous frame".
const TRANSPARENT_INDEX: u8 = 255;
/// Real colors in the global palette; slot 255 is the transparent index.
const PALETTE_COLORS: usize = 255;
/// Pixel budget for training the global palette (~1 MB of RGBA samples, spread evenly
/// across the whole clip so late scenes get palette slots too).
const PALETTE_SAMPLE_PIXELS: usize = 1 << 18;

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

/// Map a gifski-style `quality` (0–100) to a NeuQuant sample factor (1 = best/slowest,
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

/// One global NeuQuant palette for the whole clip, with an exact 24-bit-RGB lookup cache
/// (composited frames are opaque, so alpha is ignored). Identical colors always map to
/// identical indices, which is what makes index-level frame diffing exact.
struct GlobalQuantizer {
    quant: NeuQuant,
    /// `1 << 24` entries keyed by RGB; -1 = unseen, else the palette index.
    cache: Vec<i16>,
}

impl GlobalQuantizer {
    /// Train on up to [`PALETTE_SAMPLE_PIXELS`] pixels sampled evenly across all frames.
    fn train(frames: &[RgbaImage], speed: i32) -> Self {
        let total_px: usize = frames.iter().map(|f| f.pixels.len() / 4).sum();
        let step = (total_px / PALETTE_SAMPLE_PIXELS).max(1);
        let mut samples = Vec::with_capacity((total_px / step + 1) * 4);
        let mut next = 0usize; // next global pixel index to sample
        let mut seen = 0usize;
        for f in frames {
            let n = f.pixels.len() / 4;
            while next < seen + n {
                let o = (next - seen) * 4;
                samples.extend_from_slice(&[f.pixels[o], f.pixels[o + 1], f.pixels[o + 2], 255]);
                next += step;
            }
            seen += n;
        }
        Self {
            quant: NeuQuant::new(speed.clamp(1, 30), PALETTE_COLORS, &samples),
            cache: vec![-1i16; 1 << 24],
        }
    }

    /// The 256-entry global palette as RGB triples (slot 255 is the transparent slot).
    fn palette(&self) -> Vec<u8> {
        let mut p = self.quant.color_map_rgb();
        p.resize(256 * 3, 0);
        p
    }

    fn index(&mut self, r: u8, g: u8, b: u8) -> u8 {
        let key = ((r as usize) << 16) | ((g as usize) << 8) | (b as usize);
        let cached = self.cache[key];
        if cached >= 0 {
            return cached as u8;
        }
        let i = self.quant.index_of(&[r, g, b, 255]).min(PALETTE_COLORS - 1);
        self.cache[key] = i as i16;
        i as u8
    }

    /// Quantize a frame to global-palette indices (one byte per pixel).
    fn index_frame(&mut self, img: &RgbaImage) -> Vec<u8> {
        let mut out = Vec::with_capacity(img.pixels.len() / 4);
        for px in img.pixels.chunks_exact(4) {
            out.push(self.index(px[0], px[1], px[2]));
        }
        out
    }
}

/// Bounding box `(x0, y0, x1, y1)` (x1/y1 exclusive) of pixels whose palette index
/// differs between two same-sized index buffers, or `None` if they are identical.
fn diff_bbox(prev: &[u8], cur: &[u8], w: usize, h: usize) -> Option<(usize, usize, usize, usize)> {
    let (mut x0, mut x1, mut y0, mut y1) = (w, 0usize, h, 0usize);
    for y in 0..h {
        let p = &prev[y * w..(y + 1) * w];
        let c = &cur[y * w..(y + 1) * w];
        if p == c {
            continue;
        }
        // The rows differ, so both positions exist.
        let first = p.iter().zip(c).position(|(a, b)| a != b).unwrap_or(0);
        let last = w - p
            .iter()
            .rev()
            .zip(c.iter().rev())
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        x0 = x0.min(first);
        x1 = x1.max(last);
        y0 = y0.min(y);
        y1 = y + 1;
    }
    (y1 > 0).then_some((x0, y0, x1, y1))
}

/// A frame waiting to be written — held back one step so duplicate successors can extend
/// its delay instead of being emitted.
struct PendingFrame {
    left: u16,
    top: u16,
    width: u16,
    height: u16,
    buffer: Vec<u8>,
    delay_cs: u32,
}

fn write_pending<W: std::io::Write>(
    enc: &mut gif::Encoder<W>,
    p: &PendingFrame,
) -> Result<(), EncodeError> {
    let frame = gif::Frame {
        left: p.left,
        top: p.top,
        width: p.width,
        height: p.height,
        delay: p.delay_cs.min(u32::from(u16::MAX)) as u16,
        dispose: gif::DisposalMethod::Keep,
        transparent: Some(TRANSPARENT_INDEX),
        buffer: std::borrow::Cow::Borrowed(&p.buffer),
        ..gif::Frame::default()
    };
    enc.write_frame(&frame)
        .map_err(|e| EncodeError::Gif(e.to_string()))
}

/// Encode RGBA `frames` into an animated GIF at `out` using the pure-Rust `gif` encoder
/// with a global palette and delta-rectangle frames.
///
/// Honors `settings.fps` (frame delay), `settings.width` (downscale cap), and
/// `settings.quality` (palette quantization effort). Loops infinitely.
///
/// # Errors
/// Returns [`EncodeError`] if there are no frames, the frames disagree on dimensions, or
/// the file cannot be written/encoded.
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
    let (w, h) = (scaled[0].width, scaled[0].height);
    if scaled.iter().any(|f| f.width != w || f.height != h) {
        return Err(EncodeError::Gif("frame dimensions differ".into()));
    }
    if w == 0 || h == 0 {
        // A zero-pixel frame yields an empty palette sample set, which would panic the
        // quantizer (`NeuQuant::new` on an empty slice divides by zero).
        return Err(EncodeError::Gif("zero-size frame".into()));
    }
    let (wu, hu) = (w as usize, h as usize);

    let mut quant = GlobalQuantizer::train(&scaled, quality_to_speed(settings.quality));
    let palette = quant.palette();

    let file = File::create(out)?;
    let mut encoder = gif::Encoder::new(BufWriter::new(file), w as u16, h as u16, &palette)
        .map_err(|e| EncodeError::Gif(e.to_string()))?;
    encoder
        .set_repeat(gif::Repeat::Infinite)
        .map_err(|e| EncodeError::Gif(e.to_string()))?;

    let delay = u32::from(frame_delay_cs(settings.fps));
    let mut prev = quant.index_frame(&scaled[0]);
    // First frame is a full keyframe (it uses no transparent pixels: real colors only
    // ever map to indices 0..=254).
    let mut pending = PendingFrame {
        left: 0,
        top: 0,
        width: w as u16,
        height: h as u16,
        buffer: prev.clone(),
        delay_cs: delay,
    };

    for img in &scaled[1..] {
        let cur = quant.index_frame(img);
        match diff_bbox(&prev, &cur, wu, hu) {
            // Identical frame: hold the pending frame on screen longer instead.
            None => pending.delay_cs += delay,
            Some((x0, y0, x1, y1)) => {
                write_pending(&mut encoder, &pending)?;
                let (bw, bh) = (x1 - x0, y1 - y0);
                let mut buf = Vec::with_capacity(bw * bh);
                for y in y0..y1 {
                    let row = y * wu;
                    for x in x0..x1 {
                        let i = row + x;
                        buf.push(if cur[i] == prev[i] {
                            TRANSPARENT_INDEX
                        } else {
                            cur[i]
                        });
                    }
                }
                pending = PendingFrame {
                    left: x0 as u16,
                    top: y0 as u16,
                    width: bw as u16,
                    height: bh as u16,
                    buffer: buf,
                    delay_cs: delay,
                };
                prev = cur;
            }
        }
    }
    write_pending(&mut encoder, &pending)?;
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

    /// Decode a GIF back into (has_global_palette, frames-with-geometry).
    fn decode(path: &Path) -> (bool, Vec<gif::Frame<'static>>) {
        let mut opts = gif::DecodeOptions::new();
        opts.set_color_output(gif::ColorOutput::Indexed);
        let mut d = opts.read_info(File::open(path).unwrap()).unwrap();
        let has_global = d.global_palette().is_some();
        let mut frames = Vec::new();
        while let Some(f) = d.read_next_frame().unwrap() {
            frames.push(f.clone());
        }
        (has_global, frames)
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
    fn zero_size_frame_is_an_error_not_a_panic() {
        // A 0x0 frame used to reach the quantizer with an empty sample set and panic.
        let frames = vec![solid(0, 0, [0, 0, 0, 255])];
        let out = std::env::temp_dir().join("vuoom-zero-frame-test.gif");
        let r = export_gif_native(&frames, &GifSettings::readme(), &out);
        assert!(matches!(r, Err(EncodeError::Gif(_))));
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
    fn mismatched_dimensions_are_an_error() {
        let frames = vec![solid(8, 8, [255, 0, 0, 255]), solid(4, 4, [255, 0, 0, 255])];
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("bad.gif");
        let err = export_gif_native(&frames, &GifSettings::readme(), &out);
        assert!(matches!(err, Err(EncodeError::Gif(_))));
    }

    #[test]
    fn writes_a_real_gif_with_a_global_palette() {
        let frames = vec![solid(8, 8, [255, 0, 0, 255]), solid(8, 8, [0, 255, 0, 255])];
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("o.gif");
        export_gif_native(&frames, &GifSettings::readme(), &out).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(&bytes[0..6], b"GIF89a");
        let (has_global, decoded) = decode(&out);
        assert!(has_global);
        assert_eq!(decoded.len(), 2);
    }

    #[test]
    fn duplicate_frames_collapse_into_a_longer_delay() {
        let f = solid(8, 8, [40, 80, 120, 255]);
        let frames = vec![f.clone(), f.clone(), f];
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("dup.gif");
        let settings = GifSettings::readme(); // 15 fps -> 7cs per frame
        export_gif_native(&frames, &settings, &out).unwrap();
        let (_, decoded) = decode(&out);
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].delay, 3 * frame_delay_cs(settings.fps));
    }

    #[test]
    fn delta_frame_covers_only_the_changed_region() {
        let base = solid(16, 16, [200, 0, 0, 255]);
        let mut changed = base.clone();
        for y in 8..12 {
            for x in 8..12 {
                let o = (y * 16 + x) * 4;
                changed.pixels[o..o + 4].copy_from_slice(&[0, 200, 0, 255]);
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("delta.gif");
        export_gif_native(&[base, changed], &GifSettings::readme(), &out).unwrap();
        let (_, decoded) = decode(&out);
        assert_eq!(decoded.len(), 2);
        // Keyframe is full-size at the origin.
        let key = &decoded[0];
        assert_eq!((key.left, key.top, key.width, key.height), (0, 0, 16, 16));
        assert!(!key.buffer.contains(&TRANSPARENT_INDEX));
        // Delta frame is exactly the 4x4 changed block.
        let delta = &decoded[1];
        assert_eq!(
            (delta.left, delta.top, delta.width, delta.height),
            (8, 8, 4, 4)
        );
        assert_eq!(delta.dispose, gif::DisposalMethod::Keep);
        assert!(!delta.buffer.contains(&TRANSPARENT_INDEX));
    }

    #[test]
    fn mostly_static_clip_is_far_smaller_than_full_frame_encoding() {
        // A product-demo-like clip: a noisy-but-static background with a small box
        // moving across it. The realistic upper bound for size is keyframe × frames
        // (what full-frame encoding produced); deltas must be far below it.
        let (w, h, n) = (320u32, 200u32, 30usize);
        let mut bg = Vec::with_capacity((w * h * 4) as usize);
        for i in 0..(w * h) {
            let v = ((i * 37) % 251) as u8; // deterministic texture so LZW can't trivially RLE it
            bg.extend_from_slice(&[v, v / 2, 255 - v, 255]);
        }
        let frames: Vec<RgbaImage> = (0..n)
            .map(|f| {
                let mut px = bg.clone();
                for y in 90..110usize {
                    for x in (f * 8)..(f * 8 + 20) {
                        let o = (y * w as usize + x) * 4;
                        px[o..o + 4].copy_from_slice(&[255, 255, 255, 255]);
                    }
                }
                RgbaImage::new(w, h, px)
            })
            .collect();

        let dir = tempfile::tempdir().unwrap();
        let settings = GifSettings {
            width: None,
            ..GifSettings::readme()
        };
        let full = dir.path().join("clip.gif");
        export_gif_native(&frames, &settings, &full).unwrap();
        let key = dir.path().join("key.gif");
        export_gif_native(&frames[0..1], &settings, &key).unwrap();

        let clip_bytes = std::fs::metadata(&full).unwrap().len();
        let key_bytes = std::fs::metadata(&key).unwrap().len();
        assert!(
            clip_bytes < key_bytes * n as u64 / 4,
            "delta encoding regressed: clip={clip_bytes}B vs full-frame≈{}B",
            key_bytes * n as u64
        );
    }

    #[test]
    fn unchanged_pixels_inside_the_delta_rect_are_transparent() {
        let base = solid(16, 16, [200, 0, 0, 255]);
        let mut changed = base.clone();
        // Two far-apart pixels -> a large bbox that is mostly unchanged inside.
        for &(x, y) in &[(2usize, 2usize), (13, 13)] {
            let o = (y * 16 + x) * 4;
            changed.pixels[o..o + 4].copy_from_slice(&[0, 200, 0, 255]);
        }
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("sparse.gif");
        export_gif_native(&[base, changed], &GifSettings::readme(), &out).unwrap();
        let (_, decoded) = decode(&out);
        assert_eq!(decoded.len(), 2);
        let delta = &decoded[1];
        assert_eq!(
            (delta.left, delta.top, delta.width, delta.height),
            (2, 2, 12, 12)
        );
        assert_eq!(delta.transparent, Some(TRANSPARENT_INDEX));
        let opaque = delta
            .buffer
            .iter()
            .filter(|&&i| i != TRANSPARENT_INDEX)
            .count();
        assert_eq!(opaque, 2);
    }
}
