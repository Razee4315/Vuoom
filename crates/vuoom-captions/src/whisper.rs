//! Speech to text with whisper.cpp, entirely offline.
//!
//! The narration is mixed to mono and brought to 16 kHz ([`to_16k_mono`]), then whisper
//! transcribes it word by word (`max_len = 1` with `split_on_word`, whisper.cpp's own way of
//! getting per-word times). Words over silence are dropped: whisper sometimes "hears" stock
//! phrases ("Thank you.") where nobody spoke.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use vuoom_audio::Pcm;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::cues::Word;

/// The rate whisper listens at.
pub const SAMPLE_RATE: u32 = 16_000;

/// The error [`transcribe`] returns when it was cancelled.
pub const CANCELLED: &str = "cancelled";

/// Words quieter than this RMS (full scale 1.0, about -50 dBFS) are taken as silence.
const SILENCE_RMS: f32 = 0.003;

/// What whisper heard.
#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    /// The words, in order, timed from the start of the audio.
    pub words: Vec<Word>,
    /// The language it detected, as an ISO 639-1 code such as `"en"` (empty if unknown).
    pub language: String,
}

/// Whether this processor can run the speech model. whisper.cpp is built for AVX2 with FMA,
/// F16C and BMI2 (Intel since 2013, AMD since 2015); on anything older captions are simply
/// unavailable, rather than the app crashing on an unknown instruction.
#[cfg(target_arch = "x86_64")]
#[must_use]
pub fn cpu_supported() -> bool {
    is_x86_feature_detected!("avx2")
        && is_x86_feature_detected!("fma")
        && is_x86_feature_detected!("f16c")
        && is_x86_feature_detected!("bmi2")
}

/// Whether this processor can run the speech model.
#[cfg(not(target_arch = "x86_64"))]
#[must_use]
pub fn cpu_supported() -> bool {
    true
}

/// Mix to mono, bring to 16 kHz and scale to -1..1, as whisper expects. Downsampling
/// averages each output sample's span of input (a box filter), which keeps the speech band
/// clean enough for recognition.
#[must_use]
pub fn to_16k_mono(pcm: &Pcm) -> Vec<f32> {
    let ch = usize::from(pcm.channels.max(1));
    let scale = ch as f32 * 32_768.0;
    let mono: Vec<f32> = pcm
        .samples
        .chunks_exact(ch)
        .map(|f| f.iter().map(|&s| f32::from(s)).sum::<f32>() / scale)
        .collect();
    if pcm.rate == SAMPLE_RATE || pcm.rate == 0 || mono.is_empty() {
        return mono;
    }
    let step = f64::from(pcm.rate) / f64::from(SAMPLE_RATE);
    let n = (mono.len() as f64 / step).floor() as usize;
    (0..n)
        .map(|i| {
            let a = (i as f64 * step) as usize;
            let b = (((i + 1) as f64 * step) as usize).clamp(a + 1, mono.len());
            mono[a..b].iter().sum::<f32>() / (b - a) as f32
        })
        .collect()
}

