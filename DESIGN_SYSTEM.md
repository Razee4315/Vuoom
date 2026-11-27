# Vuoom Design System Specification (Phase U3)

**Chosen Direction:** **Direction A — Tungsten Darkroom & Daylight Archival Studio** *(co-designed with Codex GPT-6.2 Astra; see [`DECISIONS.md`](./DECISIONS.md) D-015)*.  
**Scope:** Native desktop UI across Windows 11 and macOS 14+ (Tauri v2 + SolidJS 1.9 + pure CSS custom properties).  
**Standards:** Strict WCAG 2.2 AA (`>= 4.5:1` text, `>= 3.0:1` UI/lines, `>= 24×24px` interactive targets), zero-flicker startup, `880×560` minimum window to fullscreen, crisp at `100%`, `125%`, `150%`, and `200%` DPI.

---

## 1. Design Principles

1. **Stage First & Color-Neutral:** Chrome recedes so captured footage stays accurate. Tungsten warmth lives in chrome (`--backdrop`, `--panel`, `--raised`, `--rim`); the editing `--stage-bg` stays strictly achromatic neutral gray (`#181818` dark / `#D8D8D8` light) so users never misread color temperature in their recordings.
2. **One Accent Means Capture:** Saturated Record Crimson (`#C62F38`) is reserved exclusively for recording/stop fills. Ordinary selection uses `--ink`, focus uses `--focus-ring`, and Timeline tracks own distinct color-coded hues + glyphs. **Zero purple anywhere.**
3. **Tactile Clarity Without Blur Tax:** Depth comes from an opaque luminance ladder, `1px` functional hairlines, and a `1px` top specular `--rim` on raised groups and floating islands — never expensive `backdrop-filter` blurs over live `<canvas>` video or `100vmax` drag spread shadows.
4. **Keyboard & Target Parity Everywhere:** Every interactive target maintains `>= 24×24px` actual hit area (via layout or expanded `::before` pad), every composite widget uses roving focus (`Tab` to enter, `Arrows` to navigate), and every drag interaction has a keyboard and/or numeric equivalent.
5. **Calm at Rest, Dense When Editing:** Empty states teach cleanly with 1-line guidance and an original geometric motif; active editing reveals precision tabular numeric fields (`ScrubField`), collapsible sections with live summary badges, and clean spatial orientation.

---

## 2. Theme Preference Contract & Zero-Flicker Startup

### 2.1 Available Themes & Legacy Migration
- **Supported `localStorage["vuoom-theme"]` values:**
  - `"system"` **(default for new installs)** — resolves to `"tungsten"` when OS prefers dark, `"daylight"` when OS prefers light.
  - `"tungsten"` — **Tungsten Darkroom** (flagship dark theme).
  - `"daylight"` — **Daylight Archival Studio** (flagship light theme).
  - `"mono-dark"` — Achromatic dark.
  - `"mono-light"` — Achromatic light.
  - `"graphite"` — Cool slate dark.
  - `"midnight"` — Deep navy dark.
- **Legacy Alias:** `"paper"` resolves to the full `"daylight"` palette seamlessly so existing users keep light mode and see `Daylight Archival` selected without losing their preference string until changed.
- **Separation of Preference vs. Resolved Palette:**
  - `themePref()` stores the user's saved choice (`"system" | ThemeId`).
  - `resolvedTheme()` stores the computed palette (`"tungsten" | "daylight" | "mono-dark" | "mono-light" | "graphite" | "midnight"`).
  - `?theme=` query overrides and Settings preview miniatures never overwrite `localStorage["vuoom-theme"]`.

### 2.2 Synchronous `<head>` Initialization & System Listener
- An inline blocking `<script>` in `index.html` `<head>` runs before `#splash` renders:
  1. Checks `?theme=` override, then `localStorage.getItem("vuoom-theme")`, falling back to `"system"`.
  2. Resolves `"system"` via `window.matchMedia("(prefers-color-scheme: dark)").matches`.
  3. Sets `document.documentElement.dataset.theme`, `document.documentElement.style.colorScheme` (`"dark"` or `"light"`), matching `#splash` background (`#110F0E` Tungsten / `#F4F0E8` Daylight / palette backdrop), and `<meta name="theme-color">`.
