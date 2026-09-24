//! Drawing the mouse pointer into Desktop Duplication frames, which (unlike Windows Graphics
//! Capture frames) arrive without it.
//!
//! Windows reports three kinds of pointer image:
//! - **color**: 32-bit BGRA with straight alpha, blended over the screen;
//! - **masked color**: 32-bit BGRA where alpha is a mask: 0 replaces the screen pixel with the
//!   pointer's color, 0xFF inverts the screen pixel by XOR-ing it with that color (the text
//!   I-beam that stays visible on any background);
//! - **monochrome**: 1-bit AND and XOR masks stacked in one image twice the pointer's height:
//!   the screen pixel is AND-ed with the first and XOR-ed with the second, giving
//!   transparent, black, white or inverted pixels.

/// The kind of pointer image, as `DXGI_OUTDUPL_POINTER_SHAPE_TYPE` reports it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ShapeKind {
    #[default]
    Color,
    MaskedColor,
    Monochrome,
}

impl ShapeKind {
    /// From the `DXGI_OUTDUPL_POINTER_SHAPE_TYPE` value; `None` for an unknown kind.
    #[must_use]
    pub fn from_dxgi(kind: u32) -> Option<Self> {
        match kind {
            1 => Some(Self::Monochrome),
            2 => Some(Self::Color),
            4 => Some(Self::MaskedColor),
            _ => None,
        }
    }
}

/// A pointer image as Desktop Duplication reports it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PointerShape {
    pub kind: ShapeKind,
    pub width: u32,
    /// Rows of image data; for a monochrome pointer both masks, so twice its height.
    pub height: u32,
    /// Bytes from one row of `data` to the next.
    pub pitch: u32,
    pub data: Vec<u8>,
}

impl PointerShape {
    /// Rows the pointer covers on screen.
    #[must_use]
    pub fn rows(&self) -> u32 {
        match self.kind {
            ShapeKind::Monochrome => self.height / 2,
            _ => self.height,
        }
    }

    fn bgra(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        let i = (y * self.pitch + x * 4) as usize;
        let px = self.data.get(i..i + 4)?;
        Some([px[0], px[1], px[2], px[3]])
    }

    fn bit(&self, x: u32, y: u32) -> Option<bool> {
        let byte = self.data.get((y * self.pitch + x / 8) as usize)?;
        Some(byte & (0x80 >> (x % 8)) != 0)
    }
}

/// Draw `shape` into a tightly packed BGRA frame of `fw`×`fh`, with the pointer image's
/// top-left corner at frame pixel (`x`, `y`). Parts outside the frame are skipped.
pub fn draw(frame: &mut [u8], fw: u32, fh: u32, x: i32, y: i32, shape: &PointerShape) {
    let rows = shape.rows();
    for sy in 0..rows {
        let fy = i64::from(y) + i64::from(sy);
        if fy < 0 || fy >= i64::from(fh) {
            continue;
        }
        for sx in 0..shape.width {
            let fx = i64::from(x) + i64::from(sx);
            if fx < 0 || fx >= i64::from(fw) {
                continue;
            }
            let i = ((fy * i64::from(fw) + fx) * 4) as usize;
            let Some(px) = frame.get_mut(i..i + 4) else {
                continue;
            };
            match shape.kind {
                ShapeKind::Color => {
                    if let Some(s) = shape.bgra(sx, sy) {
                        blend(px, s);
                    }
                }
                ShapeKind::MaskedColor => {
                    if let Some(s) = shape.bgra(sx, sy) {
                        masked(px, s);
                    }
                }
                ShapeKind::Monochrome => {
                    let and = shape.bit(sx, sy);
                    let xor = shape.bit(sx, sy + rows);
                    if let (Some(and), Some(xor)) = (and, xor) {
                        mono(px, and, xor);
                    }
                }
            }
        }
    }
}

/// Straight-alpha blend of `s` over the screen pixel.
fn blend(px: &mut [u8], s: [u8; 4]) {
    let a = u32::from(s[3]);
    for (d, &c) in px[..3].iter_mut().zip(&s[..3]) {
        let v = (u32::from(c) * a + u32::from(*d) * (255 - a) + 127) / 255;
        *d = v as u8;
    }
}

