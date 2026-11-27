# Vuoom Motion & Micro-Interaction Specification (Phase U4)

**Goal:** Crisp, informative, zero-bloat motion that reinforces spatial hierarchy and user agency while holding **60fps under 6× CPU throttling** and **0.0% idle animation CPU**.  
**Technology Decision (`D-018`):** Native CSS transitions/keyframes + Web Animations API (WAAPI) only. **Zero external animation dependencies** (`0 B` added to `dist/`).

---

## 1. Timing & Easing Tokens

### Duration Tokens (`src/styles/tokens.css`)
| Token | Value | Use Case |
|---|---|---|
| `--dur-instant` | `80ms` | Press scale feedback, icon swaps, instant highlight cues |
| `--dur-fast` | `140ms` | Hover transitions, focus rings, switch/toggle snaps, **all exits/dismissals** |
| `--dur-normal` | `200ms` | Dropdown menus, popovers, tab panels, toast enter, section reveals |
| `--dur-slow` | `280ms` | Dialogs, recording overlay reveals, large workspace state swaps *(strictly `<= 400ms`)* |

> **Asymmetry Law:** Exits and dismissals always use `--dur-fast` (`140ms`), faster than `--dur-normal`/`--dur-slow` entrances (`200–280ms`), so dismissed surfaces clear immediately out of the user's way.

### Easing Tokens
| Token | Curve | Purpose |
|---|---|---|
| `--ease-standard` | `cubic-bezier(0.2, 0, 0, 1)` | In-place state shifts (segmented thumbs, switches, chevrons, hover fades) |
| `--ease-enter` | `cubic-bezier(0.16, 1, 0.3, 1)` | Decelerating entrances (dialogs, menus, toasts, popovers) |
| `--ease-exit` | `cubic-bezier(0.3, 0, 1, 1)` | Accelerating dismissals (popovers, modals, toasts leaving viewport) |

---

## 2. Hard Performance & Motion Laws

1. **Compositor-Only Rule (`transform` & `opacity` ONLY):**
   - Never animate `width`, `height`, `top`, `left`, `padding`, `margin`, `grid-template-rows`, `box-shadow`, `stroke-dashoffset` (on active recording counters unless composited/light), or `background-position`.
   - **Required fixes from `UI_AUDIT.md`:**
     - `.meter-fill`: replace `transition: width 60ms` with `transform: scaleX(var(--level, 0))` (`transform-origin: left center`).
     - `.cap-bar > i`: replace `transition: width` with `transform: scaleX(...)` (`transform-origin: left center`).
     - `.ptr-hotspot-mark`: replace `transition: left, top` with `transform: translate(...)`.
     - `.sect-body-wrap`: replace `grid-template-rows` animation with instantaneous disclosure container + `opacity` / `translateY(-4px -> 0)` compositor reveal on `.sect-body`.
2. **Zero Infinite Idle Animations:**
   - Remove `.insp-empty-icon { animation: float 3.2s infinite; }` and `.tl-film:empty { animation: film-wait 1.6s infinite; }` completely. Idle CPU on Home and Editor must stay at rest with zero continuous animation frames.
   - Active busy indicators (e.g., export progress or active spin during user-initiated waits) pause when `document.hidden` and stop in reduced-motion mode.
3. **Never Block Input:**
   - All opening/closing transitions must allow immediate pointer/keyboard interruption (`pointer-events` active immediately on entering controls, disabled immediately on exiting overlays so dismissed layers never eat clicks).
4. **Purposeful Motion Only:**
   - Motion explains spatial relationships (where a popover came from, which tab/segment switched, whether a destructive action failed) — never decorative bouncing on resting UI.

---

## 3. Reduced Motion Contract

Both OS preference (`@media (prefers-reduced-motion: reduce)`) AND Vuoom's in-app **Reduce motion** toggle (`:root[data-motion="reduced"]`) must trigger identical deterministic suppression across `index.html`, `src/styles/*.css`, and `src/RecordOverlay.css`:

1. Set `--dur-instant`, `--dur-fast`, `--dur-normal`, `--dur-slow` to **`0ms`**.
2. Zero all `animation-delay` / stagger delays (`0ms`) so staggered cards/sections never wait invisible for `20–140ms`.
3. Replace continuous spinners/pulses (`.spinner`, `.sb-dot.busy`, `.live-dot`, `.cap-bar.busy`) with calm static states (solid recording indicator, static progress bar/percentage, `"Working…"` text).
4. Replace spring scales (`scale(0.97)` / `translateY`) with clean `0ms` state snaps.