- `index.html` splash CSS also includes a `@media (prefers-color-scheme: light)` fallback when `localStorage` is blocked.
- In `src/themes.ts`, when `themePref() === "system"`, subscribe once to `matchMedia("(prefers-color-scheme: dark)")` `'change'` and detach cleanly on theme change or `onCleanup`.

### 2.3 Inactive-Window Dimming & Scoped Preview Containers
- Drive `:root[data-window-focused="false"]` from Tauri's window focus event: subtly step down Titlebar title ink (`var(--text-2)`) and mute decorative `--rim` highlights while preserving full body text readability and focus rings.
- All theme palette selectors match `[data-theme="..."]` on **both** `:root` and scoped preview containers (e.g., Settings theme thumbnail cards), guaranteeing light miniatures inside dark Settings render authentic light foregrounds/fills.

---

## 3. Color Tokens & WCAG 2.2 AA Verification

### 3.1 Palette Tokens (All 6 Palettes; `#` omitted in header, hex values exact)
*(Note: `"paper"` inherits the exact `"daylight"` column; `"system"` resolves to `"tungsten"` or `"daylight"`.)*

| Token | Role | Tungsten (`dark`) | Daylight (`light`) | Mono Dark | Mono Light | Graphite | Midnight |
|---|---|---:|---:|---:|---:|---:|---:|
| `--backdrop` | App frame / StatusBar | `#110F0E` | `#F4F0E8` | `#0B0B0B` | `#F1F1F1` | `#111518` | `#090F17` |
| `--panel` | Docked panels / modals | `#171412` | `#FFFCF5` | `#121212` | `#FFFFFF` | `#181D21` | `#101823` |
| `--panel-2` | Secondary panel / menus | `#1D1916` | `#F7F3EB` | `#191919` | `#F8F8F8` | `#1D2328` | `#17202B` |
| `--raised` | Cards / inputs / controls | `#221E1A` | `#FFFFFF` | `#212121` | `#FCFCFC` | `#232A30` | `#1C2734` |
| `--hover` | Control hover surface | `#2B2520` | `#EFE9DE` | `#292929` | `#EAEAEA` | `#293239` | `#23303E` |
| `--active` | Control pressed / active thumb | `#342C26` | `#E6DFD3` | `#323232` | `#E1E1E1` | `#303940` | `#293746` |
| `--line` | Subtle structural divider (`>=3:1`) | `#82766A` | `#81766A` | `#808080` | `#7C7C7C` | `#8798A3` | `#8294A5` |
| `--line-2` | Control / card border | `#978A7D` | `#73685D` | `#909090` | `#6D6D6D` | `#98A8B2` | `#94A5B5` |
| `--line-strong` | Active / focused border | `#B4A392` | `#5F554A` | `#A6A6A6` | `#5C5C5C` | `#B0BFC8` | `#AABACA` |
| `--rim` | Decorative 1px top specular edge | `rgba(255,229,189,.22)` | `rgba(255,255,255,.92)` | `rgba(255,255,255,.14)` | `rgba(255,255,255,.92)` | `rgba(255,255,255,.14)` | `rgba(255,255,255,.14)` |
| `--text` | Primary copy & headings | `#F6EFE5` | `#27221D` | `#F5F5F5` | `#151515` | `#EAF0F4` | `#EAF1F7` |
| `--text-2` | Secondary copy & values | `#DACEC1` | `#453D35` | `#D6D6D6` | `#393939` | `#D1DCE3` | `#CFDBE6` |
| `--muted` | Labels, metadata, captions | `#BBAEA1` | `#5F5449` | `#B8B8B8` | `#555555` | `#B5C3CC` | `#B4C3D2` |
| `--faint` | Hints, placeholders, group tags | `#AB9F93` | `#685C50` | `#A5A5A5` | `#606060` | `#A8B6C0` | `#A7B7C7` |
| `--ink` | Selected control / CTA fill | `#F6EFE5` | `#312A24` | `#F5F5F5` | `#222222` | `#EAF0F4` | `#EAF1F7` |
| `--ink-text` | Text/icons on `--ink` fill | `#171412` | `#FFFCF5` | `#121212` | `#FFFFFF` | `#181D21` | `#101823` |
| `--accent` | Precision / active highlights | `#E4AD73` | `#77430F` | `#F5F5F5` | `#151515` | `#EAF0F4` | `#EAF1F7` |
| `--focus-ring` | Keyboard `:focus-visible` ring | `#FFC680` | `#76420F` | `#F5F5F5` | `#151515` | `#EAF0F4` | `#EAF1F7` |
| `--stage-bg` | Neutral editing video matte | `#181818` | `#D8D8D8` | `#181818` | `#D8D8D8` | `#181818` | `#181818` |
| `--stage-dot` | Peripheral matte dot pattern | `#303030` | `#C3C3C3` | `#303030` | `#C3C3C3` | `#303030` | `#303030` |

