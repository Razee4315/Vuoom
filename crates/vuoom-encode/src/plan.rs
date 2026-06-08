//! Pure-logic GIF planning: which source frames to emit, their presentation times, and
//! how to estimate the output size before committing to a full encode.
//!
//! See `docs/06-Export.md` (frame selection + sample-and-extrapolate size estimation).

/// One frame selected for the GIF: which source frame it samples and when it shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmittedFrame {
    /// Index into the source (composited) frame sequence.
    pub source_index: usize,
    /// Presentation time in seconds (this is how gifski controls timing).
    pub pts: f64,
}

/// Select source frames to hit `target_fps` from a `source_fps` sequence of
/// `source_frames` frames. Never emits past the source; pts is on the emitted timeline.
#[must_use]
pub fn plan_frames(source_frames: usize, source_fps: f64, target_fps: f64) -> Vec<EmittedFrame> {
    if source_frames == 0 || source_fps <= 0.0 || target_fps <= 0.0 {
        return Vec::new();
    }
    let step = source_fps / target_fps;
    let count = ((source_frames as f64) / step).ceil().max(1.0) as usize;
    let mut out = Vec::with_capacity(count);
    for k in 0..count {
        let idx = ((k as f64) * step).round() as usize;
        if idx >= source_frames {
            break;
        }
        out.push(EmittedFrame {
            source_index: idx,
            pts: k as f64 / target_fps,
        });
    }
    out
}

/// Estimate total GIF size by extrapolating from an encoded sample.
///
/// GIF has no closed-form size formula (gifski builds per-frame palettes + diffs
/// temporally), so we encode a representative sample and scale linearly by frame count,
/// nudged up by a motion factor (`>= 1.0`; higher = more inter-frame change). See
/// `docs/06-Export.md`.
#[must_use]
pub fn estimate_total_bytes(
    sample_bytes: u64,
    sample_frames: usize,
    total_frames: usize,
    motion_factor: f64,
) -> u64 {
    if sample_frames == 0 || total_frames == 0 {
        return 0;
    }
    let per_frame = sample_bytes as f64 / sample_frames as f64;
    (per_frame * total_frames as f64 * motion_factor.max(1.0)).round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downsamples_60_to_15_fps() {
        // 60 source frames (1s @ 60fps) -> ~15 emitted frames.
        let frames = plan_frames(60, 60.0, 15.0);
        assert_eq!(frames.len(), 15);
        assert_eq!(frames[0].source_index, 0);
        assert_eq!(frames[1].source_index, 4); // step = 4
                                               // pts is on the emitted (15fps) timeline.
        assert!((frames[1].pts - 1.0 / 15.0).abs() < 1e-9);
    }

    #[test]
    fn never_emits_past_source() {
        let frames = plan_frames(10, 60.0, 15.0);
        assert!(frames.iter().all(|f| f.source_index < 10));
    }

    #[test]
    fn empty_inputs_yield_no_frames() {
        assert!(plan_frames(0, 60.0, 15.0).is_empty());
        assert!(plan_frames(60, 0.0, 15.0).is_empty());
    }

    #[test]
    fn size_estimate_scales_with_frames_and_motion() {
        // 100KB over 10 frames -> ~10KB/frame.
        let base = estimate_total_bytes(100_000, 10, 150, 1.0);
        assert_eq!(base, 1_500_000);
        // Higher motion inflates the estimate.
        let busy = estimate_total_bytes(100_000, 10, 150, 1.4);
        assert_eq!(busy, 2_100_000);
    }
}
