# 18 · Captions

Captions turn the narration into on-screen text, burned into the export and also saved as
an `.srt` file for YouTube, LinkedIn and video players. Everything runs offline on the
user's machine: no account, no upload.

## Speech to text: whisper.cpp

`crates/vuoom-captions` wraps [whisper.cpp](https://github.com/ggerganov/whisper.cpp) (MIT)
through `whisper-rs` (Unlicense), which builds the C++ library from source with CMake.

- **Model:** whisper *base*, multilingual, quantized to 5 bits (`ggml-base-q5_1.bin`,
  57 MB). It is close to the full-size base model's accuracy at 40% of its size, detects
  the spoken language itself, and runs well faster than real time on a laptop processor.
  It is not shipped in the installer: the app downloads it once, on the first "Generate
  captions", from a URL pinned to one Hugging Face revision, and checks its SHA-256
  (`vuoom_captions::MODEL`).
- **Input:** the microphone track (after voice clean-up when that is on), mixed to mono and
  brought to 16 kHz with a box filter (`to_16k_mono`).
- **Words, not segments:** whisper runs with `max_len = 1` and `split_on_word`, whisper.cpp's
  own way to get a time for every word. Greedy decoding (`best_of = 1`) keeps it quick.
- **Hallucinations:** on silence whisper sometimes "hears" stock phrases ("Thank you.").
  Any word whose audio is quieter than about -50 dBFS RMS is dropped.
- **Progress and cancel:** whisper's progress callback feeds the UI; its abort callback
  checks a flag, so a long transcription can be stopped at once.

### Portable builds

whisper.cpp defaults to `GGML_NATIVE=ON`, which compiles for the build machine's own
processor. On a CI runner that could mean AVX-512, and the app would crash on most users'
PCs. Both workflows set `GGML_NATIVE=OFF` (the build script forwards `GGML_*` variables to
CMake), which targets a fixed baseline: AVX2 with FMA, F16C and BMI2 (Intel since 2013,
AMD since 2015). `cpu_supported()` checks those at runtime before any whisper code runs, so
an older processor gets "captions need a newer processor" instead of a crash.

## Cues

`cues::group` turns words into caption cues the way subtitle guidelines suggest:

- a line holds at most 42 characters and covers at most 5 seconds of speech;
- a sentence end (`.`, `?`, `!`) or a pause of 0.6 s or more starts a new cue;
- punctuation and word pieces (no leading space) always stay with their word;
- a short cue stays up for at least 1 second, unless the next cue starts sooner.

`srt::srt` writes cues as SubRip (`HH:MM:SS,mmm`).
