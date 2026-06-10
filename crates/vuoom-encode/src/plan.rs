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

/// Estimate the total size of a **delta-encoded** GIF from contiguous sample windows.
///
/// The encoder writes one full keyframe and then per-frame delta rectangles, so size is
/// `keyframe + per-delta × (frames − 1)` — extrapolating raw window bytes linearly would
/// count the window's keyframe once per window-length and badly overestimate. Each
/// window is `(encoded_bytes, keyframe_bytes, frame_count)`, where `keyframe_bytes` is a
/// 1-frame encode of the window's first frame; subtracting it isolates the delta cost.
#[must_use]
pub fn estimate_delta_total_bytes(
    windows: &[(u64, u64, usize)],
    total_frames: usize,
    motion_factor: f64,
) -> u64 {
    if total_frames == 0 || windows.is_empty() {
        return 0;
    }
    let (mut key_bytes, mut keys) = (0u64, 0u64);
    let (mut delta_bytes, mut delta_frames) = (0f64, 0usize);
    for &(bytes, kbytes, frames) in windows {
        if frames == 0 {
            continue;
        }
        key_bytes += kbytes;
        keys += 1;
        if frames > 1 {
            delta_bytes += bytes.saturating_sub(kbytes) as f64;
            delta_frames += frames - 1;
        }
    }
    if keys == 0 {
        return 0;
    }
    let key_avg = key_bytes as f64 / keys as f64;
    let per_delta = if delta_frames > 0 {
        delta_bytes / delta_frames as f64
    } else {
        0.0
    };
    ((key_avg + per_delta * (total_frames - 1) as f64) * motion_factor.max(1.0)).round() as u64
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
    fn delta_estimate_amortizes_the_keyframe() {
        // One 9-frame window: 50KB encoded, 10KB keyframe -> 5KB per delta frame.
        let e = estimate_delta_total_bytes(&[(50_000, 10_000, 9)], 901, 1.0);
        assert_eq!(e, 10_000 + 5_000 * 900);
        // Naive linear extrapolation would have said ~5MB for a static-ish clip too,
        // but with cheap deltas the estimate tracks the keyframe + delta structure:
        let cheap = estimate_delta_total_bytes(&[(11_600, 10_000, 9)], 901, 1.0);
        assert_eq!(cheap, 10_000 + 200 * 900);
    }

    #[test]
    fn delta_estimate_averages_multiple_windows() {
        // Windows with 1KB and 3KB per delta -> 2KB average; 10KB average keyframe.
        let windows = [(14_000u64, 10_000u64, 5usize), (22_000, 10_000, 5)];
        let e = estimate_delta_total_bytes(&windows, 101, 1.0);
        assert_eq!(e, 10_000 + 2_000 * 100);
    }

    #[test]
    fn delta_estimate_handles_degenerate_inputs() {
        assert_eq!(estimate_delta_total_bytes(&[], 100, 1.0), 0);
        assert_eq!(
            estimate_delta_total_bytes(&[(10_000, 10_000, 1)], 0, 1.0),
            0
        );
        // Single-frame clip: just the keyframe.
        assert_eq!(
            estimate_delta_total_bytes(&[(10_000, 10_000, 1)], 1, 1.0),
            10_000
        );
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
