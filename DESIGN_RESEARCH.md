# Vuoom Design Research & Open-Source Architecture Study (Phase U2)

**Target Stack:** SolidJS 1.9 + TypeScript + Vanilla CSS Custom Properties + Tauri v2 (`Apache-2.0`)  
**Performance & Bundle Guardrails:** `dist/ <= 1,478 KB raw / 694 KB gz`, `Cold start <= 2,050 ms`, `First usable UI <= 500 ms`, `Rust idle CPU <= 0.50% / 0.10%`, `0` heavy `backdrop-filter` blurs over live video canvases.

> **Originality & Asset Rules:** All references below are studied strictly for interaction patterns, visual hierarchy, and token/CSS architecture. Vuoom will never copy another product screen-for-screen or incorporate third-party logos, illustrations, icons, or proprietary assets. Custom illustrations, empty-state art, and bespoke icons will be generated with **Codex (GPT-6.2 Astra)** or drawn in Vuoom's license-clean SVG system.

---

## 1. Brand Alignment Opportunity: Vuoom Website vs. Desktop App

Inspecting Vuoom's own [`website/docs/17-tungsten-darkroom-redesign.md`](./website/docs/17-tungsten-darkroom-redesign.md), [`website/src/styles/tokens.css`](./website/src/styles/tokens.css), and [`website/src/layouts/Base.astro`](./website/src/layouts/Base.astro) alongside the current `v2.4.0` desktop shell reveals an immediate opportunity to unify Vuoom's product identity:

| Dimension | Website (`Tungsten Darkroom` / `Daylight Archival Studio`) | Current Desktop App (`v2.4.0`) | Redesign Alignment Opportunity |
|---|---|---|---|
| **Dark & Light Mood** | Warm smoked obsidian (`#0b0a09` -> `#110f0e` -> `#171412` -> `#221e1a`), warm parchment ink (`#f6efe5`); Daylight Archival `#f4f0e8` ground & `#fffcf5` raised cards. | Cool neutral charcoal (`mono-dark`: `#08080a` -> `#111114` -> `#16161a` -> `#1c1c21`); generic stark `#ffffff` / `#ececf0` light theme. | Offer **Tungsten Darkroom** & **Daylight Archival Studio** alongside clean Monochrome (`Mono Dark`/`Mono Light`) and **System** auto-switching so desktop & website share one unmistakable studio voice. |
| **Tactile Rim Depth** | 1px top-edge specular rim (`--rim: rgb(255 229 189 / 0.28)` dark / `rgb(255 255 255 / 0.92)` light) inside `inset 0 1px 0 var(--rim)`. | Generic `rgba(255,255,255,0.03)` inset shadow; heavy `backdrop-filter` blur on HUDs. | Use semantic `--rim` top-edge highlights and 1px hairline borders on panels, islands, and cards instead of expensive `backdrop-filter` blurs over video. |
| **Color & Zero Purple** | Record Crimson (`#e5484d`), Warm Tungsten (`#e4a45f` / `#9a571f`), Emerald (`#4fb37f` / `#1f7a4d`), zero purple. | Record `#e5484d` + inverted `tokens.css:220-230` light/dark override bug failing contrast on light themes (`1.48–2.59:1`). | Fix light/dark accent pairing mathematically (`>= 4.5:1` AA), introduce warm `--tungsten` focus/highlight accents where helpful, and keep purple out of UI chrome. |
| **Startup & System Sync** | Inline `<script>` in `<head>` paints theme before hydration. | Hardcoded `#0e0e0f` dark splash in `index.html`; no `prefers-color-scheme` support. | Add a blocking `<head>` script in `index.html` resolving `localStorage` + `prefers-color-scheme` so light-mode users never see a dark startup flash. |

---

## 2. Visual Design & Desktop UX References (15 Curated References)

### Part A: Desktop Creative & Pro-Tool Apps (8 References)

