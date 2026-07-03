//! The auto-zoom planner: turns a timestamped input log into editable zoom segments.
//!
//! Click-driven, debounced, hold-extended, and frequency-limited — the algorithm from
//! `docs/04-Input-and-AutoZoom.md` Part B. Pure logic, no GPU/OS, fully unit-tested.
//!
//! Input events are assumed chronological (a capture log always is).

use crate::config::ZoomConfig;
use crate::event::InputEvent;
use crate::keyframe::{ZoomKeyframe, ZoomMode};
use glam::DVec2;

/// A growing cluster of nearby-in-time-and-space zoom triggers.
struct Cluster {
    first_t: f64,
    last_t: f64,
    sum: DVec2,
    count: u32,
}

impl Cluster {
    fn centroid(&self) -> DVec2 {
        self.sum / f64::from(self.count)
    }
}

/// Plan zoom segments from an input event log over a clip of `duration` seconds.
///
/// Returns editable [`ZoomKeyframe`]s in start order. An empty log (or one with no
/// clicks/drags) yields no zooms — the clip stays at 1.0.
///
/// The manual hotkey (Ctrl+Shift+Z) is an explicit **toggle**: the first press zooms in
/// (and the camera follows the cursor), the next press zooms back out, and so on — so you
/// control exactly when a product demo is zoomed. Click-to-zoom (opt-in) keeps the
/// debounced auto-clustering instead.
#[must_use]
pub fn plan_zooms(events: &[InputEvent], duration: f64, cfg: &ZoomConfig) -> Vec<ZoomKeyframe> {
    let mut segments = manual_toggle_segments(events, duration, cfg);
    if cfg.auto_zoom_on_click {
        segments.extend(click_segments(events, duration, cfg));
    }
    segments.sort_by(|a, b| a.start.total_cmp(&b.start));
    segments
}

/// Pair up manual zoom marks into in→out segments: marks 0&1 are one zoom, 2&3 the next,
/// etc. A final unpaired press holds the zoom to the end of the clip.
fn manual_toggle_segments(
    events: &[InputEvent],
    duration: f64,
    cfg: &ZoomConfig,
) -> Vec<ZoomKeyframe> {
    let mut marks: Vec<(f64, DVec2)> = events
        .iter()
        .filter(|e| e.is_zoom_mark())
        .filter_map(|e| e.pos().map(|p| (e.t(), p)))
        .collect();
    marks.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut out = Vec::new();
    let mut i = 0;
    while i < marks.len() {
        let in_t = marks[i].0;
        let out_t = marks.get(i + 1).map_or(duration, |m| m.0);
        let start = (in_t - cfg.pre_roll).max(0.0);
        let end = out_t.min(duration);
        if end > start {
            out.push(ZoomKeyframe {
                start,
                end,
                amount: cfg.amount,
                mode: ZoomMode::Auto,
                edge_snap_ratio: cfg.edge_snap_ratio,
                hl_zoom_in: None,
                hl_zoom_out: None,
            });
        }
        i += 2; // skip the matching zoom-out press
    }
    out
}

