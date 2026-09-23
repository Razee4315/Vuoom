//! Audio for Vuoom recordings: microphone and system sound.
//!
//! - [`capture`]: WASAPI capture (a microphone, or loopback of the speakers), laid onto the
//!   recording timeline by the same performance-counter clock the frames use.
//! - [`wav`]: 16-bit PCM WAV storage that survives a crash mid-take.
//! - [`mix`]: the edit-aware mixer shared by preview and MP4 export (cuts skipped, sped-up
//!   spans silent, fades at every join).
//!
//! See `docs/14-Audio.md`.

pub mod capture;
pub mod mix;
pub mod wav;

pub use capture::{has_output, list_inputs, Anchor, Device, Recorder, Source};
pub use mix::{MixTrack, Plan, Segment};
pub use wav::{Pcm, WavWriter};

/// Sample rate of mixed output (MP4 audio).
pub const OUTPUT_RATE: u32 = 48_000;
