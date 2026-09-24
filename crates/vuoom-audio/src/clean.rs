//! Voice clean-up for narration: noise removal and even volume.
//!
//! Both run offline on a whole recorded track (the editor applies them on demand and caches
//! the result next to the original), so they can look ahead and never pump or lag:
//!
//! - [`denoise`]: RNNoise (via `nnnoiseless`), a small recurrent network that suppresses
//!   background noise (fans, hum, hiss, keyboard clatter) while keeping speech. It works on
//!   48 kHz mono in 10 ms frames and delays its output by one frame; that delay is removed
//!   here, so the voice stays in sync with the picture.
//! - [`level`]: a speech-gated leveller. It measures loudness only where someone is talking,
//!   brings quiet and loud passages toward one level, and leaves pauses alone, so room noise
//!   is never pumped up. A look-ahead limiter then holds peaks under -1 dBFS.
//!
//! Samples are `f32` in 16-bit range throughout (what RNNoise expects).

use std::collections::VecDeque;

use nnnoiseless::DenoiseState;

use crate::wav::Pcm;

/// The rate clean-up works at (RNNoise's native rate).
pub const RATE: u32 = 48_000;
/// RNNoise's frame: 10 ms at 48 kHz.
const FRAME: usize = DenoiseState::FRAME_SIZE;

/// Speech is brought to this typical level: the average, in dB, of its 10 ms blocks. A
/// passage's overall RMS runs a few dB above that (about -18 dBFS for normal speech).
pub const TARGET_DB: f32 = -22.0;
/// Peaks are held under this (dBFS).
pub const CEILING_DB: f32 = -1.0;
/// The most the leveller boosts a quiet passage, and cuts a loud one (dB).
const MAX_BOOST_DB: f32 = 24.0;
const MAX_CUT_DB: f32 = 12.0;
/// Loudness is measured in 10 ms blocks...
const BLOCK: usize = 480;
/// ...averaged over the speech within ±0.6 s of each block...
const WINDOW_BLOCKS: usize = 60;
/// ...and the resulting gain is eased over ±0.2 s so it never steps.
const EASE_BLOCKS: usize = 20;
/// Blocks this far above the noise floor count as speech (dB), but never below
/// [`GATE_MIN_DB`].
const GATE_ABOVE_FLOOR_DB: f32 = 10.0;
const GATE_MIN_DB: f32 = -50.0;
/// The limiter's look-ahead (5 ms) and release time.
const LOOK: usize = 240;
const RELEASE_SECS: f32 = 0.08;

/// Full scale of 16-bit samples.
const FULL: f32 = 32_768.0;

/// Clean up a recorded voice: optionally remove noise, optionally even out its volume.
/// Returns 48 kHz mono covering the same time as `pcm` (same start, same length).
#[must_use]
pub fn clean(pcm: &Pcm, remove_noise: bool, even_volume: bool) -> Pcm {
    let mut x = mono_48k(pcm);
    if remove_noise {
        x = denoise(&x);
    }
    if even_volume {
        level(&mut x);
    }
    Pcm {
        rate: RATE,
        channels: 1,
        samples: x
            .iter()
            .map(|s| s.round().clamp(-FULL, FULL - 1.0) as i16)
            .collect(),
    }
}

/// Mix down to mono and resample to [`RATE`] (linear interpolation, as the mixer does).
fn mono_48k(pcm: &Pcm) -> Vec<f32> {
    let ch = usize::from(pcm.channels.max(1));
    let mono: Vec<f32> = pcm
        .samples
        .chunks_exact(ch)
        .map(|f| f.iter().map(|&s| f32::from(s)).sum::<f32>() / ch as f32)
        .collect();
    if pcm.rate == RATE || pcm.rate == 0 || mono.is_empty() {
        return mono;
    }
    let step = f64::from(pcm.rate) / f64::from(RATE);
    let n = (mono.len() as f64 / step).floor() as usize;
    (0..n)
        .map(|i| {
            let p = i as f64 * step;
            let j = p as usize;
            let a = mono[j];
            let b = mono.get(j + 1).copied().unwrap_or(a);
            a + (b - a) * (p - j as f64) as f32
        })
        .collect()
}

