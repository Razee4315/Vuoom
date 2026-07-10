//! Speed-region and cut time remapping.
//!
//! When parts of a clip are sped up (to skim dead time) or cut out entirely, the *played*
//! (output) timeline is shorter than the *source* timeline. These pure functions convert
//! between the two so scrubbing and export sample the right source frame. A cut is simply
//! a region with an infinite speed factor — its output length is exactly zero. See
//! `docs/11-Editor-and-Annotations.md`.

use crate::{SpeedRegion, Trim};

/// Build a gap-filled, sorted list of `(src_start, src_end, factor)` covering
/// `[0, source_duration]`.
///
/// Semantics — **cuts always win**. Any source instant inside a cut is removed
/// (emitted as an infinite-factor, zero-output segment) regardless of whether a
/// speed region also covers it, so a cut nested in a sped span never plays. Cuts
/// are clamped and merged first (overlapping/adjacent cuts fuse). The surviving
/// (un-cut) time then takes the factor of the first — earliest-starting — speed
/// region covering it; where two speed regions overlap, the earlier one wins the
/// overlap (deterministic and simplest correct rule). Uncovered gaps play at 1.0×.
fn segments(source_duration: f64, regions: &[SpeedRegion], cuts: &[Trim]) -> Vec<(f64, f64, f64)> {
    // Valid speed regions, clamped to the clip and sorted by start so the
    // earliest-starting region wins any overlap.
    let mut speeds: Vec<SpeedRegion> = regions
        .iter()
        .copied()
        .filter(|r| r.end > r.start && r.factor > 0.0)
        .map(|r| SpeedRegion {
            start: r.start.clamp(0.0, source_duration),
            end: r.end.clamp(0.0, source_duration),
            factor: r.factor,
        })
        .filter(|r| r.end > r.start)
        .collect();
    speeds.sort_by(|a, b| a.start.total_cmp(&b.start));

    // Valid cut spans, clamped and merged so overlapping/adjacent cuts fuse.
    let mut cut_spans: Vec<(f64, f64)> = cuts
        .iter()
        .filter(|c| c.end > c.start)
        .map(|c| {
            (
                c.start.clamp(0.0, source_duration),
                c.end.clamp(0.0, source_duration),
            )
        })
        .filter(|(s, e)| e > s)
        .collect();
    cut_spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut cuts_merged: Vec<(f64, f64)> = Vec::new();
    for (s, e) in cut_spans {
        match cuts_merged.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => cuts_merged.push((s, e)),
        }
    }

    // Sweep every boundary. Each sub-span between consecutive boundaries is
    // uniform: use its midpoint to decide the factor. Cuts take precedence.
    let mut bounds: Vec<f64> = vec![0.0, source_duration];
    for r in &speeds {
        bounds.push(r.start);
        bounds.push(r.end);
    }
    for &(s, e) in &cuts_merged {
        bounds.push(s);
        bounds.push(e);
    }
    bounds.sort_by(f64::total_cmp);
    bounds.dedup_by(|a, b| (*a - *b).abs() < 1e-12);

    let mut segs: Vec<(f64, f64, f64)> = Vec::new();
    for w in bounds.windows(2) {
        let (a, b) = (w[0], w[1]);
        if b - a <= 1e-12 {
            continue;
        }
        let mid = 0.5 * (a + b);
        let factor = if cuts_merged.iter().any(|&(s, e)| mid >= s && mid < e) {
            f64::INFINITY
        } else {
            speeds
                .iter()
                .find(|r| mid >= r.start && mid < r.end)
                .map_or(1.0, |r| r.factor)
        };
        // Coalesce touching sub-spans that share a factor for a tidy list
        // (INFINITY == INFINITY, so adjacent cut pieces fuse too).
        match segs.last_mut() {
            Some(last) if last.2 == factor => last.1 = b,
            _ => segs.push((a, b, factor)),
        }
    }
    segs
}

/// Total played (output) duration after applying speed regions and cuts.
#[must_use]
pub fn output_duration(source_duration: f64, regions: &[SpeedRegion], cuts: &[Trim]) -> f64 {
    segments(source_duration, regions, cuts)
        .iter()
        .map(|&(s, e, f)| (e - s) / f)
        .sum()
}

