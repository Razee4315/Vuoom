//! The preview wire format.
//!
//! Composited RGBA is sent to the webview as a single binary WebSocket message: the raw
//! pixels followed by a fixed 24-byte little-endian trailer. The webview's worker reads
//! the trailer to size and un-pad the frame. This mirrors Cap's proven layout (the stride
//! is carried because `copy_texture_to_buffer` pads rows to 256 bytes). See
//! `docs/05-Compositing-and-Preview.md`.

/// Trailing metadata for one preview frame, appended after the RGBA bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameMeta {
    /// Bytes per row in the RGBA payload (may be padded above `width * 4`).
    pub stride: u32,
    pub height: u32,
    pub width: u32,
    /// Monotonic frame counter (for the worker's ordering/coalescing).
    pub frame_number: u32,
    /// Presentation time in nanoseconds (for playback timing).
    pub target_time_ns: u64,
}

/// Size of the binary trailer in bytes (4+4+4+4+8).
pub const META_LEN: usize = 24;

/// Pack RGBA pixels + metadata into one binary message.
#[must_use]
pub fn pack_frame(rgba: &[u8], meta: FrameMeta) -> Vec<u8> {
    let mut buf = Vec::with_capacity(rgba.len() + META_LEN);
    buf.extend_from_slice(rgba);
    buf.extend_from_slice(&meta.stride.to_le_bytes());
    buf.extend_from_slice(&meta.height.to_le_bytes());
    buf.extend_from_slice(&meta.width.to_le_bytes());
    buf.extend_from_slice(&meta.frame_number.to_le_bytes());
    buf.extend_from_slice(&meta.target_time_ns.to_le_bytes());
    buf
}

/// Read the trailing [`FrameMeta`] from a packed message (`None` if too short).
#[must_use]
pub fn parse_meta(buf: &[u8]) -> Option<FrameMeta> {
    if buf.len() < META_LEN {
        return None;
    }
    let m = &buf[buf.len() - META_LEN..];
    Some(FrameMeta {
        stride: u32::from_le_bytes(m[0..4].try_into().ok()?),
        height: u32::from_le_bytes(m[4..8].try_into().ok()?),
        width: u32::from_le_bytes(m[8..12].try_into().ok()?),
        frame_number: u32::from_le_bytes(m[12..16].try_into().ok()?),
        target_time_ns: u64::from_le_bytes(m[16..24].try_into().ok()?),
    })
}

/// The RGBA payload slice of a packed message (everything before the trailer).
#[must_use]
pub fn payload(buf: &[u8]) -> &[u8] {
    let end = buf.len().saturating_sub(META_LEN);
    &buf[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_then_parse_round_trips() {
        let rgba = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let meta = FrameMeta {
            stride: 256,
            height: 2,
            width: 1,
            frame_number: 7,
            target_time_ns: 1_234_567,
        };
        let buf = pack_frame(&rgba, meta);
        assert_eq!(buf.len(), rgba.len() + META_LEN);
        assert_eq!(parse_meta(&buf), Some(meta));
        assert_eq!(payload(&buf), &rgba[..]);
    }

    #[test]
    fn too_short_is_none() {
        assert_eq!(parse_meta(&[0u8; 10]), None);
        assert!(payload(&[0u8; 10]).is_empty());
    }
}