#### 1. Screen Studio
- **URL:** <https://screen.studio/>
- **Category:** Desktop App (macOS Screen Recorder & Editor)
- **What Is Good About It:** Opens straight into a matte dark stage where the framed recording is the hero; represents automated zooms as clean draggable blocks on a low-clutter timeline; exposes a concise top-right Export flow (format, resolution, FPS, compression) without NLE clutter.
- **Takeaways for Vuoom:** Elevate the **Stage** in a quiet neutral canvas; keep zoom/speed/cut segments visually tactile on the Timeline with clear hover readouts; surface a streamlined **ExportDialog** without identical redundant presets.

#### 2. CleanShot X
- **URL:** <https://cleanshot.com/>
- **Category:** Desktop App (Capture & Annotation Tool)
- **What Is Good About It:** Compact pre-capture floating HUD combining capture area, audio, camera, and overlay toggles without wrapping into awkward multi-row bars; sharp dimension tags around selection boxes; clean icon-topped Preferences tabs with inline shortcut capture.
- **Takeaways for Vuoom:** Streamline **RecordOverlay**'s bottom selector HUD so it fits cleanly on 1280px screens without overlapping 16:9 selections; render clean corner/edge selection readouts that never clip outside viewport bounds.

#### 3. Cap (Cap.so Desktop)
- **URL:** <https://cap.so/>
- **Category:** Desktop App (Open Screen Recorder)
- **What Is Good About It:** Unmistakable primary action on the launcher ("New Recording") paired with clean 16:9 local project cards showing duration, resolution badges, and quick hover actions; uncluttered multi-track timeline isolating screen, camera, and audio lanes.
- **Takeaways for Vuoom:** Redesign **Home** with a stronger spatial balance between primary capture actions, quick-configuration pills, and rich recent-project cards instead of tiny floating footer links.

#### 4. Loom Desktop App
- **URL:** <https://www.loom.com>
- **Category:** Desktop App (Floating Capture HUD)
- **What Is Good About It:** Compact draggable recording StopBar excluded from capture; unmistakable recording vs paused visual states with tabular monospace elapsed clock and global hotkey badges inside tooltips.
- **Takeaways for Vuoom:** Keep Vuoom's **StopBar / MiniBar** calm, crisp, and failure-transparent (surfacing camera/preview errors rather than silently showing black frames), with platform-aware hotkey hints.

#### 5. DaVinci Resolve (Cut Page)
- **URL:** <https://www.blackmagicdesign.com/products/davinciresolve/cut>
- **Category:** Desktop App (Non-Linear Video Editor)
- **What Is Good About It:** Neutral charcoal chrome prevents color-cast illusions against video frames; right-hand Inspector arranges parameters cleanly with scrubbable numeric values, tabular numerals, and per-section reset actions.
- **Takeaways for Vuoom:** Add per-section quick resets and clean `tabular-nums` alignment in **Inspector**, keep **Stage** neutral behind the frame, and expand the default-most-useful Clip sections or provide a clear visual summary rather than 9 closed accordions over empty space.

#### 6. Figma (UI3 Desktop Workspace)
- **URL:** <https://www.figma.com/blog/ui3-the-new-figma-ui/>
- **Category:** Desktop App (Design Tool)
- **What Is Good About It:** UI3 proved via real user feedback that floating outer sidebars with perimeter gaps waste laptop canvas width—leading Figma back to **docked, collapsible edge panels with crisp 1px dividers**, reserving floating pills strictly for transient canvas toolbars (like Crop or Zoom HUDs).
- **Takeaways for Vuoom:** Keep **ToolRail**, **Stage**, **Timeline**, and **Inspector** crisply docked/separated without excessive gutter bloat at `880×560`, fix ToolRail vertical overflow so all 11 tools + collapse toggle always fit cleanly, and use floating glass pills only for transient **CropBar** / **Stage** hints.