/// Click-driven auto-zoom: debounced, hold-extended, frequency-limited clustering.
fn click_segments(events: &[InputEvent], duration: f64, cfg: &ZoomConfig) -> Vec<ZoomKeyframe> {
    // 1. Gather the deliberate "points of interest" from clicks/drag-starts.
    let mut triggers: Vec<(f64, DVec2)> = events
        .iter()
        .filter(|e| e.is_click_trigger())
        .filter_map(|e| e.pos().map(|p| (e.t(), p)))
        .collect();
    triggers.sort_by(|a, b| a.0.total_cmp(&b.0));
    if triggers.is_empty() {
        return Vec::new();
    }

    // 2. Greedy spatial-temporal clustering (debounce rapid/co-located clicks).
    let mut clusters: Vec<Cluster> = Vec::new();
    for (t, pos) in triggers {
        if let Some(c) = clusters.last_mut() {
            if t - c.last_t <= cfg.merge_gap && pos.distance(c.centroid()) <= cfg.merge_radius {
                c.last_t = t;
                c.sum += pos;
                c.count += 1;
                continue;
            }
        }
        clusters.push(Cluster {
            first_t: t,
            last_t: t,
            sum: pos,
            count: 1,
        });
    }

    // 3. Cluster -> segment, extending the hold by nearby ongoing activity
    //    (drags, scrolls, typing) so a zoom does not drop mid-interaction.
    let mut segments: Vec<ZoomKeyframe> = Vec::with_capacity(clusters.len());
    for c in &clusters {
        let centroid = c.centroid();
        let mut last_activity = c.last_t;
        for e in events {
            if !e.is_activity() {
                continue;
            }
            let te = e.t();
            if te <= last_activity {
                continue;
            }
            if te > last_activity + cfg.hold {
                continue;
            }
            let near = match e.pos() {
                Some(p) => p.distance(centroid) <= cfg.merge_radius * 1.5,
                None => true, // typing sustains regardless of position
            };
            if near {
                last_activity = te;
            }
        }

        let start = (c.first_t - cfg.pre_roll).max(0.0);
        let end = (last_activity + cfg.hold).min(duration);
        if end > start {
            segments.push(ZoomKeyframe {
                start,
                end,
                amount: cfg.amount,
                mode: ZoomMode::Auto,
                edge_snap_ratio: cfg.edge_snap_ratio,
                hl_zoom_in: None,
                hl_zoom_out: None,
            });
        }
    }

    // 4. Frequency limiting: merge segments closer than the min re-zoom interval so the
    //    result never feels like motion sickness.
    let mut merged: Vec<ZoomKeyframe> = Vec::with_capacity(segments.len());
    for seg in segments {
        if let Some(prev) = merged.last_mut() {
            if seg.start < prev.end + cfg.min_rezoom_interval {
                prev.end = prev.end.max(seg.end);
                continue;
            }
        }
        merged.push(seg);
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MouseButton;

    /// Click-to-zoom is off by default now (manual hotkey is the default trigger);
    /// these click-pathway tests opt it back on.
    fn click_cfg() -> ZoomConfig {
        ZoomConfig {
            auto_zoom_on_click: true,
            ..ZoomConfig::default()
        }
    }

    fn click(t: f64, x: f64, y: f64) -> InputEvent {
        InputEvent::Click {
            t,
            pos: DVec2::new(x, y),
            button: MouseButton::Left,
        }
    }

    #[test]
    fn no_triggers_no_zooms() {
        let cfg = click_cfg();
        let events = [
            InputEvent::Move {
                t: 0.1,
                pos: DVec2::new(0.5, 0.5),
            },
            InputEvent::Move {
                t: 0.2,
                pos: DVec2::new(0.51, 0.5),
            },
        ];
        assert!(plan_zooms(&events, 5.0, &cfg).is_empty());
    }

    #[test]
    fn rapid_colocated_clicks_debounce_to_one_segment() {
        let cfg = click_cfg();
        // Three quick clicks at the same spot.
        let events = [
            click(1.0, 0.3, 0.3),
            click(1.2, 0.31, 0.29),
            click(1.4, 0.30, 0.31),
        ];
        let zooms = plan_zooms(&events, 10.0, &cfg);
        assert_eq!(zooms.len(), 1, "rapid co-located clicks must merge");
    }

    #[test]
    fn distant_clicks_in_time_make_separate_segments() {
        let cfg = click_cfg();
        // Two clicks far apart in time and space -> two zooms.
        let events = [click(1.0, 0.2, 0.2), click(8.0, 0.8, 0.8)];
        let zooms = plan_zooms(&events, 12.0, &cfg);
        assert_eq!(zooms.len(), 2);
        assert!(zooms[0].end < zooms[1].start);
    }

    #[test]
    fn close_in_time_segments_merge_for_frequency_limit() {
        let cfg = click_cfg();
        // Two clusters whose holds would overlap within min_rezoom_interval -> merged.
        let events = [click(1.0, 0.2, 0.2), click(2.2, 0.8, 0.8)];
        let zooms = plan_zooms(&events, 12.0, &cfg);
        assert_eq!(
            zooms.len(),
            1,
            "near-adjacent zooms merge to avoid seasickness"
        );
    }

    #[test]
    fn segments_respect_min_rezoom_spacing() {
        let cfg = click_cfg();
        let events = [click(1.0, 0.2, 0.2), click(8.0, 0.8, 0.8)];
        let zooms = plan_zooms(&events, 12.0, &cfg);
        for w in zooms.windows(2) {
            assert!(w[1].start >= w[0].end + cfg.min_rezoom_interval - 1e-9);
        }
    }

    #[test]
    fn pre_roll_never_goes_negative() {
        let cfg = click_cfg();
        let events = [click(0.05, 0.5, 0.5)];
        let zooms = plan_zooms(&events, 5.0, &cfg);
        assert_eq!(zooms.len(), 1);
        assert!(zooms[0].start >= 0.0);
    }

    #[test]
    fn clicks_do_not_zoom_in_manual_mode() {
        // Default config = manual: clicks alone seed nothing.
        let cfg = ZoomConfig::default();
        assert!(!cfg.auto_zoom_on_click);
        let events = [click(1.0, 0.3, 0.3), click(5.0, 0.7, 0.7)];
        assert!(plan_zooms(&events, 8.0, &cfg).is_empty());
    }

    #[test]
    fn positioned_keytype_does_not_spawn_a_zoom() {
        // A caret-positioned KeyType is steer/extend only — it must never seed a cluster,
        // even with click-to-zoom enabled (only clicks/drags/zoom-marks spawn).
        let cfg = click_cfg();
        let events = [
            InputEvent::KeyType { t: 1.0, pos: Some(DVec2::new(0.4, 0.4)) },
            InputEvent::KeyType { t: 1.5, pos: Some(DVec2::new(0.42, 0.41)) },
        ];
        assert!(plan_zooms(&events, 8.0, &cfg).is_empty());
    }

    #[test]
    fn zoom_mark_seeds_a_zoom_even_in_manual_mode() {
        let cfg = ZoomConfig::default();
        let events = [InputEvent::ZoomMark {
            t: 2.0,
            pos: DVec2::new(0.4, 0.6),
        }];
        let zooms = plan_zooms(&events, 8.0, &cfg);
        assert_eq!(zooms.len(), 1, "manual zoom mark must seed a zoom");
    }
}
