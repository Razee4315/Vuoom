# Voice clean-up: noise removal and even volume

Most narration is recorded on a laptop mic in a room with a fan, a hum, or a keyboard, and
people drift closer to and further from the mic as they talk. Two switches on the
microphone track (Clip > Audio) fix that after the fact:

- **Remove noise**: filters out fans, hum and hiss while keeping the voice.
- **Even out volume**: brings quiet and loud passages to one steady level.

Code: `crates/vuoom-audio/src/clean.rs` (the processing), `src-tauri/src/audio.rs`
(`track_pcm`: caching, and use in preview and export).

## Offline, on the whole track

Clean-up never runs while recording. The recorded `mic.wav` is kept exactly as captured,
and the processed version is made on demand the first time the editor or an export asks
for it, then cached beside the recording under a name that says what was done to it:
`mic.denoise.wav`, `mic.level.wav`, `mic.denoise-level.wav`. Turning an option off plays the
original again instantly; turning it back on reuses the cache. A cache is used only if it is
newer than the recording, and it is written to a `.part` file and renamed, so a crash
mid-write never leaves a half-file that passes for a cache. Saved bundles carry the cached
copy along with the recording.

Working offline means both stages see the whole take, including what comes next, so
neither has to lag behind the voice or guess.

The switches are project settings (`AudioTrack.denoise`, `AudioTrack.level`, both off by
default and absent from older manifests). Each toggle is its own undo step; undo and redo
re-fetch the track, and the previous audio keeps playing until the new one is ready. The
Inspector shows "Cleaning up the voice…" meanwhile.

Output is always 48 kHz mono covering the same span as the recording, which the mixer
resamples like any other track.

## Noise removal: RNNoise

[RNNoise](https://jmvalin.ca/demo/rnnoise/) is a small recurrent network that estimates, a
hundred times a second, how much of each of 22 frequency bands is voice, and turns the rest
down. It was trained on a wide range of real noises, it keeps the
voice natural (it works on band gains plus a pitch filter, not by synthesizing speech), and
it is fast: far faster than real time on one core. We use `nnnoiseless`, a pure-Rust port
(BSD-3-Clause, no native build step), with default features off so only the library comes
in.

Details that matter for sync:

- It works on 48 kHz mono in frames of 480 samples (10 ms), with samples as `f32` in 16-bit
  range. The track is mixed down and resampled first.
- Each output frame comes out **one frame late** (the analysis window spans the previous
  frame). The first output frame is dropped and one frame of silence is pushed at the end, so
  the output lines up sample for sample with the input and has the same length.
- A CI test measures this: a chirp far below RNNoise's silence threshold skips the network
  entirely, so the output is the input through analysis and synthesis alone. Its
  cross-correlation with the input must peak at lag 0. A second test checks that steady
  noise comes out at least 6 dB quieter.

## Even volume: a speech-gated leveller

1. **Measure** the level of every 10 ms block (dBFS).
2. **Find speech.** The noise floor is the 10th percentile of block levels; blocks 10 dB above
   it (and above -50 dBFS) count as speech. Audio with no speech at all (silence, or steady
   noise) is left as it is.
3. **Loudness around each moment** is the average level, in dB, of the speech blocks within
   ±0.6 s. Averaging dB rather than energy means one loud click (a keyboard near the mic)
   barely moves it.
4. **Gain** brings that to -22 dB (a passage's overall RMS lands around -18 dBFS, a
   comfortable level for narration), within +24 / -12 dB. Stretches with no speech nearby
   keep the gain of the speech before them, so pauses are never pumped up on their own.
5. **Ease** the gain with a ±0.2 s moving average, and interpolate it per sample.
6. **Limit.** A look-ahead limiter holds every sample under -1 dBFS: the gain dips over the
   5 ms before a peak (a smooth ramp, so no click) and recovers over about 80 ms.

When both are on, noise removal runs first, so the leveller measures the voice rather than
the room.

## Not yet

- Showing the cleaned waveform next to the original, or an A/B button.
- Clean-up for system sound (its content is usually already mixed and mastered).
- Ducking system sound under speech.