#### 7. Linear (Desktop App & UI Redesign)
- **URL:** <https://linear.app/blog/how-we-redesigned-the-linear-ui-part-2>
- **Category:** Desktop App (Pro Productivity)
- **What Is Good About It:** Calibrated 4-step near-black luminance surface ladder separated by 1px hairline borders; disciplined `500–550` medium typography instead of heavy 700 bolds everywhere; left-nav + structured right-pane **Settings** with clean category separation and zero nested modal chains.
- **Takeaways for Vuoom:** Consolidate Vuoom's 13 erratic font weights down to a crisp 4-weight ramp (`400`, `500`, `600`, `700`), standardize a 4-tier surface ladder with 1px hairlines, and upgrade **Settings** with pinned header/close controls and clean section cards.

#### 8. Raycast & Things 3 (Keyboard UX & Empty States)
- **URL:** <https://manual.raycast.com/> · <https://culturedcode.com/things/>
- **Category:** Desktop Apps (Launcher & Task Manager)
- **What Is Good About It:** Raycast anchors a generous top search input, clean `<Kbd>` physical keycaps, and a bottom navigation hint bar (`↑↓ Navigate · ↵ Run · Esc Close`). Things 3 treats empty states as calm, intentional spaces with a subtle motif, 1-line title, 1-line hint, and primary CTA.
- **Takeaways for Vuoom:** Upgrade **CommandPalette** with Raycast's fixed list height + bottom shortcut bar + ARIA combobox semantics, and standardize Vuoom's **EmptyState** grammar across Home, Inspector, Captions, and Pickers.

---

### Part B: Design Inspiration Galleries (5 Curated References)

#### 9. Mobbin
- **URL:** <https://mobbin.com>
- **Category:** Gallery (Real Shipped Flows & Micro-States)
- **What Is Good About It:** Indexes full shipped flows including loading states, inline validation errors, empty states, and confirmation steps—not just static happy-path mockups.
- **Takeaways for Vuoom:** Close every silent failure and missing loading state uncovered in `UI_AUDIT.md` (`.busy-veil` card container, `fitToBudget` abort, Storage confirmation modal, Caption/Audio inline errors, Waveform loading shimmer).

#### 10. The Component Gallery
- **URL:** <https://component.gallery> (`https://component.gallery/components/modal/`, `https://component.gallery/components/empty-state/`)
- **Category:** Gallery (Component Anatomy & Accessibility)
- **What Is Good About It:** Benchmarks ~60 UI components across 100 design systems with explicit WAI-ARIA contracts (`role="dialog"`, `aria-modal`, `combobox`, `tablist`/`tabpanel`, `toolbar`).
- **Takeaways for Vuoom:** Standardize Vuoom's shared primitive library (`src/ui.tsx`) against canonical WAI-ARIA contracts and `>= 24×24px` WCAG 2.2 AA touch/pointer target sizes.

#### 11. Refero
- **URL:** <https://refero.design>
- **Category:** Gallery (Real-Product Settings & Forms)
- **What Is Good About It:** Demonstrates consistent vertical form rhythm (`24px` section spacing, `12px` sibling rows, aligned toggle controls, clear destructive zones).
- **Takeaways for Vuoom:** Fix misaligned rows in **ExportDialog** (`Smooth gradients`, `Fit under`), **Settings**, and **Inspector** by enforcing a shared horizontal/stacked `Field` grid with consistent label widths and spacing tokens.

#### 12. Dribbble & Behance (Dark Desktop & Video Editor Collections)
- **URL:** <https://dribbble.com/search/video-editor-desktop-app> · <https://www.behance.net/search/projects?search=video%20editor%20desktop%20ui>
- **Category:** Galleries (Visual Polish, Dark Palettes & Export Dialogs)
- **What Is Good About It:** Strongest dark-mode video studio designs avoid pure `#000` halation, constrain saturated accents strictly to playhead/record/CTA states, and lay out Export dialogs with clear parameter groupings and live output previews.
- **Takeaways for Vuoom:** Keep timeline track hues distinguishable yet desaturated enough not to clash, and restructure **ExportDialog** into balanced parameter rows + clean preset differentiation.

