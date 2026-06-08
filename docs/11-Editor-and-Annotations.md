# 11 — The Editor UI & Annotations (Text, Arrows, Highlights)

Vuoom's editor must be **simple, clean, and "looks good by default"** — a focused demo-GIF
editor, *not* a video NLE. This doc covers the editing UI layout, the minimal tool set, the
interaction patterns, and how text/arrow/highlight annotations are rendered in the wgpu pipeline.

> Guiding philosophy (from every successful tool in the space): **automate first, allow manual
> override second.** A user should be able to record → export a beautiful GIF without touching a
> single control. Editing is for polish, not for assembly.

---

## 1. The minimal tool set (this is the whole list — resist adding more)

1. **Trim** — in/out handles on the timeline. The single most-used edit.
2. **Zoom-region edit** — auto-zoom blocks (from clicks) + "add zoom at playhead"; edit
   level/position/timing. The killer feature. (See [`04`](./04-Input-and-AutoZoom.md).)
3. **Text labels** ⭐ — the headline new feature. Add, position, font/size/color, and timing
   (appear/disappear + fade).
4. **Arrow** and **highlight box** — the basic annotation set. (Optional: spotlight/blur region.)
5. **Background + padding + rounded corners** — already in the compositor; exposed as a few
   presets + sliders.
6. **Speed-up dead time** — accelerate/skip idle stretches. Big perceived-quality win, low UI cost.
7. **Crop / aspect ratio** — presets: 16:9, 9:16, 1:1 (README / social).
8. **GIF export panel** — dimensions, fps, loop, quality, with a **live size estimate**.

**Deliberately left out** (to stay simple): multi-track timeline, audio, webcam, transitions
library, AI captions, layers/blend modes, color grading, bezier keyframe curve editors. Screen
Studio is praised precisely for *not* being a full editor — hold that line.

## 2. Layout — single window, record → edit → export

```
┌─────────────────────────────────────────────────────┐
│ Top bar:  ● Record      project title      Export GIF │
├──────────────────────────────────────┬──────────────┤
│                                       │  Properties  │
│            CANVAS (live preview)      │  panel       │
│   bg + padding + corners + zoom/pan   │  (context-   │
│   + text + arrows/highlights,         │   sensitive: │
│   composited by wgpu, streamed in)    │   shows the  │
│                                       │   selected   │
│                                       │   element)   │
├──────────────────────────────────────┴──────────────┤
│ Tool rail:  ▭ select │ T text │ ↗ arrow │ ▢ box │ ⌧ crop │
├──────────────────────────────────────────────────────┤
│ TIMELINE  ▸ playhead · trim handles · zoom blocks ·   │
│             text/annotation bars · speed regions      │
└──────────────────────────────────────────────────────┘
```

- **Canvas top, timeline bottom, one context-sensitive properties panel** on the right. This is
  the consistent pattern across Screen Studio, Cap, and screen-demo. The properties panel shows
  *only* the selected element's controls — never a permanent wall of options.
- **Strong defaults**: on entering the editor, a nice background gradient, padding, rounded
  corners, and auto-zoom are **already applied**. The empty/first-run state already looks great.

## 3. Interaction patterns (use conventions users already know — Figma/Canva)

**Text on the canvas:**
- **Click-to-add** an auto-sizing text box (or click-drag for fixed width).
- **Double-click to edit inline** on the canvas; **drag to move**; **corner handles to
  resize/scale**; optional snap/alignment guides.
- Font / size / color / weight live in the **properties panel**, bound to the selection, updating
  the preview in real time.
- The text's **time range** is a bar on the timeline — drag the ends to set when it appears /
  disappears; a small toggle for fade in/out.

**Arrows & highlight boxes:**
- Pick the tool, **drag on the canvas** to draw (arrow = drag from tail to head; box = drag a
  rect). Re-select to move/resize via handles. Color/thickness in the properties panel. Same
  timeline time-range bar as text.

**Zoom keyframes on the timeline:**
- Auto-generated zoom **blocks** appear on the timeline. **Drag a block** to retime it, **drag its
  edges** to change duration, edit **level/target** in the panel (or drag the zoom focus on the
  canvas). Vuoom interpolates smooth zoom/pan between blocks automatically — the user never
  touches raw curves.

## 4. Non-destructive, two-pass model (architecture)

Everything is non-destructive: capture stores the **near-lossless intermediate + the input/event
metadata**; every edit (zoom, text, arrow, trim, speed, background) is just **parameters** in the
`.vuoom` project. Rendering at any time `t` — for **scrubbing** and for **deterministic GIF
export** — is the *same* `render(t)` code path. This makes re-editing and re-exporting trivial and
lossless. (See [`02`](./02-Architecture.md), [`05`](./05-Compositing-and-Preview.md).)

