//! Timeline editing operations for zoom keyframes.
//!
//! The editor mutates the *same* `ZoomKeyframe` list the planner produced (spec §5.1:
//! auto-placed zooms are fully editable). These helpers keep the list valid — sorted by
//! start, clamped to the clip, and never shorter than [`MIN_LEN`]. Pure logic, unit-tested.

use crate::keyframe::ZoomKeyframe;

/// Minimum zoom-segment length (seconds) the editor will allow.
pub const MIN_LEN: f64 = 0.2;

/// Sort keyframes by start time (stable).
pub fn sort_by_start(zooms: &mut [ZoomKeyframe]) {
    zooms.sort_by(|a, b| a.start.total_cmp(&b.start));
}

/// Insert a keyframe, keeping the list sorted by start. Returns its index.
pub fn insert_sorted(zooms: &mut Vec<ZoomKeyframe>, kf: ZoomKeyframe) -> usize {
    let idx = zooms.partition_point(|k| k.start <= kf.start);
    zooms.insert(idx, kf);
    idx
}

/// Remove the keyframe at `index`. Returns it, or `None` if out of range.
pub fn remove(zooms: &mut Vec<ZoomKeyframe>, index: usize) -> Option<ZoomKeyframe> {
    if index < zooms.len() {
        Some(zooms.remove(index))
    } else {
        None
    }
}

/// Move the keyframe at `index` so it starts at `new_start`, preserving its duration and
/// clamping to `[0, duration]`. Re-sorts the list. Returns `false` if `index` is invalid.
pub fn move_to(zooms: &mut [ZoomKeyframe], index: usize, new_start: f64, duration: f64) -> bool {
    let Some(kf) = zooms.get_mut(index) else {
        return false;
    };
    let len = (kf.end - kf.start).max(MIN_LEN);
    let start = new_start.clamp(0.0, (duration - len).max(0.0));
    kf.start = start;
    kf.end = start + len;
    sort_by_start(zooms);
    true
}

/// Resize the keyframe at `index` to `[new_start, new_end]`, clamped to `[0, duration]`
/// and enforcing [`MIN_LEN`]. Returns `false` if the clip is too short or `index` invalid.
pub fn resize(
    zooms: &mut [ZoomKeyframe],
    index: usize,
    new_start: f64,
    new_end: f64,
    duration: f64,
) -> bool {
    if duration < MIN_LEN {
        return false;
    }
    let Some(kf) = zooms.get_mut(index) else {
        return false;
    };
    let start = new_start.clamp(0.0, duration - MIN_LEN);
    let end = new_end.clamp(start + MIN_LEN, duration);
    kf.start = start;
    kf.end = end;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyframe::ZoomMode;

    fn kf(start: f64, end: f64) -> ZoomKeyframe {
        ZoomKeyframe {
            start,
            end,
            amount: 1.8,
            mode: ZoomMode::Auto,
            edge_snap_ratio: 0.25,
        }
    }

    #[test]
    fn insert_keeps_sorted() {
        let mut z = vec![kf(0.0, 1.0), kf(5.0, 6.0)];
        let i = insert_sorted(&mut z, kf(2.0, 3.0));
        assert_eq!(i, 1);
        assert!(z.windows(2).all(|w| w[0].start <= w[1].start));
    }

    #[test]
    fn move_preserves_duration_and_clamps() {
        let mut z = vec![kf(1.0, 3.0)]; // duration 2
        assert!(move_to(&mut z, 0, 100.0, 10.0)); // clamp so it fits in [0,10]
        assert!((z[0].duration() - 2.0).abs() < 1e-9);
        assert!((z[0].end - 10.0).abs() < 1e-9);
        assert!((z[0].start - 8.0).abs() < 1e-9);
    }

    #[test]
    fn resize_enforces_min_len() {
        let mut z = vec![kf(1.0, 5.0)];
        // Try to collapse it to zero length; MIN_LEN is enforced.
        assert!(resize(&mut z, 0, 2.0, 2.0, 10.0));
        assert!(z[0].duration() >= MIN_LEN - 1e-9);
    }

    #[test]
    fn remove_out_of_range_is_none() {
        let mut z = vec![kf(0.0, 1.0)];
        assert!(remove(&mut z, 5).is_none());
        assert!(remove(&mut z, 0).is_some());
        assert!(z.is_empty());
    }
}