#### 13. Minimal Gallery, Godly & Pinterest Desktop UI Indexes
- **URL:** <https://minimal.gallery> · <https://godly.website> · <https://www.pinterest.com/search/pins/?q=dark%20mode%20desktop%20app%20ui> *(plus `https://www.awwwards.com`, `https://www.siteinspire.com`, `https://screenlane.com`)*
- **Category:** Galleries (Editorial Whitespace, Restraint & Motion Craft)
- **What Is Good About It:** Proves disciplined spacing, subtle `1px` top-edge highlights, and ultra-fast (`<= 180ms`) compositor-only transitions elevate desktop tools far more than decorative blur or gradients.
- **Takeaways for Vuoom:** Replace heavy `backdrop-filter` overlays and `filter: drop-shadow` with crisp 1px borders, `--rim` top edges, and fast `transform`/`opacity` micro-interactions.

---

### Part C: Platform Design Guidelines (2 References)

#### 14. Windows 11 Fluent Design System (Microsoft Learn)
- **URL:** <https://learn.microsoft.com/en-us/windows/apps/design/style/layering-and-elevation> (`.../basics/titlebar`, `.../style/typography`, `.../style/color`, `.../style/geometry`)
- **Category:** Platform Guideline
- **What Is Good About It:** Strict 4px grid (`4/8/12/16/24px`), Effective Pixels (`epx`) scaling cleanly across `100%`, `125%`, `150%`, `200%` DPI, `36–44px` interactive titlebar with explicit drag regions and native right-hand caption controls, visible inactive-window dimming, and strict WCAG AA contrast (`>= 4.5:1` text, `>= 3:1` UI boundaries).
- **Takeaways for Vuoom:** Enforce a strict 4px spacing scale and uniform `32px`/`28px` control heights in the Titlebar; add inactive-window dimming; verify zero clipping across `100%–200%` DPI and `880×560` minimum window size.

#### 15. macOS Human Interface Guidelines (Apple Developer)
- **URL:** <https://developer.apple.com/design/human-interface-guidelines/windows> (`.../sidebars`, `.../panels`, `.../materials`, `.../layout`)
- **Category:** Platform Guideline
- **What Is Good About It:** Reserves a `70–78pt` left safe area for traffic lights inside unified toolbars (`38–44px`), enforces clean Sidebar (left) vs contextual collapsible Inspector (right) semantics, dims chrome subtly when unfocused, and endorses opaque tinted surface fallbacks over live video playback to protect GPU frame budgets.
- **Takeaways for Vuoom:** Preserve macOS `padding-left: 84px` traffic-light clearance, keep left ToolRail vs right Inspector clearly separated, and replace expensive `backdrop-filter` blurs over `<canvas>` with tuned opaque/translucent surfaces.

---

## 3. Open-Source UI & Code Architecture References (15 Curated References)

### Part D: Component & Design Token Systems (6 References)

#### 16. `shadcn/ui`
- **URL:** <https://github.com/shadcn-ui/ui> · <https://ui.shadcn.com/docs/theming> *(MIT)*
- **Pattern:** Copy-owned zero-runtime primitives + paired surface/foreground tokens (`--surface` / `--on-surface`) + blocking `<head>` theme resolution script.
- **Takeaway for Vuoom:** Keep Vuoom's zero-dependency copy-owned primitives in [`src/ui.tsx`](./src/ui.tsx); pair every surface and accent explicitly with its contrast-verified foreground token (`--on-record`, `--on-ok`, `--on-warn`).

#### 17. Radix UI (`radix-ui/colors` & `radix-ui/primitives`)
- **URL:** <https://github.com/radix-ui/colors> · <https://github.com/radix-ui/primitives> *(MIT)*
- **Pattern:** 12-step functional contrast scale (canvas -> panel -> interactive rest/hover/active -> subtle/element/focus borders -> solid accent -> low/high-contrast text) + alpha counterparts (`-a`) + `data-state="open|closed"` styling attributes.
- **Takeaway for Vuoom:** Structure `src/styles/tokens.css` cleanly into ordered luminance steps + alpha hairlines (`--line`, `--line-2`, `--rim`) and replace mixed `.on`/`.open` CSS selectors with predictable `data-state` hooks.

