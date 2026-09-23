# Recording pipeline: capture, storage, saving, encoding

How a take gets from the screen to disk, into a project, and out as an MP4, and why each
stage is built the way it is. Numbers are for a 1920×1080 capture unless noted.

## 1. Capture: pace at the compositor

Windows Graphics Capture (WGC) delivers a frame on every DWM composition unless told
otherwise: 60, 144 or 240 per second depending on the display. Each delivery costs a
GPU→CPU copy of the whole frame, so a 144 Hz monitor used to push ~1.2 GB/s through the
capture callback, most of it identical frames we then discarded.

Vuoom sets `MinUpdateInterval` on the capture session so the compositor itself coalesces
updates and hands over the **latest** content once the interval passes; nothing visible is
dropped. Measured throttling undershoots (an interval of 1/60 s yields about 45 fps, see
[windows-capture#190](https://github.com/NiiightmareXD/windows-capture/issues/190)), so the
interval is set 1.5× tighter than the target. The target is a user setting (24 / 30 / 60
fps, `set_capture_fps`); the live preview runs its own capture capped at 30 fps. Windows 10
lacks the API, where capture stays uncapped as before.

We deliberately do **not** drop frames in software after they arrive: WGC only delivers on
change, so dropping an early frame could lose the last visible state of a static screen.

## 2. Storage: compressed deltas, streamed to disk

Raw BGRA is 8.3 MB per frame, ~250 MB/s at 30 fps and ~500 MB/s at 60 fps. Screen content
is extremely redundant: flat UI, and between two frames usually only a caret, a hover state
or a scrolled panel changes. The frame store (`src-tauri/src/frame_store.rs`) exploits that:

- **Duplicate frames** are a 28-byte index record and no pixel I/O.
- **Delta frames** are the XOR of the frame against the previous one. Unchanged pixels
  become zero bytes, which LZ4 compresses to almost nothing.
- **Keyframes** (every 30 frames, or on a size change) are LZ4 of the pixels themselves, so
  a random seek replays at most 30 deltas.
- Frames are cut into horizontal strips that are compressed and decompressed in parallel on
  rayon's pool, keeping a 4K frame well inside a 60 fps budget. LZ4 compresses at
  >500 MB/s per core and decodes at GB/s ([lz4.org](https://lz4.org/),
  [lz4_flex](https://github.com/pseitz/lz4_flex), pure Rust, no unsafe by default).
- When compression would not save bytes (tiny or noisy frames) the frame is stored raw.

The encoding rides in the spare top three bits of each index record's length field, so
stores written by older builds (raw BGRA) still open unchanged. Crash recovery is intact:
pixels and index are appended incrementally, and `FrameStore::open` trims to the last frame
whose bytes fully reached disk (and never starts on a delta).

Sequential access (playback, export) decodes one delta per frame on top of the cached
previous frame. Typical screencasts shrink by one to two orders of magnitude; the disk-space
guard still plans for a pessimistic 4× ratio because full-motion content (a video playing
inside the recording) compresses far less.

## 3. Saving: copy, don't re-encode

Project bundles used to hold one PNG per frame: minutes of deflate for a long take and
gigabytes on disk, with every duplicate frame written again. A `.vuoom` bundle now holds
the compressed frame store itself (`frames/frames.raw` + `frames/index.bin`), copied
byte-for-byte, with timestamps rebased to a portable 10 MHz timebase (the QPC epoch and
frequency are machine-specific). Saving is a file copy; opening is a file copy into the
scratch store. Old PNG bundles still open, and saving over one removes its PNGs.

## 4. MP4 export: let the GPU encode

The Media Foundation sink writer only considers hardware encoders (NVENC, Quick Sync, AMF)
when `MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS` is set at creation
([docs](https://learn.microsoft.com/en-us/windows/win32/medfound/mf-readwrite-enable-hardware-transforms)).
Vuoom sets it and also disables sink-writer throttling (export is offline, frames should be
accepted as fast as we composite them). If a vendor MFT rejects the configuration, the
writer is recreated with the software encoder.

## Future work

- Feed the encoder D3D11 textures straight from the compositor (zero-copy) instead of
  reading RGBA back to the CPU.
- Use WGC dirty regions (Windows 11 24H2+) to skip XOR work on untouched strips.
- Rate control via `ICodecAPI` (constant quality instead of average bitrate).