> **Subtle Structural Borders:** While `--line` / `--line-2` / `--line-strong` achieve `>= 3.0:1` for interactive control boundaries, group containers rely primarily on surface-step contrast, alignment, and `8–12px` spacing (using `color-mix(in srgb, var(--line) 28%, transparent)` only for non-interactive layout hairlines) so panels remain calm without high-contrast wireframe boxes.

### 3.2 Semantic & Timeline Track Tokens

| Token | Dark Palettes | Light Palettes | Purpose |
|---|---|---|---|
| `--record` | `#C62F38` | `#C62F38` | Record & Stop button fills ONLY (`5.43:1` w/ `#FFFFFF`) |
| `--record-hover` | `#D13740` | `#D13740` | Record hover fill (`4.85:1` w/ `#FFFFFF`) |
| `--record-pressed` | `#AF2630` | `#AF2630` | Record pressed fill (`6.68:1` w/ `#FFFFFF`) |
| `--on-record` | `#FFFFFF` | `#FFFFFF` | Foreground text & glyph on crimson fills |
| `--record-text` | `#FF9497` | `#9D2733` | Crimson labels, errors, danger text, live rec indicators |
| `--ok` | `#75D3A0` | `#165D38` | Success state text/icons |
| `--warn` | `#F4C36F` | `#754800` | Warning & crash recovery text/icons |
| `--t-zoom` | `#FF9386` | `#A13B33` | Zoom track & segment accent |
| `--t-note` | `#93BFFF` | `#285598` | Annotations track accent |
| `--t-speed` | `#F4BC6B` | `#78500A` | Speed region track accent |
| `--t-cut` | `#EAA4A8` | `#863D46` | Cut track accent |
| `--t-audio` | `#71D2C5` | `#156156` | Audio track & meter accent |
| `--t-box` | `#FFDA82` | `#6D5300` | Shape / Highlight accent |
| `--t-caption` | `#B7D996` | `#426523` | Captions track accent |

*(Soft washes derive automatically via `color-mix(in srgb, var(--token) 14%, transparent)`, and `#E5484D` remains purely decorative inside `LogoMark` / brand dots.)*

### 3.3 Mathematically Verified WCAG 2.2 AA Contrast Floors
Minimum contrast ratio (`CR = (L1 + 0.05) / (L2 + 0.05)`) across ALL six surface steps (`--backdrop` through `--active`):

| Palette | Min Text (`--text`..`--faint`) | Min Semantic (`--record-text`, `--accent`, `--ok`, `--warn`, `--focus-ring`) | Min Track (`--t-*`) | Min Lines (`--line`..`--line-strong`) | `--ink-text` on `--ink` |
|---|---:|---:|---:|---:|---:|
| **Tungsten** | **5.29:1** | **6.48:1** | **6.37:1** | **3.10:1** | **16.06:1** |
| **Daylight / Paper** | **4.90:1** | **5.72:1** | **4.97:1** | **3.35:1** | **13.78:1** |
| **Mono Dark** | **5.21:1** | **6.07:1** | **5.97:1** | **3.25:1** | **17.18:1** |
| **Mono Light** | **4.81:1** | **5.79:1** | **5.04:1** | **3.19:1** | **15.91:1** |
| **Graphite** | **5.67:1** | **5.57:1** | **5.47:1** | **3.95:1** | **14.77:1** |
| **Midnight** | **5.92:1** | **5.75:1** | **5.65:1** | **3.89:1** | **15.65:1** |

- **`--on-record` (`#FFFFFF`) on Crimson Fills:** **5.43:1** rest, **4.85:1** hover, **6.68:1** pressed across every theme.
- **Stage Double-Keyline Overlays:** Focus reticles, crop outlines, and selection outlines use a black-and-white double stroke so visibility holds over both light and dark footage.

---

## 4. Typography, Spacing, Radii & Elevation

