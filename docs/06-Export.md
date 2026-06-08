# 06 — Export: MP4 & GIF

How Vuoom turns composited frames into polished MP4 and small-but-good GIFs — license-safe,
patent-safe, and hardware-accelerated. The headline finding: **use the OS/GPU hardware encoder**
so codec patent royalties are the vendor's responsibility and no GPL codec ships in Vuoom.

---

## MP4 encoding

### Decision

| Path | Choice | Why |
|---|---|---|
| **Primary** | **Native Windows Media Foundation HW encoder** via `windows-rs` (`MFTEnumEx`, hardware-first) | Zero external libs, copyleft-safe, **patent-safe** (Windows holds the codec license), frames stay on GPU |
| **Fallback** | **`ffmpeg-next` bindings** with `h264_nvenc`/`h264_qsv`/`h264_amf`, `h264_mf` universal HW fallback, software **last** | Catches edge cases MF mishandles (some AMD setups) |

This mirrors Cap exactly: dedicated `enc-mediafoundation` (native MF) + `enc-ffmpeg` (bindings,
**not** a sidecar) crates.

### The licensing reality (read before choosing)

Two independent layers, often conflated:

1. **Software license (copyleft):** FFmpeg is LGPL by default; **x264/x265 are GPL** and
   `--enable-gpl`. Shipping x264/x265 makes **all of Vuoom GPL** (forced open-source). LGPL ffmpeg
   is shippable with a free/closed app if **dynamically linked**.
2. **Patent license (MPEG-LA / Via LA), independent of software license:** H.264/HEVC use may
   require patent royalties. **Hardware encoders sidestep this** — NVENC/QSV/AMF/Media Foundation
   royalties are paid by the GPU/OS vendor, not the app calling them.

**→ Net rule for Vuoom: ship NO x264/x265. Use HW encoders (Media Foundation / NVENC / QSV /
AMF).** They are both copyleft-safe and patent-safe. If a software fallback is ever required, use
Cisco **OpenH264** via its "download Cisco's signed binary" model — not a bundled recompile. If
you link ffmpeg, use a **BtbN `lgpl-shared`** build, never gyan.dev's GPLv3 full builds.

### Cap's encoder-selection pattern (reference)

```
// H.264 candidate order, tried until one initializes:
hardware = ["h264_videotoolbox","h264_nvenc","h264_qsv","h264_amf","h264_mf"]   // by GPU vendor
// AMD export priority specifically:
["h264_amf","h264_mf","h264_nvenc","h264_qsv","libx264"]   // libx264 = software, last resort
```
Cap detects the primary GPU vendor, estimates whether HW can sustain the target res/fps
(`estimate_hw_encoder_max_fps`), and deliberately falls back to software for throughput HW can't
keep up with (logging a "high CPU" warning). Native MF path enumerates with
`MFTEnumEx(MFT_CATEGORY_VIDEO_ENCODER, hardware-first | software-fallback)`.

### Bitrate model (Cap-style bits-per-pixel)

```
bitrate = BPP × width × height × fps
QUALITY_BPP = 0.3      // default "polished" preset
ULTRA_BPP   = 1.0      // "max quality"
INSTANT_BPP = 0.15     // quick/small
keyframe interval = 2 s
```
HEVC (H.265) as an **opt-in** "smaller files" toggle (~30–40% smaller at similar quality); tag
`hvc1` for Apple/QuickTime compatibility. CRF mode (if ever used) forces `libx264` — keep off by
default for the licensing reasons above.

### Sidecar command lines (only if using the ffmpeg-sidecar path instead of bindings)

Pipe raw BGRA from the compositor to ffmpeg stdin:

```
# Hardware H.264 (NVENC) — ship-safe
ffmpeg -f rawvideo -pix_fmt bgra -s {W}x{H} -r {FPS} -i pipe:0 \
  -vf format=yuv420p -c:v h264_nvenc -preset p5 -rc vbr -cq 23 \
  -movflags +faststart out.mp4
```
Swap `-c:v hevc_nvenc` (or `hevc_qsv`/`hevc_amf`/`hevc_mf`) + `-tag:v hvc1` for HEVC. Detect
available encoders at runtime (`ffmpeg -hide_banner -encoders`); first that initializes wins.
(Prefer the in-process bindings or native MF over the sidecar to avoid piping raw frames.)

---

## GIF encoding

### Decision

**`gifski`** — best-in-class GIF quality (libimagequant per-frame palettes + temporal dithering),
which is what Cap uses. **But it is AGPL-3.0** — the single biggest GIF-side decision.

> ⚠️ **gifski is AGPL-3.0-or-later.** Statically linking it into a permissive/closed Vuoom imposes
> AGPL on the whole app. Options: **(a)** keep Vuoom's GIF module open-source, **(b)** buy
> gifski's commercial license from the author, or **(c)** invoke the gifski **CLI as a separate
> process** (mere aggregation). Get legal sign-off. See [`10-Licensing.md`](./10-Licensing.md).
> A pure-Rust fallback (`gif` + `color_quant` + `image`) avoids AGPL but gives markedly worse
> quality.

