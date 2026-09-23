# Audio: microphone and system sound

How Vuoom records narration and computer sound, keeps them in sync through every edit, and
writes them into MP4. Code: `crates/vuoom-audio`, `src-tauri/src/audio.rs`,
`src/editor/audio.ts`.

## 1. Capture: WASAPI, one thread per source

Each source is a shared-mode WASAPI stream on its own thread (`vuoom_audio::Recorder`):

- **Microphone**: the chosen capture endpoint (or the system default), stored **mono**.
  Multi-channel mic arrays are averaged.
- **System sound**: the default render endpoint opened with `AUDCLNT_STREAMFLAGS_LOOPBACK`,
  stored **stereo**. Surround layouts fold the center channel into both sides.

The thread polls every 10 ms against a 1 s endpoint buffer, so a slow poll can never
overflow it. Samples arrive in the device's mix format (float32, or 16/24/32-bit PCM) and
are converted to 16-bit at the device's own rate. Resampling waits for export, where it
costs nothing at capture time.

We use WASAPI directly through the `windows` crate the workspace already builds (0.62),
rather than `cpal`, for one reason: `IAudioCaptureClient::GetBuffer` returns each packet's
**performance-counter timestamp**, the same clock the video frames and input events are
stamped with. That makes sync exact instead of estimated.

## 2. Alignment: laid onto the recording timeline

The recording's time zero is `start_qpc`. For each packet, the capture computes where its
first sample belongs (`seconds_to(qpc) × rate`) and compares that with what it has written:

| Situation | What happens |
|---|---|
| First packet of the take | Placed exactly: the gap since `start_qpc` becomes silence |
| Audio from before time zero | Dropped |
| A gap (loopback delivers nothing while the PC is silent) | Filled with silence |
| An overlap | The overlapping samples are dropped |
| Small drift (< 60 ms) | Left alone, so long takes never get periodic clicks |

Audio starts **after** the video capture, so device setup never delays frame 0; alignment
makes the late start invisible. Pausing keeps audio running: pauses become cuts, and cuts
remove sound the same way they remove frames.

## 3. Storage: crash-safe WAV next to the frames

Each take writes `mic.wav` / `system.wav` into its recovery directory, next to the frame
store, plus `audio.json` holding the anchor (`start_qpc`, counter frequency). The WAV
header's sizes are only patched when the take finishes; the reader never trusts them and
reads the data chunk to the end of the file, so a crashed take's audio is recoverable.

- **Recovery** rediscovers tracks from the files and shifts them by the gap between the
  original start and the first surviving frame (a recovered timeline starts at its first
  frame).
- **Bundles** carry the WAVs in `audio/` and copy them back on open.
- The project stores only how each track plays: `kind`, `offset`, `gain` (0 to 4), `muted`.

Cost: mono 16-bit 48 kHz narration is about 5.8 MB per minute, next to hundreds of MB of
frames.

## 4. Mixing: the edit decides what is heard

`vuoom_audio::Plan` lays the played timeline out at 48 kHz from the same segment list the
video uses (trim, then cuts, then speed regions):

- **1× spans** play their source audio (every track resampled by linear interpolation,
  gain applied, summed, clamped).
- **Sped-up spans are silent.** A 3× voice is noise, not information, and pitch-shifted
  speech reads as a mistake.
- **Cuts** are skipped entirely.
- Every audible span **fades in and out over 4 ms**, so joins never click.

Output span edges are rounded from the running output time, so the soundtrack length never
drifts from the video's, however many segments there are. Rendering works on arbitrary
frame ranges, so export can interleave it with video (unit-tested: a chunked render is
bit-identical to a single pass).

## 5. MP4: AAC through Media Foundation

The sink writer gets a second stream: AAC, 48 kHz stereo, 192 kbps, fed 16-bit PCM. After
each video frame is written, audio is rendered up to that frame's end time, so the two
streams stay interleaved and the writer never buffers minutes of one while waiting for
the other. If the OS has no usable AAC encoder, the file is written video-only rather than
failing. GIF has no audio, and the export dialog says so.

## 6. Editor: waveforms and preview playback

Each track is fetched once as WAV bytes (`audio_track`, an `ArrayBuffer` over IPC),
decoded by Web Audio, and reduced to peaks at 100 buckets per second for the timeline lane.
The lane draws what will actually be heard in teal and everything else (cuts, the
trimmed-off ends, sped-up spans, a muted track) in gray, with bar height following the
volume.

Playback follows the playhead instead of scheduling ahead, because the playhead already
encodes every edit (it jumps over cuts, races through speed-ups, loops):

- While the playhead moves at 1× through audible time, each track plays from the
  playhead's position.
- In a sped-up span the tracks stop.
- If what is heard and the playhead disagree by more than 120 ms (a seek, a cut jump, a
  loop, a rate change), the track restarts at the right spot.

Volume and mute go through `GainNode`s immediately and persist as undoable project edits.

## 7. Recording UI

- **Home, Settings**: an Audio picker (microphone off / each device, system sound).
- **Region HUD**: the same menu plus a live mic meter, fed by a short mic check that runs
  only while you frame the shot. It never runs on the home screen, so Windows' microphone
  indicator isn't lit while Vuoom sits idle.
- **Live panel**: mic and system meters while recording.

## Not yet

- Noise suppression / loudness normalization for narration.
- Waveform editing (per-span volume, ducking system sound under speech).
- Picking a specific output device for system sound (the default device is used).