/// Transcribe 16 kHz mono speech with the model at `model`. `language` is an ISO 639-1 code,
/// or `None` to detect it. `progress` receives 0..=100 as whisper works through the audio;
/// setting `cancel` stops it early, and the result is then `Err(CANCELLED)`.
pub fn transcribe(
    model: &Path,
    audio: &[f32],
    language: Option<&str>,
    progress: impl FnMut(i32) + 'static,
    cancel: Arc<AtomicBool>,
) -> Result<Transcript, String> {
    if !cpu_supported() {
        return Err("Captions need a newer processor (with AVX2, most PCs from 2015 on)".into());
    }
    whisper_rs::install_logging_hooks();
    let ctx = WhisperContext::new_with_params(model, WhisperContextParameters::default())
        .map_err(|e| format!("couldn't load the speech model: {e}"))?;
    let mut state = ctx.create_state().map_err(|e| e.to_string())?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(threads());
    params.set_language(Some(language.unwrap_or("auto")));
    params.set_token_timestamps(true);
    params.set_max_len(1);
    params.set_split_on_word(true);
    params.set_suppress_nst(true);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    // Both callbacks are passed as boxed trait objects: that is the type whisper-rs's
    // trampolines read their user data as.
    let on_progress: Box<dyn FnMut(i32)> = Box::new(progress);
    params.set_progress_callback_safe::<_, Box<dyn FnMut(i32)>>(Some(on_progress));
    let flag = Arc::clone(&cancel);
    let stop: Box<dyn FnMut() -> bool> = Box::new(move || flag.load(Ordering::Relaxed));
    params.set_abort_callback_safe::<_, Box<dyn FnMut() -> bool>>(Some(stop));

    let run = state.full(params, audio);
    if cancel.load(Ordering::Relaxed) {
        return Err(CANCELLED.into());
    }
    run.map_err(|e| format!("speech recognition failed: {e}"))?;

    let mut words = Vec::new();
    for seg in state.as_iter() {
        let Ok(text) = seg.to_str_lossy() else {
            continue;
        };
        // Segment times are in centiseconds.
        let start = seg.start_timestamp() as f64 / 100.0;
        let end = seg.end_timestamp() as f64 / 100.0;
        words.push(Word {
            text: text.into_owned(),
            start,
            end,
        });
    }
    words.retain(|w| audible(audio, w));
    let id = state.full_lang_id_from_state();
    let language = whisper_rs::get_lang_str(id).unwrap_or("").to_string();
    Ok(Transcript { words, language })
}

/// Threads for whisper: all cores up to 8 (more barely helps and starves the UI).
fn threads() -> i32 {
    let n = std::thread::available_parallelism();
    n.map_or(4, usize::from).clamp(1, 8) as i32
}

/// Whether a word's stretch of audio carries any sound (see [`SILENCE_RMS`]).
fn audible(audio: &[f32], w: &Word) -> bool {
    let rate = f64::from(SAMPLE_RATE);
    let a = ((w.start * rate) as usize).min(audio.len());
    let b = ((w.end * rate) as usize).clamp(a, audio.len());
    if b == a {
        return true;
    }
    let power = audio[a..b].iter().map(|s| s * s).sum::<f32>() / (b - a) as f32;
    power.sqrt() >= SILENCE_RMS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_48k_becomes_mono_16k() {
        // One second of a 48 kHz stereo ramp, left and right opposite in sign but for a
        // constant offset, so the mono mix is that offset.
        let mut samples = Vec::new();
        for i in 0..48_000_i32 {
            let v = (i % 1000) as i16;
            samples.extend([v + 8192, 8192 - v]);
        }
        let pcm = Pcm {
            rate: 48_000,
            channels: 2,
            samples,
        };
        let out = to_16k_mono(&pcm);
        assert_eq!(out.len(), 16_000);
        for s in out {
            assert!((s - 0.25).abs() < 1e-6, "{s}");
        }
    }

    #[test]
    fn silence_is_not_a_word() {
        let mut audio = vec![0.0_f32; 32_000];
        for (i, s) in audio[16_000..].iter_mut().enumerate() {
            *s = 0.1 * (i as f32 * 0.3).sin();
        }
        let word = |start: f64, end: f64| Word {
            text: " Thanks".into(),
            start,
            end,
        };
        assert!(!audible(&audio, &word(0.2, 0.8)));
        assert!(audible(&audio, &word(1.2, 1.8)));
        // A word with no length can't be judged, so it is kept.
        assert!(audible(&audio, &word(0.5, 0.5)));
    }

    #[test]
    fn a_missing_model_is_an_error_not_a_crash() {
        let r = transcribe(
            Path::new("no-such-model.bin"),
            &[0.0; 16_000],
            None,
            |_| {},
            Arc::default(),
        );
        assert!(r.is_err());
    }
}
