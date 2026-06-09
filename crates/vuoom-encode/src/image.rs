//! RGBA frame container + PNG writer (the format gifski consumes).

use crate::error::EncodeError;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

/// An 8-bit RGBA image: `pixels.len() == width * height * 4`.
#[derive(Debug, Clone)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    #[must_use]
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        Self {
            width,
            height,
            pixels,
        }
    }

    /// Whether the pixel buffer length matches `width * height * 4`.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.pixels.len() == (self.width as usize) * (self.height as usize) * 4
    }
}

/// Write an [`RgbaImage`] to `path` as a PNG.
///
/// # Errors
/// Returns [`EncodeError`] on I/O failure or if the pixel buffer is the wrong size.
pub fn write_png(path: &Path, img: &RgbaImage) -> Result<(), EncodeError> {
    if !img.is_valid() {
        return Err(EncodeError::Png(format!(
            "pixel buffer {} != {}x{}x4",
            img.pixels.len(),
            img.width,
            img.height
        )));
    }
    let file = File::create(path)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), img.width, img.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| EncodeError::Png(e.to_string()))?;
    writer
        .write_image_data(&img.pixels)
        .map_err(|e| EncodeError::Png(e.to_string()))?;
    Ok(())
}

/// Read an RGBA8 PNG written by [`write_png`] back into an [`RgbaImage`].
///
/// # Errors
/// Returns [`EncodeError`] on I/O failure or an unsupported (non-8-bit / exotic) PNG.
pub fn read_png(path: &Path) -> Result<RgbaImage, EncodeError> {
    let file = File::open(path)?;
    let decoder = png::Decoder::new(BufReader::new(file));
    let mut reader = decoder
        .read_info()
        .map_err(|e| EncodeError::Png(e.to_string()))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| EncodeError::Png(e.to_string()))?;
    buf.truncate(info.buffer_size());
    if info.bit_depth != png::BitDepth::Eight {
        return Err(EncodeError::Png(format!(
            "unsupported PNG bit depth {:?}",
            info.bit_depth
        )));
    }
    let pixels = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity(buf.len() / 3 * 4);
            for px in buf.chunks_exact(3) {
                out.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            out
        }
        other => {
            return Err(EncodeError::Png(format!(
                "unsupported PNG color type {other:?}"
            )))
        }
    };
    Ok(RgbaImage::new(info.width, info.height, pixels))
}

/// Swap the red and blue channels of a 4-byte-per-pixel buffer (BGRA↔RGBA — the op is its
/// own inverse). Panics-free: a trailing partial pixel is left untouched.
#[must_use]
pub fn swizzle_rb(src: &[u8]) -> Vec<u8> {
    let mut out = src.to_vec();
    for px in out.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_buffer_check() {
        assert!(RgbaImage::new(2, 2, vec![0u8; 16]).is_valid());
        assert!(!RgbaImage::new(2, 2, vec![0u8; 15]).is_valid());
    }

    #[test]
    fn writes_valid_png_signature() {
        let img = RgbaImage::new(2, 2, vec![255u8; 16]);
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.png");
        write_png(&p, &img).unwrap();
        let bytes = std::fs::read(&p).unwrap();
        assert_eq!(
            &bytes[0..8],
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
        );
    }

    #[test]
    fn wrong_size_buffer_errors() {
        let img = RgbaImage::new(2, 2, vec![0u8; 10]);
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("bad.png");
        assert!(matches!(write_png(&p, &img), Err(EncodeError::Png(_))));
    }

    #[test]
    fn png_round_trips_rgba() {
        let img = RgbaImage::new(2, 1, vec![1, 2, 3, 255, 4, 5, 6, 255]);
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("rt.png");
        write_png(&p, &img).unwrap();
        let back = read_png(&p).unwrap();
        assert_eq!((back.width, back.height), (2, 1));
        assert_eq!(back.pixels, img.pixels);
    }

    #[test]
    fn swizzle_is_its_own_inverse() {
        let bgra = vec![10, 20, 30, 255];
        let rgba = swizzle_rb(&bgra);
        assert_eq!(rgba, vec![30, 20, 10, 255]);
        assert_eq!(swizzle_rb(&rgba), bgra);
    }
}