#### 18. Kobalte (`kobaltedev/kobalte`)
- **URL:** <https://github.com/kobaltedev/kobalte> · <https://kobalte.dev> *(MIT)*
- **Pattern:** SolidJS-native WAI-ARIA primitives using signals, roving `tabindex` collection hooks, modality-aware `focus-visible`, and animation-aware exit `createPresence`.
- **Takeaway for Vuoom:** Extract a lightweight (~60 LOC) SolidJS `createRovingTabindex` helper for `Seg`, `ToolRail`, `Inspector` tabs, and `Settings` nav without adding heavy npm package overhead.

#### 19. `corvu` (`corvudev/corvu`)
- **URL:** <https://github.com/corvudev/corvu> · <https://corvu.dev> *(MIT)*
- **Pattern:** SolidJS headless split-pane resizer (`role="separator"`, `aria-valuenow`, arrow-key step resizing, `rAF` coalesced pointer moves) and focus-restoring nested modal management.
- **Takeaway for Vuoom:** Upgrade Vuoom's `Inspector` and `Timeline` resizers with `role="separator"` + keyboard resizing + `rAF` throttling, and fix [`dialog.ts`](./src/dialog.ts) `inert` background & detached-opener focus return.

#### 20. Park UI (`chakra-ui/park-ui`)
- **URL:** <https://github.com/chakra-ui/park-ui> · <https://park-ui.com> *(MIT)*
- **Pattern:** Strict **3-tier token layering**: Tier 1 Palette Primitives -> Tier 2 Semantic Roles -> Tier 3 Component Slots, separating neutral warmth (`Darkroom`/`Daylight`/`Mono`) cleanly from semantic accents.
- **Takeaway for Vuoom:** Refactor [`src/styles/tokens.css`](./src/styles/tokens.css) into explicit Tier 1 primitives + Tier 2 semantic roles so components never hardcode `#ffffff`, `#0e0e0f`, or `rgba(128,128,128,0.28)` in component CSS.

#### 21. Tailwind CSS v4 (`@theme` & Cascade Layers)
- **URL:** <https://github.com/tailwindlabs/tailwindcss> · <https://tailwindcss.com/docs/theme> *(MIT)*
- **Pattern:** Native CSS Cascade Layers (`@layer tokens, base, components, screens, motion;`) and `color-mix(in oklab, ...)` to compute soft/hover/border variants automatically with zero `!important`.
- **Takeaway for Vuoom:** Organize Vuoom's CSS entry order under `@layer` to eliminate all `8` existing `!important` overrides across `dialogs.css` and `shell.css`.

---

### Part E: Open-Source Desktop & Pro-Tool Apps (9 References)

#### 22. Zed (`zed-industries/zed`)
- **URL:** <https://github.com/zed-industries/zed> · <https://zed.dev/schema/themes/v0.2.0.json> *(GPL-3.0; study architecture only — copy zero GPL code)*
- **Pattern:** Explicit desktop chrome tokens (`title_bar.background`, `title_bar.inactive_background`, `panel.background`, `status_bar.background`), 6-state border hierarchy (`border`, `border.focused`, `border.selected`), and active-pane focus highlights.
- **Takeaway for Vuoom:** Add `:root[data-window-focused="false"]` inactive-window chrome dimming and clear pane focus cues across Stage, Timeline, and Inspector.

#### 23. AppFlowy (`AppFlowy-IO/AppFlowy`)
- **URL:** <https://github.com/AppFlowy-IO/AppFlowy> *(AGPL-3.0; study architecture only — copy zero AGPL code)*
- **Pattern:** Role-based workspace zones, strict 4px spacing & radius scales, and hover/focus-within opacity reveal for secondary list row actions without layout shift.
- **Takeaway for Vuoom:** Keep Home recent-project card actions (`.recent-remove` / reveal) smoothly revealed via `opacity` at `>= 24×24px` hit size without shifting card geometry.