/// Masked color: alpha 0 replaces the screen color, anything else inverts it by XOR.
fn masked(px: &mut [u8], s: [u8; 4]) {
    if s[3] == 0 {
        px[..3].copy_from_slice(&s[..3]);
    } else {
        for (d, &c) in px[..3].iter_mut().zip(&s[..3]) {
            *d ^= c;
        }
    }
}

/// Monochrome: `(screen AND and) XOR xor`, with each bit spread over the whole channel.
fn mono(px: &mut [u8], and: bool, xor: bool) {
    let a = if and { 0xFF } else { 0 };
    let x = if xor { 0xFF } else { 0 };
    for d in &mut px[..3] {
        *d = (*d & a) ^ x;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame of `w`×`h` pixels, all `bgr`.
    fn frame(w: u32, h: u32, bgr: [u8; 3]) -> Vec<u8> {
        (0..w * h)
            .flat_map(|_| [bgr[0], bgr[1], bgr[2], 255])
            .collect()
    }

    fn px(f: &[u8], w: u32, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * w + x) * 4) as usize;
        [f[i], f[i + 1], f[i + 2]]
    }

    fn color(kind: ShapeKind, pixels: &[[u8; 4]]) -> PointerShape {
        PointerShape {
            kind,
            width: pixels.len() as u32,
            height: 1,
            pitch: pixels.len() as u32 * 4,
            data: pixels.iter().flatten().copied().collect(),
        }
    }

    #[test]
    fn color_pointers_blend_by_alpha() {
        let mut f = frame(4, 1, [200, 0, 0]);
        let shape = color(
            ShapeKind::Color,
            &[
                [0, 0, 255, 255],
                [0, 0, 0, 0],
                [0, 0, 0, 128],
                [9, 9, 9, 255],
            ],
        );
        draw(&mut f, 4, 1, 0, 0, &shape);
        assert_eq!(px(&f, 4, 0, 0), [0, 0, 255], "opaque: the pointer's color");
        assert_eq!(px(&f, 4, 1, 0), [200, 0, 0], "transparent: the screen");
        assert_eq!(px(&f, 4, 2, 0), [100, 0, 0], "half: in between");
        assert!(f.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    }

    #[test]
    fn masked_color_replaces_or_inverts() {
        let mut f = frame(2, 1, [0x0F, 0xF0, 0xFF]);
        let shape = color(
            ShapeKind::MaskedColor,
            &[[1, 2, 3, 0], [0xFF, 0xFF, 0xFF, 0xFF]],
        );
        draw(&mut f, 2, 1, 0, 0, &shape);
        assert_eq!(px(&f, 2, 0, 0), [1, 2, 3]);
        assert_eq!(px(&f, 2, 1, 0), [0xF0, 0x0F, 0x00]);
    }

    #[test]
    fn monochrome_masks_give_all_four_results() {
        // Four pixels wide, one row tall: the AND mask row then the XOR mask row.
        // Pixels: (and, xor) = (1,0) keep, (0,0) black, (0,1) white, (1,1) invert.
        let shape = PointerShape {
            kind: ShapeKind::Monochrome,
            width: 4,
            height: 2,
            pitch: 1,
            data: vec![0b1001_0000, 0b0011_0000],
        };
        let mut f = frame(4, 1, [10, 20, 30]);
        draw(&mut f, 4, 1, 0, 0, &shape);
        assert_eq!(px(&f, 4, 0, 0), [10, 20, 30]);
        assert_eq!(px(&f, 4, 1, 0), [0, 0, 0]);
        assert_eq!(px(&f, 4, 2, 0), [255, 255, 255]);
        assert_eq!(px(&f, 4, 3, 0), [245, 235, 225]);
    }

    #[test]
    fn pointers_partly_off_the_frame_are_clipped() {
        let mut f = frame(2, 2, [0, 0, 0]);
        let white = [255, 255, 255, 255];
        let shape = PointerShape {
            kind: ShapeKind::Color,
            width: 2,
            height: 2,
            pitch: 8,
            data: [white; 4].iter().flatten().copied().collect(),
        };
        // Hanging off the top left: only the frame's first pixel is covered.
        draw(&mut f, 2, 2, -1, -1, &shape);
        assert_eq!(px(&f, 2, 0, 0), [255, 255, 255]);
        assert_eq!(px(&f, 2, 1, 1), [0, 0, 0]);
        // Entirely outside: nothing changes, nothing panics.
        draw(&mut f, 2, 2, 50, -50, &shape);
        assert_eq!(ShapeKind::from_dxgi(3), None);
    }
}