## 5. Rendering annotations in the wgpu compositor

All annotations are drawn **on top** of the composited frame, in the same wgpu pass, with a
permissive (Apache/MIT) crate stack — **zero AGPL/GPL contamination**:

| Annotation | Crate | License | How |
|---|---|---|---|
| **Text labels** | **glyphon** 0.9.x (+ cosmic-text) | Apache-2.0 / MIT / zlib | "Bring your own wgpu" middleware — `prepare()` (CPU layout/raster/atlas) then `render()` into the existing render pass. Multi-line, color emoji, per-span color, clip bounds, runtime font loading. |
| **Arrows, highlight boxes, spotlight outline** | **lyon** (`lyon_tessellation`) | MIT / Apache-2.0 | Tessellate strokes/fills → triangles → a trivial wgpu pipeline. Arrow = stroked polyline + triangle head; box = stroked/filled rounded rect; spotlight = full-screen dim quad with a punched-out rounded rect. |
| **Blur / spotlight region** | your own wgpu shader | — | After compositing to a texture, run a small box/gaussian blur (or dim-mask) sampling that texture, masked to the lyon rounded-rect region. |

### Why this stack

- **glyphon** is the modern standard for text-in-wgpu, actively maintained, triple-licensed, and
  integrates *without adding a separate render pass*. Rejected alternatives: `wgpu_text`/`wgpu_glyph`
  (no real shaping, no emoji), raw `cosmic-text` (you'd reimplement glyphon), and **vello** (the
  appealing "one renderer for text+shapes" option, but **still alpha / blur in progress** in
  mid-2026 — revisit in ~a year).
- **lyon** is tessellation-only (backend-agnostic), so it slots into our own wgpu pipeline; it
  ships a wgpu example.
- **CPU fallback** if the wgpu shape pipeline gets fiddly: render annotations with
  `tiny-skia` + `cosmic-text` to an RGBA buffer and composite that as one texture. Simplest to
  reason about; costs a per-change CPU raster + upload. Keep as a backup, not the default.

### Keyframed / animated annotations (appear, move, fade) — keep it on the CPU

Don't pull an animation framework. Model each annotation as
`{ start, end, fade_in, fade_out, keyframes… }` in the timeline. Each render frame, compute
current `opacity / position / scale` from the playhead via lerp/ease, then:

- feed `opacity` into the glyph color **alpha** (cosmic-text per-span / `TextArea` color) and into
  the shape pipeline's uniform;
- only re-run glyphon `prepare()` when **text content or layout** changes — fades are just alpha,
  moves are just `TextArea.left/top`, both cheap.

The timeline is the single source of truth; scrubbing and export share the identical `render(t)`.

### Performance notes

- glyphon layout/raster is CPU-side — negligible for static labels; don't re-`prepare()` every frame.
- For **export**, rasterize text at the **export resolution** (don't scale glyphs up — avoids blur
  and atlas churn). Keep the `Viewport`/`Resolution` in sync with canvas size and DPI.

## 6. Reference products to emulate

- **Screen Studio** — defaults + the simple "drag zooms on a timeline" model (the gold standard).
- **Cap (cap.so)** — panel layout & studio-mode structure (⛔ AGPL — *design* reference only).
- **screen-demo (njraladdin, MIT)** — closest analog on our exact stack; notably it has **no
  text/annotations**, so leading with clean text + arrows is a real differentiator.

## Sources

- glyphon: <https://github.com/grovesNL/glyphon> · <https://docs.rs/glyphon> · cosmic-text:
  <https://github.com/pop-os/cosmic-text>
- lyon: <https://github.com/nical/lyon> · wgpu example:
  <https://github.com/nical/lyon/blob/main/examples/wgpu/src/main.rs>
- vello (why deferred): <https://docs.rs/vello> · <https://linebender.org/blog/tmil-19/>
- tiny-skia (CPU fallback): <https://github.com/linebender/tiny-skia>
- Editor UX: Screen Studio zooms guide <https://screen.studio/guide/adding-editing-zooms> ·
  Cap studio mode <https://cap.so/features/studio-mode> · screen-demo
  <https://github.com/njraladdin/screen-demo> · Screenize two-pass
  <https://www.blog.brightcoding.dev/2026/02/07/screenize-the-revolutionary-auto-zoom-screen-recorder> ·
  canvas text editing conventions (Figma) <https://help.figma.com/hc/en-us/articles/360039956434-Guide-to-text-in-Figma-Design>