### 4.1 Typography
- **Font Stack:** `"Inter Variable"` (Latin subset only, `-85 KB` to `-170 KB` bundle reduction) with immediate fallback `system-ui, -apple-system, "Segoe UI", "PingFang SC", "Microsoft YaHei UI", sans-serif`.
- **Annotation Font Parity:** Fix `"Inter, sans-serif"` -> `"Inter Variable"` resolution in `src/editor/constants.ts` and `src/mock/engine.ts` without altering user-authored project typography or line-wrap metrics in exports.
- **4-Weight Scale (eliminating 13 erratic weights):**
  - `400` — Body copy, values, muted helper text
  - `500` — Buttons, tabs, labels, menu items, controls
  - `600` — Section headers, Inspector/Settings subheadings, badges
  - `700` — Home hero headline, modal/dialog titles
- **Type Ramp (`size / line-height` in CSS px):**
  - `--fs-xs`: `12px / 16px` (metadata, hints, timeline ruler, badges)
  - `--fs-sm`: `13px / 18px` (controls, inspector labels, menus, statusbar)
  - `--fs-md`: `14px / 20px` (body prose, command items, settings rows)
  - `--fs-lg`: `16px / 22px` (section headings, card titles)
  - `--fs-xl`: `20px / 26px` (dialog titles, large readouts)
  - `--fs-2xl`: `28px / 34px` (Home hero headline)
- **Tabular Numerals:** Enforce `font-variant-numeric: tabular-nums` on timecodes, FPS, dimensions, sliders, `ScrubField`, ETA, and estimated byte readouts.

### 4.2 Spacing, Radii, Elevation & CSS Layers
- **4px Base Grid (`--s-1`..`--s-7`):** `4px`, `8px`, `12px`, `16px`, `24px`, `32px`, `48px`.
- **Corner Radii:**
  - `--r-xs: 4px` (compact badges, `<kbd>`, handles)
  - `--r-sm: 6px` (buttons, inputs, tabs, icon buttons)
  - `--r-md: 8px` (cards, menus, floating islands, HUDs)
  - `--r-lg: 12px` (modals, Settings, ExportDialog)
  - `--r-full: 999px` (switches, status dots)
- **3-Tier Elevation (Zero Heavy Blur):**
  1. **Flat Matte:** Docked panels (`--panel` + subtle 1px border).
  2. **Raised Card / Island:** `--raised` or `--panel-2` + `inset 0 1px 0 var(--rim)` + functional border.
  3. **Floating Menu / Dialog:** `--panel-2` + `inset 0 1px 0 var(--rim)` + `0 4px 12px rgba(0, 0, 0, 0.20)`.
- **Cascade Order:** `@layer tokens, base, components, screens, motion;` — zero `!important` overrides.

---

## 5. Layout Grids & Redesigned Screen Architecture

### 5.1 Home (`REDESIGN`)
- **Desktop (`1280×800`):** Top-aligned 2-column grid beneath the 44px Titlebar (`32px` outer padding, `24px` column gutter). Left column (`384px` fixed): Hero headline, `Region`/`Full screen`/`Window` (`64px` high full-width action cards with icon, title, subtitle, capture affordance + crimson active indicator), `36px` setup strip (`Audio · Camera · Options` anchored popovers + inline enumeration error/Retry). Right column (`1fr`): `Permissions` (macOS) / `Recover` card (when present) + `Recent projects` (`repeat(auto-fit, minmax(176px, 1fr))` 16:9 cards with `28×28px` focus-accessible Remove button) or `ContactSheetArt` empty state + **Open project** & **Try sample clip** actions. Fits cleanly with **zero page scroll** when empty; long recents list scrolls inside its own column.
- **Minimum Window (`880×560`):** `16px` outer padding, `16px` gutter, `312px` capture column.

