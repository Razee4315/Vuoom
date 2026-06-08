//! Speed-region time remapping.
//!
//! When parts of a clip are sped up (to skim dead time), the *played* (output) timeline
//! is shorter than the *source* timeline. These pure functions convert between the two so
//! scrubbing and export sample the right source frame. See `docs/11-Editor-and-Annotations.md`.

use crate::SpeedRegion;

/// Build a gap-filled, sorted list of `(src_start, src_end, factor)` covering
/// `[0, source_duration]`. Invalid/overlapping regions are skipped; gaps play at 1.0×.
fn segments(source_duration: f64, regions: &[SpeedRegion]) -> Vec<(f64, f64, f64)> {
    let mut rs: Vec<SpeedRegion> = regions
        .iter()
        .copied()
        .filter(|r| r.end > r.start && r.factor > 0.0)
        .collect();
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

/// Total played (output) duration after applying speed regions.
#[must_use]
pub fn output_duration(source_duration: f64, regions: &[SpeedRegion]) -> f64 {
    segments(source_duration, regions)
        .iter()
        .map(|&(s, e, f)| (e - s) / f)
        .sum()
}

/// Map an output (played) time to the source time to sample.
#[must_use]
pub fn output_to_source(t_out: f64, source_duration: f64, regions: &[SpeedRegion]) -> f64 {
    let mut acc = 0.0;
    for (s, e, f) in segments(source_duration, regions) {
        let out_len = (e - s) / f;
        if t_out <= acc + out_len {
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
        assert!((output_duration(6.0, &[]) - 6.0).abs() < 1e-9);
        assert!((output_to_source(2.5, 6.0, &[]) - 2.5).abs() < 1e-9);
    }

    #[test]
    fn sped_region_shortens_output() {
        // [2,4] at 2x: 2s of source plays in 1s. Total 6s -> 5s output.
        let regions = [SpeedRegion {
            start: 2.0,
            end: 4.0,
            factor: 2.0,
        }];
        assert!((output_duration(6.0, &regions) - 5.0).abs() < 1e-9);
        // Output t=2.5 is mid-region -> source 3.0.
        assert!((output_to_source(2.5, 6.0, &regions) - 3.0).abs() < 1e-9);
        // After the region, output t=4.0 -> source 5.0.
        assert!((output_to_source(4.0, 6.0, &regions) - 5.0).abs() < 1e-9);
    }
}
