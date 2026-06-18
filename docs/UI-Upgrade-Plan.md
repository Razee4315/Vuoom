# UI Upgrade Plan — learning from palmier-pro

Status: **proposal** (no code changed yet). Author pass: 2026-06-19.

A plan to raise Vuoom's editor UI from "clean and functional" to "feels like a paid
pro tool", informed by a deep read of the open-source **palmier-pro** editor
(`D:\palmier-pro`, a native macOS Swift/AppKit NLE) used purely as a UX benchmark.

## Framing & guardrails

palmier-pro is a **multi-track NLE with AI generation** on a different stack (Swift/AppKit
vs Vuoom's Tauri + SolidJS). We copy **patterns and ergonomics, not code or scope.**

Respect Vuoom's settled identity (`vuoom-design-language`): mono-first themes, **no purple**,
record-red `#e5484d` as the only accent, the V+record-dot logo. Everything below stays inside
that language — we add *structure and motion*, not new brand colors.

**Explicitly out of scope** (would bloat the record→GIF focus): multi-track timeline, audio
tracks/waveforms, linked clips, razor/ripple tools, AI generation catalog, in-app agent chat
(the MCP **AI Demo Director** is the better moat). Keep the record-overlay flow — it already
beats palmier's import-first flow.

## Workflow

Per `vuoom-workflow-rules`: builds/tests run on **CI only**; **commit + push after every
change**. Each numbered item below lands as its own commit. Items are ordered by dependency —
the token layer (Phase 0) underpins the rest.

---

## Phase 0 — Design-token layer (foundation)

**Why:** Today `src/App.css` defines only `--bg/--panel/--line/--text/--muted/--accent`, one
radius pair, and a font scale (`App.css:6–82`). palmier's polish comes from one disciplined
token system (`AppTheme.swift`): a spacing scale, a 4-step elevation ramp, 3 shadow tiers, and
exactly 2 motion durations. Consistency *is* the premium feel.

**Add to `:root` (per theme where color-bearing):**

| Token group | Tokens | Notes |
|---|---|---|
| Spacing | `--sp-2 --sp-4 --sp-6 --sp-8 --sp-10 --sp-12 --sp-16 --sp-20 --sp-24` | even-only; screen edges always `--sp-24` |
| Elevation | extend `--panel` → `--surface --raised --prominent` | neutral, theme-aware; layers panels/cards/popovers |
| Shadow | `--shadow-sm` (r1) · `--shadow-md` (r4/y2) · `--shadow-lg` (r24/y8) | only three, like palmier |
| Motion | `--dur-hover: .15s` · `--dur-transition: .2s` | only two; reuse everywhere |
| Radius | keep `--radius`/`--radius-lg`; add `--radius-sm` (6) `--radius-xs` (3) | continuous feel |

**Acceptance:** no view hardcodes a px spacing/shadow/duration that a token covers; all 5
themes still pass a visual sanity check; zero behavior change.

---

## Phase 1 — Inspector ergonomics (highest impact)

The inspector is the surface that most reads as "settings form" today (`App.tsx:2156–2472`).
Three changes turn it pro.

### 1a. `ScrubField` component — drag-or-type number control
palmier's single best idea: every numeric value is **drag horizontally to scrub OR click to
type**, with **Shift = ×10 coarse, Ctrl = ×0.1 fine**, live preview during drag, one coalesced
undo on release, and `—` for mixed/empty.

- New `src/ScrubField.tsx`: a `<div>` capturing `pointermove` deltas → value; swaps to an
  `<input>` on click (drag < 3px). Props: `value, min, max, step, sensitivity, suffix,
  onInput (live), onCommit (undo boundary)`.
- Replace in the inspector: zoom **Strength** (`App.tsx:2376`), box **Opacity** (`2242`),
  **Thickness** (`2286`), text **Size** (`2196`), speed **factor** (`2432`), and the
  **Timing** number inputs (`2324–2346`).
- Reuse the existing throttled `pushEdit`/`refresh` path for live vs commit (`App.tsx:611`).

**Acceptance:** dragging any field scrubs with modifier precision and previews live; one undo
step per gesture; keyboard typing still works; touch/pointer captured cleanly.

### 1b. Section/row grammar
palmier groups properties into `InspectorSection` (tiny **UPPERCASE letter-spaced muted**
header) + `InspectorRow` (label left, value right-aligned, **fixed row height** so everything
aligns). Vuoom's `.field` rows vary in height and mix full-width sliders with label rows.

- Add `<InspSection title>` and `<InspRow label>` wrappers in `App.tsx` sub-components
  (near `InspectorPanel`, `App.tsx:2840`).
- Re-group: Text → `STYLE / COLOR / TIMING`; Box → `SHAPE / FILL / COLOR / TIMING`;
  Zoom → `STRENGTH / FOCUS`, etc.

**Acceptance:** every inspector row shares one baseline grid; section headers consistent
across all five selection types.

### 1c. Uniform hover/focus recipe
palmier applies one hover everywhere (scale `1.03`, shadow grows, border brightens, spring) and
an animated focus ring on the active panel. Define one `.hoverable` + `.panel-focus` in
`App.css` (using Phase-0 motion/shadow tokens) and apply to tool buttons, swatches, timeline
segments, and dialog buttons.

**Acceptance:** all interactive chrome shares one hover/active treatment; the properties panel
shows a subtle focus ring when it holds the selection.

---

## Phase 2 — Text annotations

The text surface is the weakest vs palmier and the one called out directly. Today: Inter-only
(`App.tsx:2027`), bold/italic + size slider, **move-only** on canvas (no resize handles),
no background plate.

### 2a. Bundled font set + in-typeface picker
palmier ships Anton, Bebas Neue, Space Grotesk, Playfair, Permanent Marker, etc. — that variety
is *why* their text looks designed.

- Bundle ~6 display fonts via `@font-face` (woff2 in `src/assets/fonts/`), OFL-licensed.
- Font picker where **each item renders in its own typeface**; ideally hover-preview on canvas
  with revert-on-cancel (palmier's `FontPickerField`). A styled static dropdown is an
  acceptable first cut.
- Backend: thread a `font` field through the text annotation (`add_text`/`update_text` in
  `src-tauri`) and the renderer (`vuoom-render`) + SVG preview `font-family` (`App.tsx:2027`).

### 2b. Background / outline / shadow plate (legibility)
Captions over busy screen-recording content are unreadable without a backing. palmier's TEXT
tab offers Background / Border / Shadow as color+switch rows. Add at least a **background plate**
(color + opacity + padding + corner radius) to the text annotation model, renderer, and
inspector.

### 2c. On-canvas text resize handles → font size
palmier resizes text by `fontScale` (never stretches). Vuoom's selected text draws only an
outline (`App.tsx:2034–2042`). Add corner handles that map drag → `font_size`, plus the same
move behavior. Reuse the box/arrow handle machinery (`handleAt`, `App.tsx:658`).

### 2d. Canvas snap guides
When dragging text (or any annotation), snap center/edges to the canvas center/edges and flash
a 1px guide-line (palmier draws magenta center guides). Lightweight in normalized space.

**Acceptance:** can pick from several fonts and see them on canvas; text stays legible over
any recording via a background plate; corner-drag scales the glyphs without distortion; drag
snaps to canvas center with a visible guide.

---

## Phase 3 — Timeline feel

The timeline drags are free-floating today (zoom/speed/cut/note bars, `App.tsx:2651–2785`),
so alignment is fiddly. palmier's timeline *snaps*.

### 3a. Snapping with a visual guide
- Snap any dragged band's edges to: the **playhead**, **ruler ticks**, and **other bands'
  edges**. Threshold **pixel-constant** (convert px → time via `tlEl` width) so it's
  zoom-independent; **sticky** break-away (must move ~2.5× to unstick), like palmier's
  `SnapEngine`.
- Flash a 1px vertical snap-line at the catch point; small scale-pop on the band. (No trackpad
  haptics on web — the visual sells precision instead.)
- Hook into the existing `onZoomMove/onSpeedMove/onCutMove/onAnnMove/onTrimMove` handlers.

### 3b. Adaptive ruler ticks
Replace fixed `ticks()` with palmier's approach: target ~80px between majors, snap to nice
values `[1,2,5,10,15,30,60…]`s; draw a taller mid minor tick. Cleaner ruler at any duration.

**Acceptance:** dragging a band catches the playhead/ticks/neighbors with a visible guide and a
sticky feel; the ruler shows sensible, evenly-spaced labels at short and long durations.

---

## Phase 4 — Starting UI / onboarding

The empty editor is a missed opportunity (`App.tsx:1887`). Keep it single-window — do **not**
build palmier's full sidebar home.

### 4a. Recents strip on the empty canvas
palmier's project cards: ~5:4 thumbnail, bottom gradient for legibility, **relative date**
("3 days ago"), hover-scale, right-click → Open / Reveal / Delete. Vuoom already persists
sessions (`check_recovery`, `App.tsx:347`) but only surfaces one "Recover last session" button
(`App.tsx:1902`). Replace with a small **recents grid** of cards (needs a backend list +
thumbnail; a first frame of each recording works).

### 4b. One-time welcome + coachmark
palmier gates a welcome card on `hasSeenWelcome` then offers a spotlight tour. For Vuoom's small
surface, a single centered glass card (value prop + "Record your screen" CTA) plus an optional
**4-step sequential coachmark** (Record → auto-zoom → annotate → export) anchored to the real
controls is enough. Persist a `vuoom-seen-welcome` flag in `localStorage` (like `themes.ts`).

**Acceptance:** first launch shows the welcome once; the empty state lists recent recordings as
clickable cards with relative dates and a context menu; the tour can be skipped and never
re-nags.

---

## What we deliberately skip
Multi-track, audio waveforms, linked clips, razor/ripple, keyframe animation lanes, AI
generation, in-app chat. These belong to palmier's NLE scope, not Vuoom's record→GIF job.

## Rough sequence
Phase 0 → 1 (1a is the big win) → 2 → 3 → 4. Phases 0–1 alone make the app *feel* two tiers
higher with no change to what it does.