### 5.2 Editor Shell (`Titlebar`, `ToolRail`, `Stage`, `CropEditor`, `Timeline`, `Inspector`, `StatusBar`)
- **3-Column Grid:** Full-height left **`ToolRail`** (`104px` labeled / `48px` compact) | center flex column (**`Stage`** + **`Timeline`**) | full-height right **`Inspector`** (`312px` default, `260px` min). Moving `ToolRail` out of `.upper` guarantees all 11 tools + collapse button (`416px` at `32px` row height) always fit inside `488px` at `880×560` without clipping!
- **Titlebar (`44px`) & StatusBar (`28px`):** Uniform `28px` Titlebar controls, shrinkable `projectName` + `12px` dirty dot slot, `>= 48px` drag track; below `1080px` Search becomes a named `28px` icon; below `960px` panel toggles/Settings/Update group into a named `Workspace` menu. Mac reserves `84px` left traffic-light inset; Windows reserves right caption controls. StatusBar uses `>= 24px` buttons inside `28px` height, separates success/busy/error dots + icons, and removes playback ticks from live regions.
- **Stage (`POLISH`) & CropEditor (`REDESIGN`):** Neutral `--stage-bg`, Fit / 100% view toggle, off-canvas Toast anchor (never blocking top of video), numeric X/Y `ScrubField`s for fixed zoom focus. `CropEditor` uses 4 bounded dim rectangles (zero `100vmax` shadow), 8px handles inside `24×24px` hit pads, session-scoped aspect lock, opaque `36px` `CropBar`, numeric X/Y/W/H fields, and arrow-key stepping (`1px`, `Shift = 10px`).
- **Timeline (`REDESIGN`):** Initial height `300px` (`172px` effective at `560px` window height, reserving `>= 180px` Stage). `40px` header (`Transport / Insert / View`) responsive to *Timeline container width* (`<= 760px` groups Zoom/Speed/Cut under `Insert`, shortens timecode, folds secondary view switches cleanly). Lane default cursor (`default`), segment `grab`/`grabbing`, edges `ew-resize`, text `text`. `28px` lanes, `6px` visible handles inside `24×24px` hit targets, elevated separated dual-handle strip for ultra-short segments/trims, keyboard `role="slider"` playhead & trim handles, numeric Start/End entry, static waveform loading baseline, and Caption `aria-pressed` + context menu.
- **Inspector (`REDESIGN`):** Pinned `36px` `Selection | Clip` tablist (`role="tablist"` + roving focus + manual tab lock preserved) + pinned `32px` search bar above scroll area. Opens a fresh clip with a summary card + **Frame** expanded and remaining sections cleanly grouped (*Framing*, *Motion*, *Sound & Text*, *Presentation*, *Clip Facts*) with meaningful summary badges. Armed tool mode shows actual tool glyph + placement instruction + gesture preview + pre-draw controls + keyboard `"Place..."` action + list of existing clip annotations below; true empty selection renders `SelectionHandlesArt`.

### 5.3 Modals & Recording Flows (`Settings`, `ExportDialog`, `SourcePicker`, `WindowPicker`, `RecordOverlay`, `CommandPalette`, `Welcome`, `BusyVeil`)
- **Settings (`REDESIGN`):** Max `760px` width, pinned `48px` header + named Close button, `152px` left category rail, single scrolling pane. Theme cards use scoped `[data-theme]` CSS miniatures with `System` resolution readout; `Storage` separates path, loading, and `0 B` + places `Open folder` inline and confirms `Clear recovery`; `Shortcuts` lets `Tab` exit capture and `Esc` cancel while rendering errors in `--record-text`; `About` uses `StudioEmblem` (`LogoMark`).
- **ExportDialog (`REDESIGN`):** `640px` modal, uniform 3-up Format cards with balanced copy, non-duplicate Preset subtitles (showing `"1740 px · 30 fps"` once under Output when Balanced/HQ share dimensions, differentiating `"Quality 85"` vs `"Quality 95"`), consistent `minmax(128px,max-content) minmax(0,1fr) 72px` field grid keeping `"Smooth gradients"` on 1 line and `"Fit under | [10 MB] | Fit"` aligned cleanly, `fitToBudget` abort controller, spinner in `starting`, `aria-live` size/ETA/copy, and `role="alert"` error box.
- **SourcePicker & WindowPicker (`REDESIGN`):** `SourcePicker` gets pinned search + `28px` Refresh + `WindowOutlineArt` empty state + clear distinction of `"No windows"` / `"No matches"` / `"Preview unavailable"`. `WindowPicker` gets `dialogA11y`, arrow-key window cycling, `Enter` select, and 4-edge 8px-inset clamped `.sel-dimtag`.
- **RecordOverlay (`REDESIGN`):** Single-row opaque `44px` HUD (`32px` controls, collapsing Audio/Camera/Options into one `Setup` menu on narrower screens, auto-positioning clear of the active selection/drag handle), 4 bounded dim rects, arrow-key region nudge/resize + numeric inputs, `aria-label` on `.hud-cancel`, `dialogA11y` focus management, `aria-live` countdown/finalizing, and distinct `preview unavailable` / `camera unavailable` / `recording failed` states.
- **CommandPalette / Welcome / Permissions / BusyVeil (`POLISH`):** Fixed-height palette with `role="combobox"` + `aria-activedescendant` + `tabindex="-1"` options + Raycast footer bar; Welcome focuses `Start recording` first and uses `StudioEmblem`; Permissions surfaces macOS `input` + distinct pending/denied/error states; `BusyVeil` renders a centered opaque card over an `inert` backdrop.

