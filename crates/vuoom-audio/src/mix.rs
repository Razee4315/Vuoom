//! Edit-aware mixing: recorded tracks (source time) → the played timeline (output time).
//!
//! The editor's timeline is a list of source spans, each played at a speed factor (a cut is
//! an infinite factor). Audio follows it: 1× spans play their source audio, sped-up spans are
//! silent (a 3× voice is noise, not information), and cuts are skipped. Every audible span
//! fades in and out over a few milliseconds so the joins never click. Rendering works in
//! chunks, so MP4 export can interleave audio with video without holding the whole mix.

use crate::wav::Pcm;

/// A contiguous span of source time played at one speed. `factor` is `f64::INFINITY` for a
/// cut (zero output length).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub src_start: f64,
    pub src_end: f64,
    pub factor: f64,
}

/// One input to the mix.
#[derive(Debug, Clone, Copy)]
pub struct MixTrack<'a> {
    pub pcm: &'a Pcm,
    /// Linear gain (1.0 = as recorded).
    pub gain: f32,
    /// Source time, in seconds, of the track's first sample.
    pub offset: f64,
}

/// Fade applied at both ends of every audible span.
pub const FADE_SECS: f64 = 0.004;

#[derive(Debug, Clone, Copy)]
struct Span {
    out_start: u64,
    out_end: u64,
    src_start: f64,
    audible: bool,
}

/// The output-frame layout of a segment list at a sample rate.
#[derive(Debug, Clone)]
pub struct Plan {
    spans: Vec<Span>,
    rate: u32,
    frames: u64,
}

impl Plan {
    /// Lay `segments` (in playback order) out on an output timeline sampled at `rate`.
    /// Span edges are rounded from the running output time, so the total never drifts
    /// from `output_duration × rate` however many segments there are.
    #[must_use]
    pub fn new(segments: &[Segment], rate: u32) -> Self {
        let r = f64::from(rate.max(1));
        let mut spans = Vec::new();
        let mut acc = 0.0;
        for s in segments {
            if !s.factor.is_finite() || s.factor <= 0.0 || s.src_end <= s.src_start {
                continue;
            }
            let a = (acc * r).round() as u64;
            acc += (s.src_end - s.src_start) / s.factor;
            let b = (acc * r).round() as u64;
            if b > a {
                spans.push(Span {
                    out_start: a,
                    out_end: b,
                    src_start: s.src_start,
                    audible: (s.factor - 1.0).abs() < 1e-6,
                });
            }
        }
        let frames = spans.last().map_or(0, |s| s.out_end);
        Self {
            spans,
            rate: rate.max(1),
            frames,
        }
    }

    /// Total output frames.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.frames
    }

    #[must_use]
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Render output frames `[start, start + n)` as interleaved stereo 16-bit samples.
    /// Frames past the end of the plan are silent.
    #[must_use]
    pub fn render(&self, tracks: &[MixTrack<'_>], start: u64, n: usize) -> Vec<i16> {
        let mut out = vec![0i16; n * 2];
        let end = start + n as u64;
        let r = f64::from(self.rate);
        let fade = ((FADE_SECS * r) as u64).max(1);
        let first = self.spans.partition_point(|s| s.out_end <= start);
        for sp in self.spans[first..].iter().take_while(|s| s.out_start < end) {
            if !sp.audible {
                continue;
            }
            for f in sp.out_start.max(start)..sp.out_end.min(end) {
                let src_t = sp.src_start + (f - sp.out_start) as f64 / r;
                let edge = (f - sp.out_start).min(sp.out_end - 1 - f);
                let env = if edge < fade {
                    (edge as f32 + 0.5) / fade as f32
                } else {
                    1.0
                };
                let (mut l, mut rr) = (0.0f32, 0.0f32);
                for t in tracks {
                    let (tl, tr) = sample_stereo(t.pcm, src_t - t.offset);
                    l += tl * t.gain;
                    rr += tr * t.gain;
                }
                let i = (f - start) as usize * 2;
                out[i] = to_i16(l * env);
                out[i + 1] = to_i16(rr * env);
            }
        }
        out
    }
}