#### 24. AFFiNE (`toeverything/AFFiNE` & `@toeverything/theme`)
- **URL:** <https://github.com/toeverything/AFFiNE> *(MIT core)*
- **Pattern:** Zero-runtime CSS custom-property layout geometry (`--sidebar-width`, `--header-height`) updated at shell root via `rAF`, plus crisp Library <-> Canvas workspace transitions.
- **Takeaway for Vuoom:** Bind `--inspector-w` and `--timeline-h` via CSS variables on `.workspace` updated in `rAF` rather than triggering subtree style recalculation during splitter drag.

#### 25. Hoppscotch (`hoppscotch/hoppscotch`)
- **URL:** <https://github.com/hoppscotch/hoppscotch> *(MIT)*
- **Pattern:** Tauri v2 desktop integration (`data-tauri-drag-region`), sticky tab header inside scrollable inspector pane, and orthogonal `"system"` theme preference backed by a single `matchMedia('(prefers-color-scheme: dark)')` listener.
- **Takeaway for Vuoom:** Add `"System"` theme tracking in [`src/themes.ts`](./src/themes.ts) with clean `onCleanup`, and keep Inspector tabs & clip search sticky at the top of `.inspector`.

#### 26. Bruno (`usebruno/bruno`)
- **URL:** <https://github.com/usebruno/bruno> *(MIT)*
- **Pattern:** `1px` visual splitters with `8px` invisible `::before` hit areas (`inset: 0 -4px`) that highlight cleanly on hover/drag; also demonstrates why pure CSS custom properties outperform CSS-in-JS (`styled-components`) on desktop cold start and theme switches.
- **Takeaway for Vuoom:** Keep Vuoom's zero-runtime vanilla CSS, and apply invisible expanded hit pads (`::before`) to `Timeline` handles, `CropEditor` handles, and pane splitters so all hit targets pass `>= 24×24px` without looking bulky!

#### 27. Excalidraw (`excalidraw/excalidraw`)
- **URL:** <https://github.com/excalidraw/excalidraw> *(MIT)*
- **Pattern:** Unified `.Island` surface primitive (`pointer-events: none` overlay containers with `pointer-events: auto` islands), tool buttons with tiny embedded shortcut glyphs, and contextual property cards.
- **Takeaway for Vuoom:** Unify **Stage** crop/hint pills, **RecordOverlay** HUD, and **Coachmark** around one crisp `.island` grammar, and add subtle keycap shortcut labels inside **ToolRail** tooltips/cards.

#### 28. Plane (`makeplane/plane`) & Cal.com (`calcom/cal.diy`)
- **URL:** <https://github.com/makeplane/plane> *(AGPL-3.0; patterns only)* · <https://github.com/calcom/cal.diy> *(MIT)*
- **Pattern:** Reusable `<EmptyState>` and `<SettingsHeader>` primitives, inline form validation (never toast-only), and **150ms delayed CSS skeleton reveal** (`opacity: 0 -> 1` after `150ms` delay) so fast local IPC resolves before skeletons ever flash.
- **Takeaway for Vuoom:** Add `<EmptyState>` and `<ModalHeader>`/`<SettingsHeader>` helpers in [`src/ui.tsx`](./src/ui.tsx), replace `.hotkey-problem` muted text with semantic error styling, and use zero-JS delayed CSS skeletons for async thumbnail/storage probes.

#### 29. `cmdk` (`pacocoursey/cmdk`)
- **URL:** <https://github.com/pacocoursey/cmdk> *(MIT)*
- **Pattern:** Canonical accessible command palette (`role="combobox"`, `aria-activedescendant`, `tabindex="-1"` options, fixed viewport height during filtering, `scrollIntoView({ block: 'nearest' })`, `Ctrl+N`/`Ctrl+P`, live preview hooks).
- **Takeaway for Vuoom:** Fix [`CommandPalette.tsx`](./src/components/CommandPalette.tsx)'s broken ARIA combobox + `Tab` walk and add a Raycast-style bottom shortcut bar + live theme preview on hover/arrow navigation.