/// Remove background noise from 48 kHz mono samples. The output has the same length and
/// lines up sample for sample with the input.
#[must_use]
pub fn denoise(input: &[f32]) -> Vec<f32> {
    let mut state = DenoiseState::new();
    let mut out = Vec::with_capacity(input.len() + 2 * FRAME);
    let mut frame_in = [0.0f32; FRAME];
    let mut frame_out = [0.0f32; FRAME];
    // RNNoise hands back each frame one frame late: the first output is only its warm-up,
    // and one extra frame of silence flushes the last real samples through.
    let frames = input.len().div_ceil(FRAME) + 1;
    for k in 0..frames {
        let start = (k * FRAME).min(input.len());
        let end = (start + FRAME).min(input.len());
        frame_in.fill(0.0);
        frame_in[..end - start].copy_from_slice(&input[start..end]);
        state.process_frame(&mut frame_out, &frame_in);
        if k > 0 {
            out.extend_from_slice(&frame_out);
        }
    }
    out.truncate(input.len());
    out
}

/// Mean square of a 16-bit-range block, as dBFS.
fn block_db(block: &[f32]) -> f32 {
    let sum: f32 = block.iter().map(|&s| (s / FULL) * (s / FULL)).sum();
    10.0 * (sum / block.len() as f32).max(1e-12).log10()
}

/// Even out the volume of 48 kHz mono speech in place: speech moves toward
/// [`TARGET_DB`], pauses keep the gain of the speech around them, and peaks stay under
/// [`CEILING_DB`]. Audio with no detectable speech only passes through the limiter.
pub fn level(x: &mut [f32]) {
    let db: Vec<f32> = x.chunks(BLOCK).map(block_db).collect();
    if let Some(gains) = block_gains(&db) {
        // Each block's gain sits at its center; samples in between interpolate.
        let last = gains.len() - 1;
        for (i, s) in x.iter_mut().enumerate() {
            let p = (i as f32 + 0.5) / BLOCK as f32 - 0.5;
            let k = (p.max(0.0) as usize).min(last);
            let frac = (p - k as f32).clamp(0.0, 1.0);
            let g = gains[k] + (gains[(k + 1).min(last)] - gains[k]) * frac;
            *s *= g;
        }
    }
    limit(x, FULL * 10f32.powf(CEILING_DB / 20.0));
}

/// Linear gain per block from per-block levels (dBFS); `None` when nothing sounds like
/// speech (silence, or a steady noise with no louder passages).
fn block_gains(db: &[f32]) -> Option<Vec<f32>> {
    let n = db.len();
    let mut sorted = db.to_vec();
    sorted.sort_by(f32::total_cmp);
    let floor = *sorted.get(n / 10)?;
    let gate = (floor + GATE_ABOVE_FLOOR_DB).max(GATE_MIN_DB);

    // Running totals of speech level and speech block count, for windowed averages.
    // Averaging dB rather than energy keeps one loud click (a keyboard near the mic) from
    // pulling the gain of the whole second around it.
    let mut total = vec![0.0f64; n + 1];
    let mut count = vec![0u32; n + 1];
    for (i, &d) in db.iter().enumerate() {
        let speech = d > gate;
        total[i + 1] = total[i] + if speech { f64::from(d) } else { 0.0 };
        count[i + 1] = count[i] + u32::from(speech);
    }
    if count[n] == 0 {
        return None;
    }

    // The gain that brings the nearby speech to the target; blocks with no speech nearby
    // hold the gain of the closest speech before them (or, at the start, after them).
    let want: Vec<Option<f32>> = (0..n)
        .map(|i| {
            let a = i.saturating_sub(WINDOW_BLOCKS);
            let b = (i + WINDOW_BLOCKS + 1).min(n);
            let c = count[b] - count[a];
            (c > 0).then(|| {
                let loud = ((total[b] - total[a]) / f64::from(c)) as f32;
                (TARGET_DB - loud).clamp(-MAX_CUT_DB, MAX_BOOST_DB)
            })
        })
        .collect();
    let mut held = want.iter().find_map(|g| *g)?;
    let want: Vec<f32> = want
        .into_iter()
        .map(|g| {
            held = g.unwrap_or(held);
            held
        })
        .collect();

    // Ease the gain with a centered moving average, then convert dB to linear.
    let mut sum = vec![0.0f64; n + 1];
    for (i, &g) in want.iter().enumerate() {
        sum[i + 1] = sum[i] + f64::from(g);
    }
    Some(
        (0..n)
            .map(|i| {
                let a = i.saturating_sub(EASE_BLOCKS);
                let b = (i + EASE_BLOCKS + 1).min(n);
                let g_db = ((sum[b] - sum[a]) / (b - a) as f64) as f32;
                10f32.powf(g_db / 20.0)
            })
            .collect(),
    )
}

