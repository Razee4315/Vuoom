//! Speed-region and cut time remapping.
//!
//! When parts of a clip are sped up (to skim dead time) or cut out entirely, the *played*
//! (output) timeline is shorter than the *source* timeline. These pure functions convert
//! between the two so scrubbing and export sample the right source frame. A cut is simply
//! a region with an infinite speed factor — its output length is exactly zero. See
//! `docs/11-Editor-and-Annotations.md`.

use crate::{SpeedRegion, Trim};

/// Build a gap-filled, sorted list of `(src_start, src_end, factor)` covering
/// `[0, source_duration]`. Cuts join as infinite-factor regions. Invalid/overlapping
/// regions are skipped (first by start time wins); gaps play at 1.0×.
fn segments(source_duration: f64, regions: &[SpeedRegion], cuts: &[Trim]) -> Vec<(f64, f64, f64)> {
    let mut rs: Vec<SpeedRegion> = regions
        .iter()
        .copied()
        .filter(|r| r.end > r.start && r.factor > 0.0)
        .collect();
    rs.extend(cuts.iter().filter(|c| c.end > c.start).map(|c| SpeedRegion {
        start: c.start,
        end: c.end,
        factor: f64::INFINITY,
    }));
    rs.sort_by(|a, b| a.start.total_cmp(&b.start));

    let mut segs: Vec<(f64, f64, f64)> = Vec::new();
    let mut cursor = 0.0;
    for r in rs {
        let s = r.start.clamp(0.0, source_duration);
        let e = r.end.clamp(0.0, source_duration);
        if s < e && s >= cursor {
            if s > cursor {
                segs.push((cursor, s, 1.0));
            }
            segs.push((s, e, r.factor));
            cursor = e;
        }
    }
    if cursor < source_duration {
        segs.push((cursor, source_duration, 1.0));
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
}