---

## 6. Iconography & Custom Visual Assets

- **Icons (`src/icons.tsx`):** Keep Vuoom's curated zero-dependency `24×24` SVG path map (`1.75px` round stroke, `currentColor`, rendered at `16px`, `20px`, or `24px`), removing duplicate glyphs (`keyboard`/`keys`, `bringForward`/`sendBackward`) and fixing camera vs audio glyph/color reuse.
- **Commissioned Custom Assets (`src/components/StudioArt.tsx`, GPT-6.2 Astra):**
  1. `ContactSheetArt` (`144×80`, `<= 3.5 KB`) — Empty Home library
  2. `SelectionHandlesArt` (`80×56`, `<= 1.5 KB`) — Inspector empty selection
  3. `StudioEmblem` (`64×64`, `<= 2.0 KB` incremental) — Welcome & Settings About
  4. `WindowOutlineArt` (`96×64`, `<= 2.0 KB`) — Empty SourcePicker/WindowPicker
  *(Combined SVG payload ceiling: `<= 9 KB` raw; strictly theme-token driven; decorative `aria-hidden="true"`.)*

---

## 7. Consolidated Component Primitives & 8-State Matrix

Consolidate `src/ui.tsx` + stylesheets around:
- **Actions & Pickers:** `Button`, `IconButton`, `PickerTrigger` (one unified size scale: `28px` compact, `32px` standard, `36px` modal primary)
- **Navigation & Selection:** `Seg` (replaces `.toggle-group`, `.crop-grid`, `.crop-locks`, `.hud-seg`), `Tabs`, `Toolbar` (sharing `createRovingTabindex`), `ChoiceCard`, `ActionCard`
- **Forms & Inputs:** `Field` (associated `<label>` + keyboard-accessible hint), `Section`, `Input`, `Select`, `Switch` (`>= 24px` hit wrapper), `Slider` (`>= 24px` hit track), `ScrubField` (`tabular-nums`), `SwatchGroup` (`aria-pressed` + `:has(:focus-visible)` ring)
- **Containers & Feedback:** `Modal` / `ModalHeader` / `ModalBody` / `ModalFooter` (via hardened `dialogA11y` with `inert` + safe opener return), `Island`, `Menu` / `ContextMenu`, `Tooltip`, `Toast`, `Kbd`, `EmptyState`, `Banner`, `Progress`, `Skeleton`

| State | Visual & Interaction Contract |
|---|---|
| **Default** | Opaque surface, readable `>= 4.5:1` label, `>= 3:1` border where needed, `>= 24×24px` real hit area. |
| **Hover** | `--hover` surface (or `--record-hover` `#D13740` for crimson buttons); tooltip triggers after delay; never shifts layout. |
| **Pressed** | `--active` surface (or `--record-pressed` `#AF2630`); `scale(0.98)` via compositor without changing bounding layout. |
| **Selected / Locked** | Persistent check mark, `--ink` bar/fill, or lock badge + `aria-pressed` / `aria-selected` / `aria-current`; independent of keyboard focus. |
| **Focus-Visible** | Exclusive `2px solid var(--focus-ring)` with `2px` surface gap; zero mouse-click focus rings on buttons/inputs (`:focus` stays quiet); `forced-colors` system outline support. |
| **Disabled** | Native `disabled` where applicable, readable muted label, flattened surface (no elevation rim), group-level reason hint when helpful. |
| **Loading** | Width preserved (`aria-busy="true"`), duplicate clicks blocked; pure-CSS `150ms`-delayed skeleton for short IPC; bounded indicator for active jobs; static under reduced motion. |
| **Error** | `--record-text` + icon + plain-language explanation + actionable recovery button; associated via `aria-describedby` / `role="alert"`. |