---

## 4. Micro-Interaction Catalog

| Interaction | Motion Spec | Reduced Motion Fallback | Notes / State Hygiene |
|---|---|---|---|
| **Button Press Feedback** | `transform: scale(0.98)` over `--dur-instant` (`80ms`) `--ease-standard` | `none` | Never shifts sibling layout or height. |
| **Hover States** | Opacity / discrete surface swap (`--hover` / `--record-hover`) in `--dur-fast` (`140ms`) | Instant swap (`0ms`) | Zero `box-shadow` transition repaints. |
| **Focus Rings** | Crisp `:focus-visible` ring (`2px` `--focus-ring` + `2px` gap); subtle `opacity` reveal `80ms` | Instant (`0ms`) | Pointer clicks never trigger `:focus-visible`. |
| **Toggles, Checkboxes & Segmented Thumbs** | Thumb translates via `transform: translateX(...)` in `--dur-fast` (`140ms`) `--ease-standard` | Instant snap (`0ms`) | Zero `left` transitions; full `>=24×24px` hit zone. |
| **List Add / Remove** | New items (recents, timeline segments, captions) fade/settle (`opacity: 0->1`, `translateY(4px->0)`) over `--dur-normal`; removed items collapse cleanly without blocking adjacent clicks | Instant mount/unmount | Optimistic UI update where safe (see below). |
| **Expand / Collapse (`Section` & `Home Options`)** | Chevron rotates `0 -> 90deg` (`transform`) in `--dur-fast`; section body fades in (`opacity: 0->1`, `translateY(-4px->0)`) over `--dur-normal` | Instant expand/collapse | No `grid-template-rows` layout animation. |
| **Tab & View Transitions** | Panel content subtle cross-fade (`opacity: 0.7 -> 1`, `translateY(2px -> 0)`) in `--dur-fast` (`140ms`) | Instant switch | Tab indicator slides via `transform` or switches cleanly; preserves manual tab selection. |
| **Modal, Menu & Popover Open / Close** | Enter: `opacity: 0->1`, `scale(0.97)->1`, `translateY(-4px->0)` (`--dur-normal` / `--dur-slow` `--ease-enter`). Exit: `opacity: 1->0`, `scale(1->0.98)` (`--dur-fast` `--ease-exit`) via `createPresence` | Instant show/hide | Immediately interruptible; `inert` backdrop; focus restored on exit. |
| **Toasts** | Off-canvas anchor; enter `opacity: 0->1`, `translateY(-8px->0)` (`--dur-normal` `--ease-enter`); exit `--dur-fast`; timer pauses on hover/focus | Instant appear/disappear | Never covers Stage video content. |
| **Copy-to-Clipboard Confirmation** | Icon morphs `copy` -> `check` (`scale(0.85->1)` `140ms`) + accessible status text (`"Copied"`) for `1800ms` | Instant icon swap + text | Live-region polite announcement once. |
| **Inline Validation** | Field border shifts to `--record-text`; error text fades in (`opacity: 0->1`, `translateX(-3px->0)`) in `--dur-fast` | Instant text + border | Associated with input via `aria-describedby` + `role="alert"`. |
| **Delayed Skeletons (Anti-Flicker)** | Pure CSS `@keyframes skeleton-show`: `opacity: 0` during `0–180ms`, then fades `0 -> 1` over `140ms` (`--ease-enter`) | Static `opacity: 1` after `180ms` delay (no shimmer) | Sub-180ms IPC resolves before skeleton ever paints (`0` timers, `0` shimmer loops). |
| **Optimistic Updates & Rollback** | Theme, toggles, recent removal, and annotation moves update signal immediately; on IPC failure, revert signal cleanly and surface error toast/banner with subtle settle | Same state rollback, `0ms` | Guarantees UI never hangs waiting for disk/IPC while staying truthful on error. |

---

## 5. 60fps Verification Protocol (Under 6× CPU Throttling)

Every UI chunk verified from the downloaded GitHub CI executable (`vuoom.exe`) must pass:
1. Attach CDP (`--remote-debugging-port=9222`), enable `Emulation.setCPUThrottlingRate({ rate: 6 })`.
2. Exercise interactive transitions: Home Options expand, Settings tab switches, Export format switching, Inspector collapsible sections + Selection switch, Timeline scrubber + zoom block select, Command Palette open/filter/close.
3. Verify zero frame drops / long tasks (`> 50ms` blocked interaction) during CSS transitions and `<= 0.50%` Rust / idle WebView2 CPU at rest on Home and Editor with zero continuous animations active.