#### 30. Lucide Icons (`lucide-icons/lucide`)
- **URL:** <https://github.com/lucide-icons/lucide> · <https://lucide.dev/guide/packages/lucide-solid> *(ISC License — compatible with `Apache-2.0`)*
- **Pattern:** Clean `24×24` grid, consistent `1.75–2px` stroke weight, rounded caps/joins, tree-shakable vector paths.
- **Takeaway for Vuoom:** Keep Vuoom's zero-dependency path map in [`src/icons.tsx`](./src/icons.tsx) (~5 KB raw, zero npm bundle tax!) rather than importing a full icon library; source any needed additional paths cleanly under ISC or generate bespoke brand visuals via **Codex (GPT-6.2 Astra)**.

---

## 4. Concrete Architectural Blueprint & Compliance Matrix for Vuoom

### Technical Patterns to Implement (Zero Heavy Dependencies)
1. **Zero-Flicker System Theme Init:** Synchronous ~15-line `<head>` script in [`index.html`](./index.html) resolving `?theme=` / `localStorage['vuoom-theme']` / `prefers-color-scheme` and setting `data-theme`, `color-scheme`, `meta[name="theme-color"]`, and `#splash` background before first paint; single `matchMedia` `'change'` listener in [`src/themes.ts`](./src/themes.ts) with `onCleanup`.
2. **3-Tier Semantic Tokens + Cascade Layers:** `@layer tokens, base, components, screens, motion;` with calibrated dark/light elevation, `--rim` top-edge highlights, `--focus-ring` exclusively on `:focus-visible`, and mathematically verified `>= 4.5:1` AA accents across all themes.
3. **Typography & Bundle Hygiene:** Fix `"Inter Variable"` font-family mismatch in [`src/editor/constants.ts`](./src/editor/constants.ts); trim `@fontsource-variable/inter` to required Latin subsets (`-85 KB` to `-170 KB` in `dist/`); replace `width` transitions (`.meter-fill`, `.cap-bar`) and infinite idle animations (`.insp-empty-icon`) with compositor-only `transform`/`opacity` respecting `prefers-reduced-motion` + `data-motion="reduced"`.
4. **Expanded Invisible Hit Targets (`::before`):** Bring every sub-24px handle/switch/slider/button to `>= 24×24px` WCAG 2.2 AA without visually bloating tight desktop controls.
5. **SolidJS `createRovingTabindex` + `dialogA11y` Hardening:** Shared keyboard navigation hook across `Seg`, `ToolRail`, `Tabs`, and `Menus`; `inert` background + safe `opener` fallback in [`src/dialog.ts`](./src/dialog.ts).

### License & Originality Compliance Guardrail

| License Tier | Projects Studied | Vuoom (`Apache-2.0`) Rule |
|---|---|---|
| **Permissive (`MIT` / `ISC`)** | `shadcn/ui`, Radix, Kobalte, corvu, Park UI, Tailwind v4, AFFiNE core, Hoppscotch, Bruno, Excalidraw, Cal.com, `cmdk`, Lucide | Safe to adapt patterns and CSS/ARIA architecture cleanly; keep zero unneeded npm deps to protect `dist/` budget. |
| **Copyleft (`GPL-3.0` / `AGPL-3.0`)** | Zed (`GPL-3.0`), AppFlowy (`AGPL-3.0`), Plane (`AGPL-3.0`) | **Architectural inspiration only.** Never copy code, styles, or assets into Vuoom. |
| **Proprietary Desktop / Brand Assets** | Screen Studio, CleanShot X, Loom, Figma, Linear, Raycast, Things 3, DaVinci Resolve | **Interaction & visual craft inspiration only.** Never replicate layouts 1:1 or reuse third-party assets. Generate original Vuoom graphics/empty-state art with **Codex (GPT-6.2 Astra)**. |