/// Convert a normalized sample to 16-bit, clamping overs.
#[must_use]
pub fn to_i16(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

/// The track's stereo value at time `t` seconds from its first sample (linear
/// interpolation, which also resamples to any output rate). Silence outside the track.
/// Mono plays on both sides; channels beyond two are ignored.
#[must_use]
pub fn sample_stereo(pcm: &Pcm, t: f64) -> (f32, f32) {
    let frames = pcm.frames();
    if frames == 0 || t < 0.0 {
        return (0.0, 0.0);
    }
    let p = t * f64::from(pcm.rate);
    let i = p.floor() as usize;
    if i >= frames {
        return (0.0, 0.0);
    }
    let frac = (p - i as f64) as f32;
    let ch = usize::from(pcm.channels);
    let at = |frame: usize, c: usize| f32::from(pcm.samples[frame * ch + c]) / 32768.0;
    let lerp = |c: usize| {
        let a = at(i, c);
        if i + 1 < frames {
            a + (at(i + 1, c) - a) * frac
        } else {
            a
        }
    };
    let l = lerp(0);
    let r = if ch > 1 { lerp(1) } else { l };
    (l, r)
}

/// Peak absolute level (0..1) of interleaved samples, for level meters.
#[must_use]
pub fn peak(samples: &[i16]) -> f32 {
    samples
        .iter()
        .map(|s| s.unsigned_abs())
        .max()
        .map_or(0.0, |m| f32::from(m) / 32768.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(src_start: f64, src_end: f64, factor: f64) -> Segment {
        Segment {
            src_start,
            src_end,
            factor,
        }
    }

    fn mono(rate: u32, samples: Vec<i16>) -> Pcm {
        Pcm {
            rate,
            channels: 1,
            samples,
        }
    }

    fn track(pcm: &Pcm) -> MixTrack<'_> {
        MixTrack {
            pcm,
            gain: 1.0,
            offset: 0.0,
        }
    }

    /// Source sample `i` of a ramp is `i`, so an output value names the source frame.
    fn ramp(rate: u32, secs: u32) -> Pcm {
        mono(rate, (0..rate * secs).map(|i| (i % 30_000) as i16).collect())
    }

    fn left(out: &[i16], frame: usize) -> i16 {
        out[frame * 2]
    }

    #[test]
    fn plain_clip_plays_its_source_with_edge_fades() {
        let pcm = ramp(1_000, 2);
        let plan = Plan::new(&[seg(0.0, 2.0, 1.0)], 1_000);
        assert_eq!(plan.frames(), 2_000);
        let out = plan.render(&[track(&pcm)], 0, 2_000);
        // Interior samples pass through (within rounding), on both channels.
        assert!((i32::from(left(&out, 500)) - 500).abs() <= 1);
        assert_eq!(out[1000], out[1001]);
        // The first sample is faded down.
        assert!(left(&out, 0).abs() <= 1);
    }

    #[test]
    fn cuts_are_skipped() {
        let pcm = ramp(1_000, 3);
        let segs = [
            seg(0.0, 1.0, 1.0),
            seg(1.0, 2.0, f64::INFINITY),
            seg(2.0, 3.0, 1.0),
        ];
        let plan = Plan::new(&segs, 1_000);
        assert_eq!(plan.frames(), 2_000);
        let out = plan.render(&[track(&pcm)], 0, 2_000);
        // Output 1.5 s is source 2.5 s.
        assert!((i32::from(left(&out, 1_500)) - 2_500).abs() <= 1);
    }

    #[test]
    fn sped_up_spans_are_silent_but_keep_their_length() {
        let pcm = ramp(1_000, 3);
        let plan = Plan::new(&[seg(0.0, 1.0, 1.0), seg(1.0, 3.0, 2.0)], 1_000);
        assert_eq!(plan.frames(), 2_000);
        let out = plan.render(&[track(&pcm)], 0, 2_000);
        assert!(left(&out, 500) > 400);
        assert!(out[2_000..].iter().all(|&s| s == 0));
    }

    #[test]
    fn resamples_by_interpolation() {
        // 1 kHz source, 2 kHz output: odd output frames land between source frames.
        let pcm = mono(1_000, (0..100).map(|i| i * 100).collect());
        let plan = Plan::new(&[seg(0.0, 0.1, 1.0)], 2_000);
        let out = plan.render(&[track(&pcm)], 0, 200);
        // Output frame 51 is source frame 25.5 → 2550.
        let v = i32::from(left(&out, 51));
        assert!((v - 2_550).abs() <= 2, "{v}");
    }

    #[test]
    fn offset_and_gain_apply() {
        let pcm = ramp(1_000, 2);
        let t = MixTrack {
            pcm: &pcm,
            gain: 0.5,
            // The track started 0.5 s before source time zero.
            offset: -0.5,
        };
        let plan = Plan::new(&[seg(0.0, 1.0, 1.0)], 1_000);
        let out = plan.render(&[t], 0, 1_000);
        // Output 0.2 s → source 0.2 s → track 0.7 s = 700, at half gain.
        assert!((i32::from(left(&out, 200)) - 350).abs() <= 1);
    }

    #[test]
    fn tracks_sum_and_clamp() {
        let loud = mono(1_000, vec![30_000; 1_000]);
        let plan = Plan::new(&[seg(0.0, 1.0, 1.0)], 1_000);
        let out = plan.render(&[track(&loud), track(&loud)], 0, 1_000);
        assert_eq!(left(&out, 500), 32_767);
    }

    #[test]
    fn chunked_render_matches_one_pass() {
        let pcm = ramp(8_000, 2);
        let segs = [seg(0.0, 0.7, 1.0), seg(0.7, 1.1, 3.0), seg(1.1, 2.0, 1.0)];
        let plan = Plan::new(&segs, 48_000);
        let n = plan.frames() as usize;
        let whole = plan.render(&[track(&pcm)], 0, n);
        let mut parts = Vec::new();
        let mut at = 0u64;
        while (at as usize) < n {
            let len = 1_601.min(n - at as usize);
            parts.extend(plan.render(&[track(&pcm)], at, len));
            at += len as u64;
        }
        assert_eq!(whole, parts);
    }

    #[test]
    fn past_the_end_is_silence() {
        let pcm = ramp(1_000, 1);
        let plan = Plan::new(&[seg(0.0, 1.0, 1.0)], 1_000);
        let out = plan.render(&[track(&pcm)], 900, 200);
        assert!(out[200..].iter().all(|&s| s == 0));
    }

    #[test]
    fn peak_reads_the_loudest_sample() {
        assert_eq!(peak(&[]), 0.0);
        assert!((peak(&[100, -16_384, 20]) - 0.5).abs() < 1e-6);
    }
}