### gifski API (collector/writer thread model — Cap's usage)

```rust
let settings = gifski::Settings {
    width:  Some(width), height: Some(height),
    quality: 90,                 // 1–100
    fast: false,                 // true = faster, lower quality
    repeat: gifski::Repeat::Infinite,
};
let (collector, writer) = gifski::new(settings)?;

// writer thread drains to file:
std::thread::spawn(move || writer.write(File::create(path)?, &mut gifski::progress::NoProgress {}));

// feed frames; pts is in SECONDS → this is how fps is controlled (there is NO fps field):
let pts = frame_index as f64 / fps as f64;
collector.add_frame_rgba(frame_index, imgref::Img::new(rgba, w, h), pts)?;
// ...
drop(collector);                 // signals EOF; join the writer
```

Feeding blocks until prior frames are written → natural back-pressure. Convert compositor
BGRA→RGBA8 (swap R/B) and downscale (Lanczos via `fast_image_resize`) *before* feeding, for
size/quality control. Hit target fps by feeding every Nth frame and computing `pts` from the
emitted timeline.

### Post-optimization

Optional `gifsicle` second pass: `gifsicle -O3 --lossy=80 --colors 256` (frame dedup + lossy LZW;
big win on static-background screencasts). gifski already does strong per-frame palettes, so treat
gifsicle as a "shrink more" toggle.

### File-size estimation (there is no formula)

gifski builds a unique cross-frame palette and diffs temporally — **no closed-form size formula
exists** (the maintainer confirms; the CLI's own estimate is "very imprecise"). The honest method:

1. **Encode a representative sample** (every Nth frame, ~10–15%) at the chosen settings, measure
   bytes, **linearly extrapolate** by frame count (GIF is roughly per-frame additive after frame 1).
2. Scale up if the sample's mean inter-frame delta is high (more motion → worse compression).
3. Expose the deterministic levers (**width**, **fps**, **quality**, **lossy**) and **re-estimate
   live** as the user adjusts — exactly how gifski's author recommends converging on a target size.

### "Copy GIF to clipboard" — what actually works

Windows has **no animated-GIF clipboard format**; `CF_DIB`/`CF_BITMAP` are static (animation
lost). What real tools do:

1. **Copy the file (`CF_HDROP`)** → user pastes the actual `.gif` into Slack/Discord/email/Explorer,
   which then uploads the animated file. **This is the reliable, recommended behavior.**
2. Optionally also register a custom `"GIF"`/`"image/gif"` clipboard format with raw bytes — some
   apps honor it, most don't.
3. **Don't** put it on as a bitmap expecting animation — it won't animate anywhere.

**→ "Copy GIF" = copy the file (CF_HDROP); also offer "Copy file path" and "Reveal in Explorer."**

### WebP / APNG

Animated **WebP** is ~70–80% smaller than GIF at similar quality, full color, ~97% browser
support — **offer as an opt-in "smaller, modern" export** (encode via ffmpeg `libwebp_anim` or the
`webp` crate). GIF stays the default (universality). Skip APNG (little size benefit, worse compat).

---

## Default presets

**README-GIF (small + good):** fps **15**, width cap **≤1000px**, gifski `quality 80`, `fast
false`, infinite loop; frame dedup; optional `gifsicle -O3 --lossy=80`. A 10–15 s UI clip lands in
the low-MB range.

**HQ-GIF:** fps **20–24**, width up to **1280px**, gifski `quality 95`, no lossy pass.

**MP4 "polished" (primary):** MF/NVENC **H.264**, BPP **0.3** (≈ `0.3·W·H·fps`) or `cq ~23`,
yuv420p, keyframe 2 s, `+faststart`. HEVC opt-in toggle (`hvc1` tag).

Every preset shows an **estimated output file size** before export (sample-and-extrapolate for
GIF; bitrate×duration for MP4).

---

## Sources

- FFmpeg licensing: <https://www.ffmpeg.org/legal.html> · x264: <https://x264.org/licensing/> ·
  OpenH264 binary license: <https://www.openh264.org/BINARY_LICENSE.txt> ·
  LGPL builds: <https://github.com/BtbN/FFmpeg-Builds>
- HW encoders: <https://trac.ffmpeg.org/wiki/HWAccelIntro> · NVENC: <https://en.wikipedia.org/wiki/NVENC>
- Cap encoders: `crates/enc-ffmpeg/src/video/h264.rs`, `crates/enc-mediafoundation/src/mft.rs`,
  `crates/enc-gif/src/lib.rs` at <https://github.com/CapSoftware/Cap> ·
  windows-rs MF: <https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Media/MediaFoundation/index.html>
- gifski: <https://docs.rs/gifski/latest/gifski/> · <https://github.com/ImageOptim/gifski/> ·
  size estimation: <https://github.com/ImageOptim/gifski/issues/28>
- gifsicle lossy: <https://kornel.ski/lossygif>
- Clipboard GIF reality: <https://asapguide.com/how-to-copy-paste-animated-gif/>
- WebP vs GIF: <https://webp-to-png.tools/blog/animated-images-in-2025-webp-vs-apng-vs-gif-real-world-use-cases/>
