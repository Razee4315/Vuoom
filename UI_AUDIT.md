# Vuoom UI Audit — Current Build (`v2.4.0`)

**Audited Artifact:** GitHub Release [`v2.4.0`](https://github.com/Razee4315/Vuoom/releases/tag/v2.4.0) (`Vuoom_2.4.0_x64_en-US.msi` extracted via `msiexec /a` -> `vuoom.exe`, `31,456,256 bytes`)  
**Audit Date:** 2026-11-27  
**Branch / Baseline Commit:** `ui/redesign` branched from `main` (`2c2db09`, rollback tag `pre-ui-redesign-2026-11-27`)  
**Screenshots Captured:** `37` PNGs in [`docs/ui-redesign/screenshots/before/`](./docs/ui-redesign/screenshots/before/) covering Dark (`mono-dark`), Light (`mono-light`), `1280×800`, `880×560` minimum window, `150%` DPI, all Settings tabs, Editor states, Export formats, Command Palette, Menus, and Record Overlays (with host desktop backdrops blurred for privacy).

---

## 1. Live `v2.4.0` CI Binary Telemetry & Runtime Health

Measured directly against the packaged `v2.4.0` `vuoom.exe` via WebView2 CDP (`--remote-debugging-port=9222`) using an isolated `WEBVIEW2_USER_DATA_FOLDER`:

| Metric | Observed (`v2.4.0`) | Notes / Source |
|---|---|---|
| Console Errors / Uncaught Exceptions | **`0`** / **`0`** | Clean across all 34 initial screen/flow transitions |
| Rust `stderr` Errors / Warnings | **`0`** errors | Only normal `INFO` logs (`vuoom_lib`, `wgc`, `frame_store` 1961.0× ratio) |
| Initial Viewport (Native Maximized) | `1536 × 824` (`dpr: 1.25`) | `1920×1080` at 125% Windows display scale |
| DOM Interactive / DOMContentLoaded / Load | `49.9 ms` / `145.9 ms` / `259.0 ms` | Measured on live WebView2 boot |
| First Paint / First Contentful Paint | `148.0 ms` / `148.0 ms` | `#splash` remained visible until `2,084 ms` (`splashGoneMs`) |
| Sample Clip Load (`Try a sample clip`) | `1,624 ms` | Triggered `.busy-veil` overlay cleanly |
| Active Editor DOM Snapshot | `688` elements, `77` SVGs, `116` buttons, `91` `[style]` | `Performance.getMetrics`: `2,168` nodes, `54` JS listeners, `3.29 MB` JS heap |
| Process Memory Footprint (after Editor load) | Rust `311.9 MB` WS (`314.0 MB` Priv); WebView2 (6 procs) `690.0 MB` WS (`493.7 MB` Priv) | Sample clip decompressed & cached in memory |

> **Note on Welcome Screen Capture:** `vuoom.exe` resolves its persistent config via Win32 `FOLDERID_RoamingAppData` rather than `%APPDATA%`, so `seen_welcome` remained set across runs and `Welcome` was audited via full source/CSS inspection ([`Welcome.tsx`](./src/components/Welcome.tsx), [`dialogs.css`](./src/styles/dialogs.css), [`stage.css`](./src/styles/stage.css)) without altering user AppData.

---

## 2. Theme System & WCAG 2.2 AA Contrast Audit

### 2.1 Critical Bug: Inverted Light/Dark Accent Overrides (`src/styles/tokens.css:220-230`)

In [`src/styles/tokens.css:220-230`](./src/styles/tokens.css#L220-L230):
```css
/* The shared accents, darkened until they read on a light panel. */
:root[data-theme="mono-dark"] {
  --ok: #11883d;
  --warn: #a56900;
  --record: #d83b40;
}
:root[data-theme="graphite"] {
  --ok: #0b8237;
  --warn: #9f6300;
  --record: #d13439;
}
```
The comment explicitly states *"darkened until they read on a light panel"*, and luminance math proves `#11883d`/`#a56900`/`#d83b40` were calibrated to clear `4.5:1` on `mono-light`'s `--panel` (`#ffffff`), while `#0b8237`/`#9f6300`/`#d13439` were calibrated for `paper`'s `--panel` (`#f7f5f0`). However, the CSS selectors target **the two dark themes** (`mono-dark` and `graphite`) instead of `mono-light` and `paper`!

This causes **double contrast damage**:
1. `mono-light` and `paper` keep bright dark-mode accents (`--ok: #3fb66b`, `--warn: #f0b429`, `--record: #e5484d`), failing WCAG 2.2 AA catastrophically on light surfaces (**1.48:1–2.59:1** for `--warn` and `--ok`, **3.11:1–3.91:1** for `--record`).
2. `mono-dark` and `graphite` receive darkened accents that drop **below 4.5:1 AA normal text** on every surface (**3.73:1–4.40:1** in `mono-dark`, **3.06:1–3.81:1** in `graphite`).

### 2.2 Live 5-Theme WCAG 2.2 Relative Luminance & Contrast Matrix

Measured from live `getComputedStyle(document.documentElement)` values on `v2.4.0`:
- **PASS AA**: `>= 4.50:1` (normal text) · **UI/Lg**: `3.00–4.49:1` (non-text UI / large text only; fails normal text) · **FAIL**: `< 3.00:1` (fails both).

| Theme | Token | Live Value | vs `--backdrop` | vs `--panel` | vs `--panel-2` | vs `--raised` | Status Summary |
|---|---|---|---:|---:|---:|---:|---|
| **`mono-dark`** | `--text` / `--text-2` | `#f5f5f6` / `#c9c9cf` | 18.37 / 12.14 | 17.30 / 11.43 | 16.56 / 10.95 | 15.58 / 10.29 | PASS AA |
| | `--muted` / `--faint` | `#8b8b95` / `#7b7b85` | 5.93 / 4.78 | 5.59 / 4.50 | 5.35 / **4.31** | 5.03 / **4.05** | `--faint` UI/Lg on `--panel-2`/`--raised` |
| | `--record` / `--ok` / `--warn`* | `#d83b40` / `#11883d` / `#a56900` | **4.40** | **4.15** | **3.97** | **3.73** | **UI/Lg only** (all 3 fail 4.5:1 due to bug) |
| **`mono-light`** | `--text` / `--text-2` | `#141417` / `#3e3e46` | 15.60 / 8.99 | 18.39 / 10.59 | 17.63 / 10.16 | 16.74 / 9.64 | PASS AA |
| | `--muted` / `--faint` | `#6d6d77` / `#757580` | **4.34** / **3.86** | 5.12 / 4.55 | 4.91 / **4.36** | 4.66 / **4.14** | `--muted` UI/Lg on backdrop; `--faint` UI/Lg |
| | `--record` | `#e5484d` | **3.32** | **3.91** | **3.75** | **3.56** | **UI/Lg only** (fails 4.5:1) |
| | `--ok` | `#3fb66b` | **2.20** | **2.59** | **2.48** | **2.36** | **FAIL** everywhere |
| | `--warn` | `#f0b429` | **1.58** | **1.86** | **1.79** | **1.70** | **FAIL** everywhere |
| **`graphite`** | `--text` / `--text-2` | `#eaedf0` / `#c2c8cd` | 15.99 / 11.13 | 14.73 / 10.25 | 13.96 / 9.72 | 12.83 / 8.93 | PASS AA |
| | `--muted` / `--faint` | `#8b959e` / `#7a848d` | 6.17 / 4.93 | 5.68 / 4.54 | 5.38 / **4.31** | 4.95 / **3.96** | `--faint` UI/Lg on `--panel-2`/`--raised` |
| | `--record` / `--ok` / `--warn`* | `#d13439` / `#0b8237` / `#9f6300` | **3.81** | **3.51** | **3.33** | **3.06** | **UI/Lg only** (all 3 fail 4.5:1 due to bug) |
| **`paper`** | `--text` / `--text-2` | `#28261f` / `#4a473d` | 12.04 / 7.39 | 13.89 / 8.53 | 13.30 / 8.16 | 12.72 / 7.81 | PASS AA |
| | `--muted` / `--faint` | `#6f6b5e` / `#757061` | **4.24** / **3.93** | 4.89 / 4.54 | 4.68 / **4.34** | **4.48** / **4.16** | `--muted` fails backdrop/raised; `--faint` UI/Lg |
| | `--record` | `#e5484d` | **3.11** | **3.59** | **3.44** | **3.29** | **UI/Lg only** |
| | `--ok` | `#3fb66b` | **2.06** | **2.38** | **2.27** | **2.18** | **FAIL** everywhere |
| | `--warn` | `#f0b429` | **1.48** | **1.71** | **1.64** | **1.57** | **FAIL** everywhere |
| **`midnight`** | `--text` / `--text-2` | `#e8edf5` / `#b8c2d1` | 16.94 / 11.07 | 15.96 / 10.43 | 15.27 / 9.98 | 14.24 / 9.31 | PASS AA |
| | `--muted` / `--faint` | `#8490a3` / `#727d90` | 6.16 / 4.79 | 5.81 / 4.51 | 5.56 / **4.32** | 5.18 / **4.03** | `--faint` UI/Lg on `--panel-2`/`--raised` |
| | `--record`/`--ok`/`--warn` | `#e5484d`/`#3fb66b`/`#f0b429` | 5.09 / 7.70 / 10.69 | 4.79 / 7.25 / 10.06 | 4.59 / 6.94 / 9.63 | **4.28** / 6.47 / 8.98 | `--record` UI/Lg on `--raised` |

Additional high-impact contrast & theme findings:
- **White text on Record buttons (`.btn.record`, `.hud-record`, `.rec-stop`):** `#ffffff` on `#e5484d` is **3.91:1** (fails 4.5:1 AA on 12–14px labels); on `:hover` (`--record-hover: #f25f63`), it drops to **3.19:1** across all themes and `RecordOverlay`.
- **Invisible default click-effect art in Light themes ([`Inspector.tsx:1797`](./src/components/Inspector.tsx#L1797)):** `#ffffff` stroke on `--panel-2` (`#fafafb` / `#f3f0e9`) yields **1.04:1**.
- **Light-mode startup theme flash ([`index.html:6-29`](./index.html#L6-L29), [`createEditor.ts:582`](./src/editor/createEditor.ts#L582)):** `index.html` hardcodes `#0e0e0f` dark splash background and `applyTheme()` only runs in `onMount` after `initLocale()`, causing light-theme users to see a `~2s` dark splash before flashing bright white.
- **No OS system theme following ([`src/themes.ts`](./src/themes.ts)):** Hardcodes `DEFAULT_THEME = "mono-dark"` with zero `prefers-color-scheme` support or `"System"` mode.

---

## 3. UI Component Inventory, Duplicates & Performance Audit

### 3.1 Complete Inventory of Components in Use

| Category | Primitive / Pattern | Source Location(s) | CSS Classes |
|---|---|---|---|
| **Buttons** | Standard button (no JSX wrapper) | `Titlebar.tsx`, `Home.tsx`, `Inspector.tsx`, `Settings.tsx`, `ExportDialog.tsx`, `Welcome.tsx`, `CropEditor.tsx`, `Permissions.tsx` | `.btn`, `.btn.sm`, `.btn.lg`, `.btn.block`, `.btn.ghost`, `.btn.primary`, `.btn.record`, `.btn.danger` ([`base.css`](./src/styles/base.css#L149-L230)) |
| **Icon Buttons** | `IconButton` + raw `<button class="ibtn">` | [`ui.tsx:24-51`](./src/ui.tsx#L24-L51), `Settings.tsx`, `ExportDialog.tsx`, `SourcePicker.tsx`, `App.tsx` | `.ibtn`, `.ibtn.on`, `.ibtn.sm`, `.ibtn.danger` |
| **Window Controls** | `WindowControls` | [`WindowControls.tsx`](./src/WindowControls.tsx) | `.winctl`, `.winbtn`, `.winbtn.close` ([`shell.css`](./src/styles/shell.css#L159-L184)) |
| **Icons & Brand** | `Icon` (97 paths on 24×24 grid), `LogoMark` | [`icons.tsx`](./src/icons.tsx), [`Logo.tsx`](./src/Logo.tsx) | Inline `<svg>`, `LogoMark` hardcodes `#e5484d` |
| **Inputs / Textareas** | Raw `<input>` / `<textarea>` | `Home.tsx`, `Inspector.tsx`, `ExportDialog.tsx`, `Titlebar.tsx`, `CommandPalette.tsx`, `Stage.tsx` | `.input`, `.input-wrap`, `.text-field`, `.cap-text`, `.clip-search .input`, `.tb-name`, `.pal-search input`, `.text-edit` |
| **Select** | Raw `<select class="input">` | [`Inspector.tsx:1931`](./src/components/Inspector.tsx#L1931) | `select.input` ([`base.css:318-328`](./src/styles/base.css#L318-L328)) |
| **Scrub Fields** | `ScrubField` (`role="spinbutton"`) | [`ScrubField.tsx`](./src/ScrubField.tsx) | `.scrub`, `.scrub.disabled`, `.scrub-val`, `.scrub-suffix`, `.scrub-input` |
| **Switches & Sliders** | `Switch`, `Slider` | [`ui.tsx:105-160`](./src/ui.tsx#L105-L160) | `.switch`/`.switch-knob`, `.slider`/`.slider-val` |
| **Segmented Controls** | `Seg` + 4 ad-hoc clones | [`ui.tsx:56-101`](./src/ui.tsx#L56-L101), `Inspector.tsx`, `CropEditor.tsx`, `RecordOverlay.tsx` | `.seg`/`.seg-thumb`/`.seg-btn`, `.toggle-group`/`.tgl`, `.crop-grid`/`.chip`, `.crop-locks`/`.crop-lock`, `.hud-seg`/`.hud-chip` |
| **Sections & Fields** | `Section`, `Field`, `<dl class="facts">` | [`ui.tsx:174-234`](./src/ui.tsx#L174-L234), `Inspector.tsx` | `.sect`, `.sect-head`, `.sect-body-wrap`, `.field`, `.note`, `.facts` |
| **Color Pickers** | Swatches & custom wells | [`Inspector.tsx`](./src/components/Inspector.tsx#L336-L390) | `.swatches`, `.color-row`, `.swatch`, `.swatch-custom`, `.color-well` |
| **Cards** | 14 unshared card variants | `Home.tsx`, `Permissions.tsx`, `Settings.tsx`, `Inspector.tsx`, `ExportDialog.tsx`, `Welcome.tsx` | `.source-card`, `.recent`, `.recover-card`, `.home-perm`, `.theme-card`, `.storage-card`, `.welcome-step`, `.export-format`, `.export-preset`, `.font-card`, `.frame-card`, `.look-card`, `.bg-picture`, `.ptr-picture` |
| **Lists** | Source list, Command list, Shortcuts, Audio | `SourcePicker.tsx`, `CommandPalette.tsx`, `Settings.tsx`, `Inspector.tsx` | `.source-item`, `.pal-item`, `.shortcut-row`, `.audio-row` |
| **Modals / Dialogs** | `dialogA11y` + 5 modals | [`dialog.ts`](./src/dialog.ts), `Settings`, `ExportDialog`, `SourcePicker`, `CommandPalette`, `Welcome` | `.modal-backdrop`, `.modal`, `.modal.settings`, `.modal.export`, `.source-modal`, `.palette`, `.welcome` |
| **Menus & Context Menus** | `Menu`, `MenuPopup`, `ContextMenuHost` | [`ui.tsx:277-460`](./src/ui.tsx#L277-L460), [`contextMenus.ts`](./src/components/contextMenus.ts) | `.menu`, `.menu-item`, `.menu-sep`, `.menu-heading` |
| **Tooltips & Toasts** | `TooltipLayer`, `ToastHost` | [`ui.tsx:467-626`](./src/ui.tsx#L467-L626) | `.tooltip`, `.toasts`, `.toast`, `.toast-action`, `.toast-x` |
| **Tabs & Navigation** | Titlebar menus, ToolRail, Inspector tabs, Settings nav, StatusBar | `Titlebar.tsx`, `ToolRail.tsx`, `Inspector.tsx`, `Settings.tsx`, `StatusBar.tsx` | `.tb-menus`, `.rail`, `.insp-tabs`, `.settings-nav`, `.statusbar` |
| **Badges / Chips / HUDs** | Status pills, dirty dot, timeline & overlay HUDs | `Titlebar`, `Inspector`, `Timeline`, `CropEditor`, `Welcome`, `RecordOverlay` | `.badge`, `.chip`, `.tb-dirty`, `.coachmark`, `.stage-hint`, `.crop-bar`, `.hud`, `.sel-dimtag` |
| **Empty & Loading States** | Empty states, splash, `.busy-veil`, rings, progress bars, meters, shimmer | `index.html`, `App.tsx`, `ExportDialog.tsx`, `Inspector.tsx`, `AudioControls.tsx`, `Timeline.tsx`, `RecordOverlay.tsx` | `.insp-empty`, `.clip-search-empty`, `.recents-empty`, `.pal-empty`, `.busy-veil`, `.spinner`, `.ring`, `.cap-bar`, `.meter`, `.tl-film:empty` |
| **Banners** | GPU lost, crash recovery, macOS permissions | `App.tsx`, `Home.tsx`, `Permissions.tsx` | `.banner`, `.recover-card`, `.home-perm` |
| **Keyboard Chips** | `Kbd`, `Rich`, raw `<kbd>`, `.pal-kbd`, `.menu-kbd`, `.tooltip-kbd` | [`ui.tsx`](./src/ui.tsx#L241-L258), `Settings.tsx`, `CommandPalette.tsx`, `Titlebar.tsx`, `RecordOverlay.tsx` | `kbd`, `.kbds`, `.pal-kbd`, `.menu-kbd`, `.tooltip-kbd` |

### 3.2 Duplicates & Inconsistencies
1. **5 Segmented Control Implementations:** `Seg` ([`ui.tsx`](./src/ui.tsx#L56)), `.toggle-group`/`.tgl` ([`inspector.css`](./src/styles/inspector.css#L179)), `.crop-grid .chip` ([`inspector.css`](./src/styles/inspector.css#L345)), `.crop-locks`/`.crop-lock` ([`stage.css`](./src/styles/stage.css#L326)), and `.hud-seg`/`.hud-chip` ([`RecordOverlay.css`](./src/RecordOverlay.css#L172)). Plus `.tgl.wide` in `Inspector.tsx:555` has zero CSS definition.
2. **Duplicate Buttons & Pickers:** `.home-more` duplicates `.audio-picker` almost verbatim; `CameraControls.tsx:151` and `RecordOverlay.tsx:1184` reuse `.audio-picker`/`.hud-audio` classes on the camera trigger (coloring the camera icon teal `--t-audio`!); `Settings.tsx`, `ExportDialog.tsx`, `SourcePicker.tsx`, and `App.tsx` use raw `<button class="ibtn">` instead of `<IconButton>`.
3. **4 Keyboard Chip Rules & Hardcoded Keys:** `Titlebar.tsx:185` hardcodes `"Ctrl K"`; `CommandPalette.tsx` renders `<kbd>Esc</kbd>` differently from `<span class="pal-kbd">` in the same header/list; `Settings.tsx:425` and `RecordOverlay.tsx` manually loop `<kbd>` instead of using `<Kbd>`.
4. **Duplicate Swatches & Ring Styles:** `.color-row` ([`inspector.css:206`](./src/styles/inspector.css#L206)) duplicates `.swatches` ([`base.css:952`](./src/styles/base.css#L952)) identically; `POINTER_COLORS` duplicates `PRESET_COLORS`; selectable cards split between outer rings (`.font-card`, `.theme-card`, `.export-format`), inner rings (`.frame-card`, `.look-card`), and filled-only (`.export-preset`).
5. **Control Height, Radius & Weight Drift:** **8 control heights** (`24, 26, 28, 30, 32, 34, 36, 40px` — 4 different heights on the `44px` Titlebar alone!), **15 hardcoded radii** (`2px`–`24px`, `RecordOverlay.css` uses zero `--r-*` tokens), **13 numeric font weights** (`400, 500, 520, 550, 560, 600, 620, 650, 680, 700, 720, 750, 760`), and **duplicate focus rings** (`.corner-cell` and `.ptr-hotspot` stack `outline` on top of `box-shadow: var(--focus)`).
6. **Dead CSS & Conflicting Breakpoints:** `--fs-xs` and `--fs-2xs` are both `11px`; `.rail-lock.on`, `.audio-cleaning`, `.hud-menu`, `.welcome-backdrop` are unused; `shell.css` has conflicting `@media (max-height: 780px)` and `(max-height: 820px)` rules; `dialogs.css` & `shell.css` rely on `8` `!important` overrides.
7. **Duplicate Icons & Brand Drift:** `icons.tsx` has overlapping `keyboard`/`keys` and indistinguishable `bringForward`/`sendBackward`; `Settings.tsx:42` duplicates theme colors in `THEME_ART` JS; `Settings.tsx:440` About tab uses a generic filmstrip icon instead of `LogoMark`.

### 3.3 Performance, Typography & Native Desktop Feel Audit
- **9 `backdrop-filter: blur(...)` usages** ([`dialogs.css:11`](./src/styles/dialogs.css#L11), [`stage.css:55,318`](./src/styles/stage.css#L55), [`RecordOverlay.css:102,142,497,514,535,584`](./src/RecordOverlay.css#L102)) sit over full viewports and live 60fps `<canvas>` elements, forcing continuous readback blurs.
- **Heavy / `100vmax` Spread Shadows & SVG Filters:** `.sel-rect` and `.crop-rect` paint `0 0 0 100vmax` shadows on every drag frame; SVG `filter: drop-shadow(...)` runs on `.handle` and `.sel-outline`.
- **Non-Composited & Idle Animations:** `.insp-empty-icon` runs `animation: float 3.2s infinite` continuously whenever the Editor has no selection; `.tl-film:empty` runs infinite `background-position`; `.sect-body-wrap` transitions `grid-template-rows`; `.meter-fill` animates `width` every `70ms` (`AudioControls.tsx`) while `CameraControls.tsx:115` runs an unguarded `setInterval(..., 66)` without `document.hidden` checks; `.cap-bar > i` transitions `width`; `.ptr-hotspot-mark` transitions `left, top`.
- **Reduced Motion Bypasses:** Hardcoded durations in `index.html:28`, `motion.css:71` (`float`), `dialogs.css:802` (`.spinner`), `base.css:676`, `shell.css:423`, `inspector.css:544`, `timeline.css:696`, and most of `RecordOverlay.css` bypass `prefers-reduced-motion` / `data-motion="reduced"`, and stagger `animation-delay` (`20–140ms`) leaks when durations drop to `0s`.
- **Font Loading Bug & Bundle Waste:** `@fontsource-variable/inter` registers `"Inter Variable"`, but `constants.ts:14` and `mock/engine.ts:1252` request `"Inter, sans-serif"`, causing canvas annotations to miss the bundled font! Worse, `@fontsource-variable/inter` bundles **7 subsets (`218.5 KB`)** despite only `en` + `zh-CN` existing, and `base.css` ships **683.6 KB** of uncompressed `.ttf` display fonts (nearly half the `dist/` budget) with mismatched `font-weight: 700` on `Poppins`.
- **Browser Leftovers & Cursors:** `base.css:64` sets `button { cursor: pointer; }` globally; `timeline.css:312` sets **`.tl { cursor: text; }`** over the timeline; `createEditor.ts:565-574` misses `Ctrl+Shift+R` and `F12` reload/devtools chords; `Inspector.tsx:1535` `<img />` omits `draggable={false}`; `.export-path`/`.export-error` suppress right-click Copy menus.

---

## 4. Screen Scorecard (1–5) & Verdicts

**Scoring rubric (`1` = broken/missing, `3` = usable with clear friction, `5` = studio-grade):**  
`VH` Visual Hierarchy · `LS` Layout & Spacing · `TY` Typography · `CO` Color & Contrast · `CN` Consistency · `DN` Density · `CA` Clarity of Main Action · `EL` Empty / Loading / Error States · `A11` Accessibility

| Screen / Surface | VH | LS | TY | CO | CN | DN | CA | EL | A11 | Avg | Verdict | Primary Drivers |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| **1. Home** | 3 | 3 | 4 | 3 | 3 | 3 | 4 | 2 | 3 | **3.1** | **REDESIGN** | Centered hero + small stacked footers wastes viewport; `880×560` scrolls; missing loading/empty polish; weak option/picker hierarchy |
| **2. Titlebar & StatusBar** | 4 | 3 | 4 | 3 | 3 | 3 | 4 | 3 | 2 | **3.2** | **POLISH** | Search overlaps dirty dot at `880×560`; 4 control heights on Titlebar; dishonest Undo/Redo; `<24px` StatusBar buttons; unlabeled collapsed `.tb-search` |
| **3. Stage & CropEditor** | 4 | 4 | 4 | 4 | 3 | 4 | 4 | 3 | 2 | **3.6** | **POLISH** *(Stage)* / **REDESIGN** *(CropEditor)* | Stage layout strong; `CropEditor` is pointer-only with `10–14px` handles & stale module-level aspect lock; toast overlaps top of stage |
| **4. ToolRail** | 3 | 2 | 3 | 4 | 4 | 2 | 4 | 4 | 3 | **3.2** | **REDESIGN** | **Bottom tools (`Spotlight`, `Mask`, collapse toggle) clip off-screen at `1280×800` & `880×560`!** Conflicting `780px`/`820px` media queries; no locked state SR label |
| **5. Timeline** | 4 | 3 | 4 | 4 | 3 | 3 | 4 | 3 | 2 | **3.3** | **REDESIGN** | Crowded 3-zone header at `880×560`; `.tl { cursor: text; }`; `<24px` handles/mute/lanes; zero keyboard scrubbing/trimming; missing waveform load & caption menu |
| **6. Inspector** | 3 | 3 | 4 | 3 | 3 | 3 | 3 | 3 | 2 | **3.0** | **REDESIGN** | Opens on Clip with 9 closed accordions leaving 40% empty void; tool mode leaves 65% blank space; incomplete ARIA `tablist`; invisible color-input focus; `.insp-empty-icon` infinite float |
| **7. ExportDialog** | 4 | 3 | 4 | 3 | 3 | 4 | 5 | 4 | 3 | **3.7** | **REDESIGN** | Uneven card text wrapping; `Balanced`/`High quality` show identical `30 fps · 1740px` in MP4; `"Smooth\ngradients"` wraps awkwardly; unaligned `"Fit under"` row; missing `fitToBudget` abort & live regions |
| **8. Settings** | 4 | 3 | 4 | 3 | 3 | 3 | 4 | 2 | 3 | **3.2** | **REDESIGN** | Close button scrolls away inside `.settings-body`; sticky focus ring stays on `Appearance`; Recording copy clips mid-line; Storage shows `"Measuring…"`/`-` & unconfirmed `Clear`; About uses film icon; no `"System"` theme |
| **9. CommandPalette** | 5 | 4 | 4 | 4 | 4 | 4 | 5 | 4 | 2 | **4.0** | **POLISH** | Crisp layout & search; broken ARIA `combobox` / `tabindex="-1"` option pattern; tool entries hardcode `"cursor"` icon; dual `<kbd>` styles |
| **10. SourcePicker & WindowPicker** | 3 | 3 | 4 | 4 | 3 | 4 | 4 | 2 | 2 | **3.2** | **REDESIGN** | `WindowPicker` lacks `dialogA11y` trap & keyboard selection + clips `.sel-dimtag`; `SourcePicker` lacks window search/refresh/app differentiation |
| **11. RecordOverlay (HUD / StopBar / MiniBar)** | 4 | 3 | 4 | 4 | 3 | 3 | 4 | 2 | 2 | **3.2** | **REDESIGN** | Bottom HUD crowds/wraps & collides with low selections; missing `aria-label` on `.hud-cancel`; no focus trap or keyboard framing; silent camera/WebSocket failure states; heavy `backdrop-filter` |
| **12. Welcome, Coachmark & Busy Veil** | 4 | 3 | 4 | 3 | 3 | 4 | 4 | 2 | 3 | **3.3** | **POLISH** | `.busy-veil` has no surface card (collides directly over Home cards); Welcome focuses skip button first; Permissions omits `input` & flattens on `--panel`; responsive overflow `<540px` |

*(Verdict summary: **8 screens/surfaces marked `REDESIGN`** + `CropEditor`; **4 marked `POLISH`**.)*

---

## 5. Screen-by-Screen Detailed Findings & Screenshot Gallery

### 5.1 Home (`src/components/Home.tsx`, `src/styles/home.css`) — **REDESIGN**
- **Visual & Layout Findings:** Centered column leaves massive empty vertical margins at `1280×800` ([`home-empty-dark.png`](./docs/ui-redesign/screenshots/before/home-empty-dark.png), [`home-empty-light.png`](./docs/ui-redesign/screenshots/before/home-empty-light.png)), yet overflows into a vertical scrollbar at minimum `880×560` ([`home-small-880x560-dark.png`](./docs/ui-redesign/screenshots/before/home-small-880x560-dark.png)). Expanding Options ([`home-options-dark.png`](./docs/ui-redesign/screenshots/before/home-options-dark.png), [`home-options-light.png`](./docs/ui-redesign/screenshots/before/home-options-light.png)) inserts a 4-row card pushing the footer down further. The empty-recents footer stacks `"Open a saved project"`, `"Try a sample clip"`, and shortcut chips vertically as tiny ungrouped links.
- **State & Accessibility Gaps:** No loading skeleton for recents/devices; no search clear button or `"Clear search"` CTA in `.recents-empty`; `.home-recents-head` lacks `flex-wrap` for `< 720px`; `.recent-remove` is `22×22px` (< 24px) and invisible until hover; `.home-more` lacks `aria-controls`; `.recover-card` uses failing `--warn` contrast; `loadAudioDevices`/`loadCameras` fail silently.
- **Screenshots:** [`home-empty-dark.png`](./docs/ui-redesign/screenshots/before/home-empty-dark.png) · [`home-empty-light.png`](./docs/ui-redesign/screenshots/before/home-empty-light.png) · [`home-options-dark.png`](./docs/ui-redesign/screenshots/before/home-options-dark.png) · [`home-options-light.png`](./docs/ui-redesign/screenshots/before/home-options-light.png) · [`home-small-880x560-dark.png`](./docs/ui-redesign/screenshots/before/home-small-880x560-dark.png) · [`home-dpi150-dark.png`](./docs/ui-redesign/screenshots/before/home-dpi150-dark.png)

### 5.2 Titlebar & StatusBar (`Titlebar.tsx`, `StatusBar.tsx`, `WindowControls.tsx`, `shell.css`) — **POLISH**
- **Visual & Layout Findings:** Clean Record + Export anchor points and File dropdown ([`titlebar-file-menu-dark.png`](./docs/ui-redesign/screenshots/before/titlebar-file-menu-dark.png)), but at `880×560` ([`editor-small-880x560-dark.png`](./docs/ui-redesign/screenshots/before/editor-small-880x560-dark.png)) `.tb-search` collapses right up against `.tb-dirty` while `.tb-drag` shrinks near `20px`. Control heights vary across `24px`, `26px`, `28px`, and `30px` on one `44px` bar.
- **State & Accessibility Gaps:** Collapsed `.tb-search` loses its span/kbd text via `display: none` with **no `aria-label`**; Undo/Redo remain enabled on a clean clip (`!ed.hasClip()` check instead of stack depth); Escape on project name doesn't revert edits; StatusBar buttons `.sb-btn` are `20px` tall (< 24px) and `.sb-dot` stays green `--ok` even on errors; `.tb-dirty` & StatusBar facts lack SR descriptions; `WindowControls` omits `type="button"`.
- **Screenshots:** [`titlebar-file-menu-dark.png`](./docs/ui-redesign/screenshots/before/titlebar-file-menu-dark.png) · [`editor-small-880x560-dark.png`](./docs/ui-redesign/screenshots/before/editor-small-880x560-dark.png)

### 5.3 Workspace / Stage & CropEditor (`Stage.tsx`, `CropEditor.tsx`, `stage.css`) — **POLISH** *(Stage)* / **REDESIGN** *(CropEditor)*
- **Visual & Layout Findings:** Container-queried Stage frames clips crisply in both dark ([`editor-clip-dark.png`](./docs/ui-redesign/screenshots/before/editor-clip-dark.png)) and light ([`editor-clip-light.png`](./docs/ui-redesign/screenshots/before/editor-clip-light.png)), and stage context menus ([`editor-context-menu-dark.png`](./docs/ui-redesign/screenshots/before/editor-context-menu-dark.png)) & crop bars ([`editor-crop-dark.png`](./docs/ui-redesign/screenshots/before/editor-crop-dark.png), [`editor-crop-light.png`](./docs/ui-redesign/screenshots/before/editor-crop-light.png)) are legible. However, initial toasts float directly over the top edge of the video canvas, and there is no canvas zoom-to-fit / magnify control for 4K detail inspection.
- **State & Accessibility Gaps:** `CropEditor` uses module-level `lock` state (leaks across cancel/clips), pointer-only `<div>`/`<span>` handles (`10×10px` / `14×14px`, failing 24px target size), and lacks `role="toolbar"` / focus move on `.crop-bar`. Stage SVG elements & fixed zoom reticle are pointer-only with no numeric X/Y zoom focus fields, and `Stage.tsx` hardcodes `#ffd23f`/`#e5484d` draft colors.
- **Screenshots:** [`editor-clip-dark.png`](./docs/ui-redesign/screenshots/before/editor-clip-dark.png) · [`editor-clip-light.png`](./docs/ui-redesign/screenshots/before/editor-clip-light.png) · [`editor-crop-dark.png`](./docs/ui-redesign/screenshots/before/editor-crop-dark.png) · [`editor-crop-light.png`](./docs/ui-redesign/screenshots/before/editor-crop-light.png) · [`editor-context-menu-dark.png`](./docs/ui-redesign/screenshots/before/editor-context-menu-dark.png)

### 5.4 ToolRail (`src/components/ToolRail.tsx`, `src/styles/shell.css`) — **REDESIGN**
- **Visual & Layout Findings (Severe):** In BOTH `1280×800` ([`editor-clip-dark.png`](./docs/ui-redesign/screenshots/before/editor-clip-dark.png)) AND `880×560` ([`editor-small-880x560-dark.png`](./docs/ui-redesign/screenshots/before/editor-small-880x560-dark.png)), **the bottom tools (`Spotlight`, `Mask`, and the Collapse chevron) clip/overflow off-screen with hidden scrollbars** (`scrollbar-width: none`)! Moreover, conflicting `@media (max-height: 780px)` and `(max-height: 820px)` rules override `layout.railCompact()` so clicking the collapse toggle has no visible effect at typical laptop heights.
- **State & Accessibility Gaps:** Locked-tool state (`.rail-lockdot` `aria-hidden="true"`) is silent to screen readers; double-click locking lacks keyboard parity on the rail button; lacks `role="toolbar"` roving arrow-key navigation.
- **Screenshots:** [`editor-clip-dark.png`](./docs/ui-redesign/screenshots/before/editor-clip-dark.png) · [`editor-small-880x560-dark.png`](./docs/ui-redesign/screenshots/before/editor-small-880x560-dark.png) · [`editor-inspector-tool-dark.png`](./docs/ui-redesign/screenshots/before/editor-inspector-tool-dark.png)

### 5.5 Timeline (`src/components/Timeline.tsx`, `Waveform.tsx`, `timeline.css`) — **REDESIGN**
- **Visual & Layout Findings:** Track color-coding and filmstrip thumbs read well ([`editor-inspector-selection-dark.png`](./docs/ui-redesign/screenshots/before/editor-inspector-selection-dark.png)), but `.tl-head` becomes cramped at `880×560` ([`editor-small-880x560-dark.png`](./docs/ui-redesign/screenshots/before/editor-small-880x560-dark.png)) and hides `.tl-rate` / `.tl-time-out` via `display: none` at `≤ 760px` without an overflow menu. `.tl { cursor: text; }` displays an I-beam text cursor across the timeline scrubber!
- **State & Accessibility Gaps:** Trim handles (`.tl-trim` `12px`), segment resize handles (`.tl-handle` `9px`), annotation bars (`.tl-ann` `22px`), and mute button (`.tl-mute` `20×20px`) all fail WCAG 2.5.8 `24×24px`; trim & playhead lack keyboard `role="slider"` controls; Caption bars lack `aria-pressed` and `onContextMenu`; Waveform draws blank while loading; Zoom/Ann vs Speed/Cut/Caption use inconsistent DOM wrappers.
- **Screenshots:** [`editor-clip-dark.png`](./docs/ui-redesign/screenshots/before/editor-clip-dark.png) · [`editor-inspector-selection-dark.png`](./docs/ui-redesign/screenshots/before/editor-inspector-selection-dark.png) · [`editor-small-880x560-dark.png`](./docs/ui-redesign/screenshots/before/editor-small-880x560-dark.png)

### 5.6 Inspector (`src/components/Inspector.tsx`, `ScrubField.tsx`, `inspector.css`) — **REDESIGN**
- **Visual & Layout Findings:** Opening a clip leaves the `Clip` tab showing 9 closed accordions and ~40% blank space below ([`editor-clip-dark.png`](./docs/ui-redesign/screenshots/before/editor-clip-dark.png), [`editor-clip-light.png`](./docs/ui-redesign/screenshots/before/editor-clip-light.png)); arming `Arrow` tool shows 1 lonely `"Draw several"` toggle over 65% empty panel ([`editor-inspector-tool-dark.png`](./docs/ui-redesign/screenshots/before/editor-inspector-tool-dark.png)); and no selection runs an infinite CPU/GPU `float` animation on `.insp-empty-icon`. Selection inspector for Zoom ([`editor-inspector-selection-dark.png`](./docs/ui-redesign/screenshots/before/editor-inspector-selection-dark.png)) is cleaner, though missing X/Y numeric focus fields for `Fixed point`.
- **State & Accessibility Gaps:** Incomplete `tablist` (no roving tabindex, arrow keys, `aria-controls`, `role="tabpanel"`); forced auto-tab switch overrides manual tab clicks; `.clip-search-x` (`22px`), `.switch` (`18px`), `.slider` (`18px`), `.corner-cell` (`23.75px`) fail 24px; `.swatch-custom`/`.color-well` focus is invisible (`opacity: 0` input); `.cap-bar` lacks `role="progressbar"`; multi-select lacks shared property editing; caption status flashes false copy before async check resolves; `Field` hints aren't keyboard reachable.
- **Screenshots:** [`editor-clip-dark.png`](./docs/ui-redesign/screenshots/before/editor-clip-dark.png) · [`editor-inspector-selection-dark.png`](./docs/ui-redesign/screenshots/before/editor-inspector-selection-dark.png) · [`editor-inspector-tool-dark.png`](./docs/ui-redesign/screenshots/before/editor-inspector-tool-dark.png) · [`editor-inspector-search-empty-dark.png`](./docs/ui-redesign/screenshots/before/editor-inspector-search-empty-dark.png)

### 5.7 ExportDialog (`src/ExportDialog.tsx`, `src/styles/dialogs.css`) — **REDESIGN**
- **Visual & Layout Findings:** Clear phase machine (`configure` -> `starting` -> `exporting` -> `done` / `error`), but real renders expose multiple layout defects:
  - In [`export-mp4-dark.png`](./docs/ui-redesign/screenshots/before/export-mp4-dark.png) & [`export-mp4-light.png`](./docs/ui-redesign/screenshots/before/export-mp4-light.png), `Balanced` and `High quality` presets display **identical** `30 fps · 1740px` subtitles on 30fps clips.
  - Format cards wrap descriptions awkwardly (`"Much smaller than GIF, full\ncolor"` orphan word in [`export-webp-dark.png`](./docs/ui-redesign/screenshots/before/export-webp-dark.png)).
  - In [`export-gif-dark.png`](./docs/ui-redesign/screenshots/before/export-gif-dark.png), fixed `78px` `.field-label` forces `"Smooth\ngradients"` onto 2 lines, and `"Fit under [MB] MB [Fit]"` misaligns sharply from the controls above it.
- **State & Accessibility Gaps:** `starting` phase has no spinner; `fitToBudget` lacks abort/cancel handling; `.export-size`, ETA, and copy confirmations lack `aria-live="polite"`; `.export-error` lacks `role="alert"`; `.modal-backdrop` applies full-viewport `blur(3px)`; light themes hit `--ok` `2.59:1` contrast fail on `.ring.done`.
- **Screenshots:** [`export-mp4-dark.png`](./docs/ui-redesign/screenshots/before/export-mp4-dark.png) · [`export-mp4-light.png`](./docs/ui-redesign/screenshots/before/export-mp4-light.png) · [`export-gif-dark.png`](./docs/ui-redesign/screenshots/before/export-gif-dark.png) · [`export-webp-dark.png`](./docs/ui-redesign/screenshots/before/export-webp-dark.png)

### 5.8 Settings (`src/components/Settings.tsx`, `src/styles/dialogs.css`) — **REDESIGN**
- **Visual & Layout Findings:**
  - `.settings-close` is `position: absolute` inside scrolling `.settings-body`, so scrolling down `Recording` or `Shortcuts` loses the close button ([`settings-recording-dark.png`](./docs/ui-redesign/screenshots/before/settings-recording-dark.png), [`settings-shortcuts-dark.png`](./docs/ui-redesign/screenshots/before/settings-shortcuts-dark.png)).
  - Switching tabs leaves the keyboard focus ring stuck on `Appearance` (`settings-recording-dark.png`, `settings-storage-dark.png`, `settings-about-dark.png`).
  - `Recording` bottom prose clips mid-line; `Storage` shows `-` for zero recovery bytes, uses `"Measuring…"` on saveDir, floats `"Open folder"` outside the card, and wipes recovery data with zero confirmation on `Clear` ([`settings-storage-dark.png`](./docs/ui-redesign/screenshots/before/settings-storage-dark.png)); `About` shows a generic film icon instead of Vuoom's `LogoMark` ([`settings-about-dark.png`](./docs/ui-redesign/screenshots/before/settings-about-dark.png)); `Appearance` lacks a `"System"` theme card and forces 5-column `.theme-grid` ([`settings-appearance-dark.png`](./docs/ui-redesign/screenshots/before/settings-appearance-dark.png), [`settings-appearance-light.png`](./docs/ui-redesign/screenshots/before/settings-appearance-light.png)).
- **State & Accessibility Gaps:** `HotkeyField` traps `Tab` during listening without saying `"Esc cancels"` and renders `.hotkey-problem` in muted gray instead of `--record`; 6 `Seg` controls omit `label`; `Language` `Seg` triggers sync focus race (`ui.tsx:80`).
- **Screenshots:** [`settings-appearance-dark.png`](./docs/ui-redesign/screenshots/before/settings-appearance-dark.png) · [`settings-appearance-light.png`](./docs/ui-redesign/screenshots/before/settings-appearance-light.png) · [`settings-recording-dark.png`](./docs/ui-redesign/screenshots/before/settings-recording-dark.png) · [`settings-editing-dark.png`](./docs/ui-redesign/screenshots/before/settings-editing-dark.png) · [`settings-storage-dark.png`](./docs/ui-redesign/screenshots/before/settings-storage-dark.png) · [`settings-shortcuts-dark.png`](./docs/ui-redesign/screenshots/before/settings-shortcuts-dark.png) · [`settings-about-dark.png`](./docs/ui-redesign/screenshots/before/settings-about-dark.png)

### 5.9 CommandPalette (`src/components/CommandPalette.tsx`, `dialogs.css`) — **POLISH**
- **Visual & Layout Findings:** Visually crisp in both dark and light modes ([`command-palette-dark.png`](./docs/ui-redesign/screenshots/before/command-palette-dark.png), [`command-palette-light.png`](./docs/ui-redesign/screenshots/before/command-palette-light.png), [`command-palette-empty-dark.png`](./docs/ui-redesign/screenshots/before/command-palette-empty-dark.png)). Minor polish: collapses modal height abruptly on empty query, lacks a bottom keycap navigation hint strip, uses two different `<kbd>` styles in one view, and hardcodes `"cursor"` icon for all tool commands (`CommandPalette.tsx:97`).
- **Accessibility Gaps:** Broken WAI-ARIA combobox (`<input>` lacks `role="combobox"` / `aria-activedescendant`; `.pal-item` lacks `id` and `tabindex="-1"`, so `Tab` walks 40+ items and breaks arrow keys); `onPointerMove` hijacks active row during scroll.
- **Screenshots:** [`command-palette-dark.png`](./docs/ui-redesign/screenshots/before/command-palette-dark.png) · [`command-palette-light.png`](./docs/ui-redesign/screenshots/before/command-palette-light.png) · [`command-palette-empty-dark.png`](./docs/ui-redesign/screenshots/before/command-palette-empty-dark.png)

### 5.10 SourcePicker & WindowPicker (`SourcePicker.tsx`, `WindowPicker.tsx`) — **REDESIGN**
- **Visual & Layout Findings:** `SourcePicker` ([`source-picker-dark.png`](./docs/ui-redesign/screenshots/before/source-picker-dark.png), [`source-picker-light.png`](./docs/ui-redesign/screenshots/before/source-picker-light.png)) gives every window the same generic icon with no search/filter or Refresh button. `WindowPicker` ([`window-picker-dark.png`](./docs/ui-redesign/screenshots/before/window-picker-dark.png)) clips `.sel-dimtag` at right screen edges, has no empty-windows or null-backdrop guidance, and has **no `dialogA11y` focus trap or keyboard window cycling**.
- **Screenshots:** [`source-picker-dark.png`](./docs/ui-redesign/screenshots/before/source-picker-dark.png) · [`source-picker-light.png`](./docs/ui-redesign/screenshots/before/source-picker-light.png) · [`window-picker-dark.png`](./docs/ui-redesign/screenshots/before/window-picker-dark.png)

### 5.11 RecordOverlay (`src/RecordOverlay.tsx`, `src/RecordOverlay.css`) — **REDESIGN**
- **Visual & Layout Findings:** Selector HUD ([`record-overlay-select-dark.png`](./docs/ui-redesign/screenshots/before/record-overlay-select-dark.png), [`record-overlay-region-dark.png`](./docs/ui-redesign/screenshots/before/record-overlay-region-dark.png)) packs 12 controls onto a wrapping bottom bar that overlaps low/16:9 selections (as directly visible in `record-overlay-region-dark.png`!) and uses `backdrop-filter: blur(18px)` + `100vmax` spread shadows during every pointer drag. Light-theme `--focus` paints dark rings over dark glass.
- **State & Accessibility Gaps:** `.hud-cancel` lacks `aria-label`; no `dialogA11y` focus trap across Selector/Countdown/StopBar; no keyboard nudge/resize for selection box; no `aria-live` countdown/finalizing announcements; silent failures when WebSocket preview or camera fails; `.hud-heavy` (`Use 30 fps`) is `~21px` tall (< 24px).
- **Screenshots:** [`record-overlay-select-dark.png`](./docs/ui-redesign/screenshots/before/record-overlay-select-dark.png) · [`record-overlay-region-dark.png`](./docs/ui-redesign/screenshots/before/record-overlay-region-dark.png)

### 5.12 Welcome, Coachmark, Permissions & Busy Veil (`Welcome.tsx`, `Permissions.tsx`, `App.tsx`) — **POLISH**
- **Visual & Layout Findings:** Live [`busy-loading-dark.png`](./docs/ui-redesign/screenshots/before/busy-loading-dark.png) reveals `.busy-veil` renders a bare spinner + text directly over dimmed Home cards (`Full screen` collides right behind the spinner) with no card or pill container and no focus trap/`inert` background. `Welcome` (`Welcome.tsx`) omits `<540px` wrapping and focuses `"Look around first"` instead of `"Start recording"`. `PermissionsCard` (`Permissions.tsx`) omits `input` monitoring permission, flattens `.home-perm` on `--panel`, repeats identical `"Allow"` labels, and treats IPC errors as granted.
- **Screenshots:** [`busy-loading-dark.png`](./docs/ui-redesign/screenshots/before/busy-loading-dark.png)