/// Map an output (played) time to the source time to sample.
#[must_use]
pub fn output_to_source(
    t_out: f64,
    source_duration: f64,
    regions: &[SpeedRegion],
    cuts: &[Trim],
) -> f64 {
    let mut acc = 0.0;
    for (s, e, f) in segments(source_duration, regions, cuts) {
        let out_len = (e - s) / f;
        // Zero-length (cut) segments can never own an output time.
        if out_len > 1e-12 && t_out <= acc + out_len {
            return s + (t_out - acc) * f;
        }
        acc += out_len;
    }
    source_duration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_regions_is_identity() {
        assert!((output_duration(6.0, &[], &[]) - 6.0).abs() < 1e-9);
        assert!((output_to_source(2.5, 6.0, &[], &[]) - 2.5).abs() < 1e-9);
    }

    #[test]
    fn sped_region_shortens_output() {
        // [2,4] at 2x: 2s of source plays in 1s. Total 6s -> 5s output.
        let regions = [SpeedRegion {
            start: 2.0,
            end: 4.0,
            factor: 2.0,
        }];
        assert!((output_duration(6.0, &regions, &[]) - 5.0).abs() < 1e-9);
        // Output t=2.5 is mid-region -> source 3.0.
        assert!((output_to_source(2.5, 6.0, &regions, &[]) - 3.0).abs() < 1e-9);
        // After the region, output t=4.0 -> source 5.0.
        assert!((output_to_source(4.0, 6.0, &regions, &[]) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn cut_removes_its_span_from_output() {
        // Cut [2,4]: 6s -> 4s of output; times after the cut shift back by 2s.
        let cuts = [Trim {
            start: 2.0,
            end: 4.0,
        }];
        assert!((output_duration(6.0, &[], &cuts) - 4.0).abs() < 1e-9);
        // Before the cut: identity.
        assert!((output_to_source(1.5, 6.0, &[], &cuts) - 1.5).abs() < 1e-9);
        // The instant the cut starts, output jumps past it.
        assert!((output_to_source(2.5, 6.0, &[], &cuts) - 4.5).abs() < 1e-9);
    }

    #[test]
    fn cut_and_speed_compose() {
        // [1,2] at 2x (plays in 0.5s) + cut [3,5]: 6s -> 0..1 (1s) + 1..2 (0.5s) + 2..3 (1s) + 5..6 (1s) = 3.5s.
        let regions = [SpeedRegion {
            start: 1.0,
            end: 2.0,
            factor: 2.0,
        }];
        let cuts = [Trim {
            start: 3.0,
            end: 5.0,
        }];
        assert!((output_duration(6.0, &regions, &cuts) - 3.5).abs() < 1e-9);
        // Output 3.0 lands 0.5s past the cut -> source 5.5.
        assert!((output_to_source(3.0, 6.0, &regions, &cuts) - 5.5).abs() < 1e-9);
    }

    #[test]
    fn cut_inside_speed_still_removes_cut() {
        // speed [2,8] at 4x with a cut [3,4] nested inside: the cut must win.
        // Output = 0..2 (2s) + 2..3 @4x (0.25s) + 3..4 cut (0s) + 4..8 @4x (1s) + 8..10 (2s) = 5.25s.
        let regions = [SpeedRegion {
            start: 2.0,
            end: 8.0,
            factor: 4.0,
        }];
        let cuts = [Trim {
            start: 3.0,
            end: 4.0,
        }];
        assert!((output_duration(10.0, &regions, &cuts) - 5.25).abs() < 1e-9);
        // An output time just inside the sped span before the cut stays before it.
        assert!((output_to_source(2.1, 10.0, &regions, &cuts) - 2.4).abs() < 1e-9);
        // An output time just past the cut boundary jumps over the removed [3,4].
        assert!((output_to_source(2.3, 10.0, &regions, &cuts) - 4.2).abs() < 1e-9);
    }

    #[test]
    fn cut_overlapping_speed_start() {
        // cut [1,3] overlaps the start of speed [2,6] @2x. Only [3,6] is sped;
        // [2,3] is swallowed by the cut. Output = 0..1 (1s) + 1..3 cut (0s) + 3..6 @2x (1.5s) + 6..10 (4s) = 6.5s.
        let regions = [SpeedRegion {
            start: 2.0,
            end: 6.0,
            factor: 2.0,
        }];
        let cuts = [Trim {
            start: 1.0,
            end: 3.0,
        }];
        assert!((output_duration(10.0, &regions, &cuts) - 6.5).abs() < 1e-9);
        // Output 1.5 is 0.5s past the cut, inside the @2x span: 3 + (1.5-1)*2 = 4.0.
        assert!((output_to_source(1.5, 10.0, &regions, &cuts) - 4.0).abs() < 1e-9);
    }

    #[test]
    fn cut_overlapping_speed_end() {
        // speed [2,6] @2x with cut [5,8] over its tail. Only [2,5] is sped.
        // Output = 0..2 (2s) + 2..5 @2x (1.5s) + 5..8 cut (0s) + 8..10 (2s) = 5.5s.
        let regions = [SpeedRegion {
            start: 2.0,
            end: 6.0,
            factor: 2.0,
        }];
        let cuts = [Trim {
            start: 5.0,
            end: 8.0,
        }];
        assert!((output_duration(10.0, &regions, &cuts) - 5.5).abs() < 1e-9);
        // Output 3.5 lands mid sped span: 2 + (3.5-2)*2 = 5.0.
        assert!((output_to_source(3.5, 10.0, &regions, &cuts) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn speed_fully_inside_cut_has_no_effect() {
        // speed [4,6] swallowed by cut [2,8]: identical to the cut alone.
        let regions = [SpeedRegion {
            start: 4.0,
            end: 6.0,
            factor: 4.0,
        }];
        let cuts = [Trim {
            start: 2.0,
            end: 8.0,
        }];
        let with_speed = output_duration(10.0, &regions, &cuts);
        let cut_only = output_duration(10.0, &[], &cuts);
        assert!((with_speed - cut_only).abs() < 1e-9);
        // 0..2 (2s) + 2..8 cut (0s) + 8..10 (2s) = 4.0s.
        assert!((with_speed - 4.0).abs() < 1e-9);
    }

    #[test]
    fn overlapping_cuts_merge() {
        // cut [2,5] and cut [4,8] fuse into [2,8]; same as a single cut [2,8].
        let cuts = [
            Trim {
                start: 2.0,
                end: 5.0,
            },
            Trim {
                start: 4.0,
                end: 8.0,
            },
        ];
        assert!((output_duration(10.0, &[], &cuts) - 4.0).abs() < 1e-9);
        // Output 2.5 (0.5s past the merged cut) -> source 8.5, proving the union is removed.
        assert!((output_to_source(2.5, 10.0, &[], &cuts) - 8.5).abs() < 1e-9);
    }

    #[test]
    fn overlapping_speeds_earliest_wins() {
        // speed [2,6] @2x and speed [4,8] @4x overlap on [4,6]; the earlier (2x) wins the overlap.
        // Output = 0..2 (2s) + 2..6 @2x (2s) + 6..8 @4x (0.5s) + 8..10 (2s) = 6.5s.
        let regions = [
            SpeedRegion {
                start: 2.0,
                end: 6.0,
                factor: 2.0,
            },
            SpeedRegion {
                start: 4.0,
                end: 8.0,
                factor: 4.0,
            },
        ];
        assert!((output_duration(10.0, &regions, &[]) - 6.5).abs() < 1e-9);
        // Output 3.5 lands in the overlap [4,6] at 2x (not 4x): 2 + (3.5-2)*2 = 5.0.
        assert!((output_to_source(3.5, 10.0, &regions, &[]) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn cut_at_exact_speed_boundary() {
        // speed [2,4] @2x abutting cut [4,6]; no overlap interaction.
        // Output = 0..2 (2s) + 2..4 @2x (1s) + 4..6 cut (0s) + 6..10 (4s) = 7.0s.
        let regions = [SpeedRegion {
            start: 2.0,
            end: 4.0,
            factor: 2.0,
        }];
        let cuts = [Trim {
            start: 4.0,
            end: 6.0,
        }];
        assert!((output_duration(10.0, &regions, &cuts) - 7.0).abs() < 1e-9);
        // Output 3.0 lands mid sped span: 2 + (3.0-2)*2 = 4.0 (right at the cut start, still valid source).
        assert!((output_to_source(3.0, 10.0, &regions, &cuts) - 4.0).abs() < 1e-9);
    }
}
