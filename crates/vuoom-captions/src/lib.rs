//! Captions for Vuoom recordings: speech to text, offline, on the user's own machine.
//!
//! - [`whisper`]: runs whisper.cpp over a narration track and returns its words with times.
//! - [`cues`]: groups those words into short, readable caption cues.
//! - [`srt`]: writes cues as a SubRip (`.srt`) file.
//! - [`MODEL`]: the speech model, downloaded once on first use.
//!
//! See `docs/18-Captions.md`.

pub mod cues;
pub mod srt;
pub mod whisper;

pub use cues::{group, Cue, Word};
pub use whisper::{cpu_supported, to_16k_mono, transcribe, Transcript, CANCELLED, SAMPLE_RATE};

/// A speech model file and where to fetch it.
#[derive(Debug, Clone, Copy)]
pub struct Model {
    /// File name in the app's model folder.
    pub file: &'static str,
    /// Download address, pinned to one revision so the file can't change under us.
    pub url: &'static str,
    /// Size in bytes, for progress and a first check of the download.
    pub bytes: u64,
    /// SHA-256 of the file (lowercase hex), checked after the download.
    pub sha256: &'static str,
}

/// whisper "base", multilingual, quantized to 5 bits: 57 MB, close to the full-size base
/// model's accuracy, and quick enough on a laptop processor.
pub const MODEL: Model = Model {
    file: "ggml-base-q5_1.bin",
    url: concat!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/",
        "5359861c739e955e79d9a303bcbc70fb988958b1/ggml-base-q5_1.bin"
    ),
    bytes: 59_707_625,
    sha256: "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898",
};
