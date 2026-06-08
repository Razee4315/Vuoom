# 06 — Export: GIF

**v1.1 scope:** Vuoom exports **GIF only**. MP4/H.264/H.265 are dropped from v1 (owner decision —
see [spec amendments](./Vuoom-Spec.md) and [`09`](./09-Decisions-and-Open-Questions.md)). This is
a big simplification: **the entire video codec patent/licensing minefield disappears** — no Media
Foundation, no ffmpeg H.264, no x264/x265. The pipeline is just: composited RGBA frames → gifski →
optimized GIF.

> MP4 remains a clean *future* add (the compositor already produces RGBA frames; an encoder is the
> only missing piece) but it is explicitly **out of v1**. If/when it returns, the previous research
> (Media Foundation HW encoder, license-safe) is preserved in git history.

---

## Decision

| Concern | Choice |
|---|---|
| GIF encoder | **gifski** — best-in-class quality (libimagequant per-frame palettes + temporal dithering) |
| How it's integrated | **Separate bundled binary, invoked out-of-process** (keeps Vuoom Apache-2.0 — see below) |
| Optional size optimizer | `gifsicle` (also out-of-process) |
| "Smaller, modern" alt | animated **WebP** (opt-in, later) |
| Pure-Rust fallback | `gif` + `color_quant` + `image` (lower quality; only if gifski is ever unavailable) |

## The licensing point that shapes the integration

gifski is **AGPL-3.0-or-later**. Vuoom is **Apache-2.0**. To keep Vuoom's own source permissive,
**do not statically link gifski.** Ship the **gifski binary** (unmodified) and invoke it as a
separate process (mere aggregation) — the same separation pattern used widely for AGPL/GPL tools.
Get legal sign-off, but this is the standard, clean path. Full rationale:
[`10-Licensing.md`](./10-Licensing.md).

Practically: feed RGBA frames to the gifski **CLI** via a temp directory of PNGs or stdin (see
its `--help`), or pin a specific gifski release and document it. The in-process `gifski` *crate*
exists and is ergonomic, but linking it would impose AGPL on Vuoom — **don't**, given Apache-2.0.

## The pipeline

```
wgpu compositor → offscreen RGBA frame  (already producing these for preview/export)
        │   downscale to target width (Lanczos via fast_image_resize)
        │   pick every Nth frame to hit target fps (e.g. 60 capture → 15 GIF)
        ▼
   gifski (separate process):  RGBA/PNG frames + per-frame pts → optimized GIF
        │   (pts in SECONDS controls timing; there is no fps field)
        ▼
   optional: gifsicle -O3 --lossy=80 --colors 256   (separate process; "shrink more")
        ▼
   final .gif
```

### gifski knobs (whether via CLI flags or the crate's `Settings`)

- `quality` (1–100) — overall.
- `--lossy-quality` (lower = grainier/smaller), `--motion-quality` (lower = more smearing/smaller).
- `--width` / target width — the biggest deterministic size lever.
- `--fps` (CLI) or, in the lib, the **per-frame `pts` in seconds** — feed every Nth frame and
  compute `pts` from the emitted timeline.
- loop: infinite by default.

Convert compositor BGRA→RGBA8 (swap R/B) and downscale **before** handing frames to gifski for
predictable size and slightly better results.

## File-size estimation (there is no formula)

gifski builds a unique cross-frame palette and diffs temporally — **no closed-form size formula
exists** (the maintainer confirms; even gifski's own running estimate is "very imprecise"). The
honest method, which the editor's live size readout uses:

1. **Encode a representative sample** — every Nth frame (~10–15%) at the chosen settings, measure
   bytes, **linearly extrapolate** by frame count (GIF is roughly per-frame additive after frame 1).
2. Scale up if the sample's mean inter-frame delta is high (more motion → worse compression).
3. Expose the deterministic levers (**width**, **fps**, **quality**, **lossy**) and **re-estimate
   live** as the user drags them — exactly how gifski's author recommends converging on a target.

## "Copy GIF to clipboard" — what actually works on Windows

Windows has **no animated-GIF clipboard format** (`CF_DIB`/`CF_BITMAP` are static; animation is
lost). The reliable behavior every real tool uses:

1. **Copy the file (`CF_HDROP`)** → the user pastes the actual `.gif` into Slack / Discord /
   email / a GitHub comment, which uploads the animated file. **This is the recommended default.**
2. Optionally also register a custom `"GIF"`/`"image/gif"` clipboard format with raw bytes — some
   apps honor it, most don't.
3. **Don't** put it on as a bitmap expecting animation — it won't animate anywhere.

→ **"Copy GIF" = copy the file (CF_HDROP).** Also offer **"Copy file path"** and **"Reveal in
Explorer."** This single action is the most-used step for posting — make it one click.

## Default export presets

**README-GIF (default — small + good):** fps **15**, width cap **≤1000px**, gifski `quality 80`,
infinite loop; optional `gifsicle -O3 --lossy=80 --colors 256` second pass. A 10–15 s UI clip lands
in the low-MB range. This is the headline preset — programmers want a small README GIF.

**HQ-GIF:** fps **20–24**, width up to **1280px**, gifski `quality 95`, no lossy pass.

**Social presets:** pair the GIF settings with the 9:16 / 1:1 crop from the editor.

Every preset shows an **estimated output size** (sample-and-extrapolate) before export.

## WebP (opt-in, later)

Animated **WebP** is ~70–80% smaller than GIF at similar quality with ~97% browser support — a
nice **opt-in "smaller, modern" export** for places that accept it (it is *not* a drop-in for
GitHub README inline rendering the way GIF is, so GIF stays the default). Encode via a small
permissively-licensed path (the `webp`/`image` crates, or an LGPL ffmpeg `libwebp_anim` sidecar).
Skip APNG (little size benefit, worse compatibility).

## Implementation checklist (M4)

- [ ] Frame tap: compositor offscreen RGBA → BGRA→RGBA → downscale → frame buffer for export.
- [ ] Frame selection to hit target fps; compute per-frame `pts`.
- [ ] gifski **out-of-process**: bundle the binary as a sidecar, pipe frames, stream progress via
      a `tauri::ipc::Channel`.
- [ ] Optional gifsicle second pass behind a "shrink more" toggle.
- [ ] Live size estimate (sample-and-extrapolate) wired to the export panel sliders.
- [ ] Copy-GIF-as-file (CF_HDROP), copy-path, reveal-in-Explorer.
- [ ] README/HQ presets; aspect/crop integration.

## Sources

- gifski: <https://docs.rs/gifski/latest/gifski/> · <https://github.com/ImageOptim/gifski/> ·
  size estimation reality: <https://github.com/ImageOptim/gifski/issues/28>
- gifsicle lossy: <https://kornel.ski/lossygif>
- Clipboard GIF reality: <https://asapguide.com/how-to-copy-paste-animated-gif/>
- WebP vs GIF: <https://webp-to-png.tools/blog/animated-images-in-2025-webp-vs-apng-vs-gif-real-world-use-cases/>
- AGPL isolation rationale: [`10-Licensing.md`](./10-Licensing.md)
