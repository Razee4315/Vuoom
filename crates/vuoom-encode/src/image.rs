//! RGBA frame container + PNG writer (the format gifski consumes).

use crate::error::EncodeError;
use std::fs::File;
use std::io::BufWriter;
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
}