/// Hold every sample under `ceiling` with a look-ahead limiter: the gain dips smoothly
/// over the [`LOOK`] samples before a peak (so there's no click) and recovers over about
/// [`RELEASE_SECS`].
fn limit(x: &mut [f32], ceiling: f32) {
    let n = x.len();
    let need: Vec<f32> = x
        .iter()
        .map(|s| (ceiling / s.abs().max(1e-9)).min(1.0))
        .collect();
    // ahead[i] = min(need[i..=i + LOOK]), with a monotonic queue walking backwards.
    let mut ahead = vec![1.0f32; n];
    let mut q: VecDeque<usize> = VecDeque::new();
    for i in (0..n).rev() {
        while q.back().is_some_and(|&j| need[j] >= need[i]) {
            q.pop_back();
        }
        q.push_back(i);
        while q.front().is_some_and(|&j| j > i + LOOK) {
            q.pop_front();
        }
        ahead[i] = q.front().map_or(1.0, |&j| need[j]);
    }
    // Release: recover toward unity, but never above what's needed ahead.
    let rel = (-1.0 / (RELEASE_SECS * RATE as f32)).exp();
    let mut g = 1.0f32;
    for a in &mut ahead {
        g = (1.0 - (1.0 - g) * rel).min(*a);
        *a = g;
    }
    // A trailing average over LOOK samples turns the steps into ramps. Every value it
    // averages at a peak already allows for that peak, so the peak still lands under the
    // ceiling; the final clamp only guards rounding.
    let mut acc = 0.0f64;
    for (i, s) in x.iter_mut().enumerate() {
        acc += f64::from(ahead[i]);
        if i >= LOOK {
            acc -= f64::from(ahead[i - LOOK]);
        }
        let g = (acc / (i.min(LOOK - 1) + 1) as f64) as f32;
        *s = (*s * g).clamp(-ceiling, ceiling);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic white noise in -1..1.
    fn noise(len: usize, seed: u32) -> Vec<f32> {
        let mut s = seed;
        (0..len)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                s as f32 / 2_147_483_648.0 - 1.0
            })
            .collect()
    }

    /// Speech-like: syllable bursts of a harmonic tone, 4 per second, with an RMS level
    /// near `db` dBFS.
    fn syllables(secs: f32, db: f32) -> Vec<f32> {
        let amp = FULL * 10f32.powf(db / 20.0) * 2.0;
        (0..(secs * RATE as f32) as usize)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                let env = (std::f32::consts::PI * 4.0 * t).sin().powi(2);
                let tone = (0..4)
                    .map(|h| {
                        let f = 150.0 * (h + 1) as f32;
                        (std::f32::consts::TAU * f * t).sin() / (h + 1) as f32
                    })
                    .sum::<f32>();
                amp * env * tone * 0.5
            })
            .collect()
    }

    #[test]
    fn denoise_output_lines_up_with_the_input() {
        // A chirp far below RNNoise's silence threshold skips the network entirely, so
        // what comes out is the input through analysis and synthesis alone: any offset
        // left is framing delay that clean-up failed to remove.
        let len = RATE as usize;
        let x: Vec<f32> = (0..len)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                0.002 * (std::f32::consts::TAU * (300.0 * t + 1_500.0 * t * t)).sin()
            })
            .collect();
        let y = denoise(&x);
        assert_eq!(y.len(), x.len());
        let window = 12_000..36_000;
        let corr = |lag: i64| -> f64 {
            window
                .clone()
                .map(|i| {
                    let j = (i as i64 + lag) as usize;
                    f64::from(x[i]) * f64::from(y[j])
                })
                .sum()
        };
        let lags = -960i64..=960;
        let best = lags.max_by(|&a, &b| corr(a).total_cmp(&corr(b)));
        assert_eq!(best, Some(0), "output is offset by {best:?} samples");
        let energy: f64 = window.clone().map(|i| f64::from(x[i]).powi(2)).sum();
        assert!(corr(0) > 0.9 * energy, "{} vs {energy}", corr(0));
    }

    #[test]
    fn denoise_quiets_steady_noise() {
        let x: Vec<f32> = noise(2 * RATE as usize, 3)
            .into_iter()
            .map(|s| s * 2_000.0)
            .collect();
        let y = denoise(&x);
        let tail = RATE as usize..2 * RATE as usize;
        let (before, after) = (block_db(&x[tail.clone()]), block_db(&y[tail]));
        assert!(after < before - 6.0, "{before} dB -> {after} dB");
    }

    #[test]
    fn level_brings_quiet_and_loud_speech_together() {
        let hush = vec![0.0f32; RATE as usize];
        let quiet = syllables(3.0, -30.0);
        let loud = syllables(3.0, -8.0);
        let mut x = [hush.clone(), quiet, hush.clone(), loud, hush].concat();
        let sec = RATE as usize;
        let mid = |x: &[f32], at: usize| block_db(&x[at * sec..(at + 1) * sec]);
        let spread_before = mid(&x, 6) - mid(&x, 2);
        level(&mut x);
        let (q, l) = (mid(&x, 2), mid(&x, 6));
        assert!(spread_before > 20.0);
        // The absolute speech gate drops more of a quiet passage's syllable troughs, so it
        // measures a little louder and ends up a few dB under the loud one; 22 dB -> ~3.
        assert!((l - q).abs() < 4.5, "quiet {q} dB, loud {l} dB");
        // A passage's RMS runs a few dB above its typical block level.
        assert!(l > TARGET_DB - 1.0 && l < TARGET_DB + 8.0, "loud {l} dB");
        // Pauses stay silent.
        assert!(x[..sec / 2].iter().all(|s| s.abs() < 1.0));
    }

    #[test]
    fn level_holds_peaks_under_the_ceiling() {
        let mut x = syllables(2.0, -24.0);
        // A sharp clap in the middle, near full scale.
        let clap = RATE as usize..RATE as usize + 200;
        for (k, s) in x[clap].iter_mut().enumerate() {
            *s += if k % 2 == 0 { 30_000.0 } else { -30_000.0 };
        }
        level(&mut x);
        let ceiling = FULL * 10f32.powf(CEILING_DB / 20.0);
        let peak = x.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak <= ceiling + 0.5, "{peak} > {ceiling}");
        assert!(peak > ceiling * 0.8, "the limiter over-reacted: {peak}");
    }

    #[test]
    fn noise_alone_is_left_at_its_level() {
        let x: Vec<f32> = noise(RATE as usize, 9)
            .into_iter()
            .map(|s| s * 300.0)
            .collect();
        let mut y = x.clone();
        level(&mut y);
        assert!((block_db(&x) - block_db(&y)).abs() < 0.1);
    }

    #[test]
    fn clean_converts_to_48k_mono_of_the_same_length() {
        let pcm = Pcm {
            rate: 16_000,
            channels: 2,
            samples: vec![1_000; 16_000 * 2],
        };
        let out = clean(&pcm, false, false);
        assert_eq!((out.rate, out.channels), (RATE, 1));
        assert_eq!(out.frames(), RATE as usize);
        assert!(out.samples.iter().all(|&s| s == 1_000));
        let both = clean(&pcm, true, true);
        assert_eq!(both.frames(), RATE as usize);
    }
}
