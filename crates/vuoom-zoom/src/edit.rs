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

/// Insert a keyframe, keeping the list sorted by start and **free of overlap**.
///
/// The requested `[start, end]` is clamped into the gap between the neighbouring
/// segments (previous segment's end .. next segment's start) rather than being allowed
/// to overlap them. The segment keeps its requested length when the gap is large enough,
/// otherwise it shrinks to fill the gap. If the gap can't hold [`MIN_LEN`], the insert is
/// rejected and `None` is returned. On success returns the insertion index.
pub fn insert_sorted(zooms: &mut Vec<ZoomKeyframe>, mut kf: ZoomKeyframe) -> Option<usize> {
    let idx = zooms.partition_point(|k| k.start <= kf.start);
    // Facing edges of the neighbours on either side of the requested spot.
    let gap_lo = idx
        .checked_sub(1)
        .and_then(|i| zooms.get(i))
        .map_or(0.0, |k| k.end);
    let gap_hi = zooms.get(idx).map_or(f64::INFINITY, |k| k.start);
    let width = gap_hi - gap_lo;
    if width < MIN_LEN {
        return None; // no room near the requested spot
    }
    let len = (kf.end - kf.start).max(MIN_LEN).min(width);
    let start = kf.start.clamp(gap_lo, gap_hi - len);
    kf.start = start;
    kf.end = start + len;
    zooms.insert(idx, kf);
    Some(idx)
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
/// clamping to `[0, duration]`. The segment also stops at its neighbours' facing edges
/// instead of crossing into them, so overlap is impossible. Re-sorts the list. Returns
/// `false` if `index` is invalid.
pub fn move_to(zooms: &mut [ZoomKeyframe], index: usize, new_start: f64, duration: f64) -> bool {
    // Neighbour edges (computed before the mutable borrow below).
    let prev_end = index
        .checked_sub(1)
        .and_then(|i| zooms.get(i))
        .map_or(0.0, |k| k.end);
    let next_start = zooms.get(index + 1).map_or(duration, |k| k.start);
    let Some(kf) = zooms.get_mut(index) else {
        return false;
    };
    let len = (kf.end - kf.start).max(MIN_LEN);
    let lo = prev_end.max(0.0);
    let hi = (next_start.min(duration) - len).max(lo);
    let start = new_start.clamp(lo, hi);
    kf.start = start;
    kf.end = start + len;
    sort_by_start(zooms);
    true
}

/// Resize the keyframe at `index` to `[new_start, new_end]`, clamped to `[0, duration]`
/// and enforcing [`MIN_LEN`]. Each edge is additionally clamped to its neighbour's facing
/// edge (previous segment's end, next segment's start) so the segment can never grow into
/// an adjacent zoom. Returns `false` if the clip is too short or `index` invalid.
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
    // Neighbour edges (computed before the mutable borrow below).
    let prev_end = index
        .checked_sub(1)
        .and_then(|i| zooms.get(i))
        .map_or(0.0, |k| k.end);
    let next_start = zooms.get(index + 1).map_or(duration, |k| k.start);
    let Some(kf) = zooms.get_mut(index) else {
        return false;
    };
    let start_hi = (next_start - MIN_LEN).max(prev_end);
    let start = new_start.clamp(prev_end, start_hi);
    // `.max(...)` keeps the clamp bounds ordered even on legacy data where neighbours
    // already overlapped (min > max would panic); the invariant repairs itself on edit.
    let end = new_end.clamp(start + MIN_LEN, next_start.max(start + MIN_LEN));
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
        assert_eq!(i, Some(1));
        assert!(z.windows(2).all(|w| w[0].start <= w[1].start));
    }

    /// The list stays free of overlap after any sequence of edits.
    fn assert_no_overlap(z: &[ZoomKeyframe]) {
        assert!(
            z.windows(2).all(|w| w[0].end <= w[1].start + 1e-9),
            "overlap detected: {z:?}"
        );
    }

    #[test]
    fn insert_overlapping_clamps_into_gap() {
        // Two segments with a 1.0s gap [3,4]; request a 2.0s zoom starting at 3.5.
        let mut z = vec![kf(0.0, 3.0), kf(4.0, 6.0)];
        let i = insert_sorted(&mut z, kf(3.5, 5.5)).expect("must fit");
        assert_eq!(i, 1);
        // Shrunk to fill the gap exactly, no overlap either side.
        assert!((z[1].start - 3.0).abs() < 1e-9);
        assert!((z[1].end - 4.0).abs() < 1e-9);
        assert_no_overlap(&z);
    }

    #[test]
    fn insert_rejected_when_gap_too_small() {
        // Gap [2.0, 2.1] is narrower than MIN_LEN.
        let mut z = vec![kf(0.0, 2.0), kf(2.1, 6.0)];
        assert!(insert_sorted(&mut z, kf(2.05, 4.0)).is_none());
        assert_eq!(z.len(), 2);
    }

    #[test]
    fn insert_exact_abutment_allowed() {
        // Requesting a segment that exactly abuts the next one (end == next.start) is fine.
        let mut z = vec![kf(4.0, 6.0)];
        let i = insert_sorted(&mut z, kf(2.0, 4.0)).expect("must fit");
        assert_eq!(i, 0);
        assert!((z[0].end - z[1].start).abs() < 1e-9); // touching, not overlapping
        assert_no_overlap(&z);
    }

    #[test]
    fn resize_clamps_into_neighbor() {
        let mut z = vec![kf(0.0, 2.0), kf(3.0, 5.0)];
        // Try to drag the second segment's left edge back to 1.0 (into the first).
        assert!(resize(&mut z, 1, 1.0, 5.0, 10.0));
        assert!(z[1].start >= z[0].end - 1e-9); // stopped at the neighbour
        assert_no_overlap(&z);
    }

    #[test]
    fn resize_right_edge_stops_at_next() {
        let mut z = vec![kf(0.0, 2.0), kf(3.0, 5.0)];
        // Grow the first segment's right edge across the second — it stops at 3.0.
        assert!(resize(&mut z, 0, 0.0, 4.0, 10.0));
        assert!((z[0].end - 3.0).abs() < 1e-9);
        assert_no_overlap(&z);
    }

    #[test]
    fn move_across_neighbor_stops() {
        let mut z = vec![kf(0.0, 2.0), kf(3.0, 4.0)];
        // Try to drag the second segment (len 1) left past the first one.
        assert!(move_to(&mut z, 1, 0.5, 10.0));
        assert_no_overlap(&z);
        // It parks right after the first segment's end (2.0), not on top of it.
        let moved = z
            .iter()
            .find(|k| (k.duration() - 1.0).abs() < 1e-9)
            .unwrap();
        assert!((moved.start - 2.0).abs() < 1e-9);
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
