# Vuoom Agent & Engineering Efficiency Rules

These rules govern all future work on Vuoom so performance, binary size, power draw, and CI hygiene do not regress after `v2.2.0`.

## 1. Runtime & Architecture

- **Heavy work stays in Rust and off the main UI thread.** Never run capture, decoding, compositing, PNG/GIF/MP4 encoding, file I/O, or window/recovery scanning on the Tauri main thread (`commands.rs` should use `async` + `tauri::async_runtime::spawn_blocking`), **except** ordered synchronous record-overlay mode commands (`enter_overlay`, `set_region`, `enter_stopbar`, `set_panel_size`, `show_region_border`, `cancel_record_flow`) documented in `docs/State-Machines.md`.
- **Batch IPC and minimize payloads.** Never serialize large project histories or raw frame buffers through Tauri JSON commands when smaller diffs, binary WebSocket frames (`vuoom-preview`), or file paths suffice.
- **No polling waste or leaked listeners.** Every `setInterval`, `setTimeout`, `requestAnimationFrame`, DOM listener, and WebSocket subscription must clean up in `onCleanup`, avoid stacking async calls (`setTimeout` after completion or `createSyncSlot`), stop once resolved when one-shot, and pause while `document.hidden`.
- **Gate browser-only code at build time.** Keep mock backends and browser test harnesses behind `!__TAURI_BUILD__` (`src/index.tsx` / `vite.config.ts`) so packaged `.exe`/`.app` bundles stay minimal.
- **No full-library imports; lazy-load heavy UI.** Keep icons, locales, and heavy non-home views/dialogs split or tree-shaken so startup and splash-to-usable stay within budget.
- **Avoid forced synchronous layouts & coarse signals.** Never interleave DOM reads (`clientWidth`, `scrollLeft`, `getComputedStyle`) inside reactive writes or playback ticks; use `ResizeObserver`, `createSelector`, `<Index>` for fixed/keyed lists, and `rAF` coalescing.

## 2. Dependencies, Security & Licensing

- **Justify every new dependency.** Before adding a Rust or npm crate, check binary/bundle impact, maintenance status, lighter alternatives, and license compatibility (`Apache-2.0` / no copyleft enforced by `deny.toml`).
- **Keep Tauri JS and Rust packages in lockstep** (`@tauri-apps/*` in `package.json` matched to `tauri` / `tauri-plugin-*` in `Cargo.toml` / `src-tauri/Cargo.toml`), and keep `[workspace.package]` version synchronized across `Cargo.toml`, `package.json`, and `src-tauri/tauri.conf.json`.
- **Do not silence `cargo-deny` or `clippy` lightly.** Any `deny.toml` exception must be narrow, temporary, and justified by why the vulnerable code path is unreachable or unexploitable.

## 3. UI Design System, Motion & Accessibility (`v2.5.0+`)

- **Use design tokens and shared primitives only.** Never introduce hardcoded hex/rgba palette colors, ad-hoc shadows, raw radii, or one-off control styles outside [`src/styles/tokens.css`](./src/styles/tokens.css) and [`src/ui.tsx`](./src/ui.tsx) (see [`DESIGN_SYSTEM.md`](./DESIGN_SYSTEM.md)). Preserve the Tungsten Darkroom & Daylight Archival Studio language, neutral matte Stage framing, zero purple, and semantic Record Crimson exclusively for capture actions.
- **No `backdrop-filter: blur()`, `100vmax` spread shadows, or `!important`.** Elevated overlays, floating toolbars, and modals must use opaque `.island` surfaces (`inset 0 1px 0 var(--rim)`), bounded dim rectangles, and explicit `@layer` cascade ordering.
- **Follow [`MOTION.md`](./MOTION.md) & respect reduced motion.** Animate only compositor-safe properties (`transform`, `opacity`), keep all idle states completely still (`0` infinite idle animations), and gate every non-essential transition/keyframe behind `@media (prefers-reduced-motion: no-preference)` and `:root:not([data-motion="reduced"])`.
- **Meet WCAG 2.2 AA & keyboard standards.** Maintain `>= 4.5:1` text contrast across all 6 themes, `>= 3.0:1` UI boundary contrast, `>= 24×24px` hit targets, `dialogA11y` focus trapping/restoration on every modal/overlay, WAI-ARIA roles (`tablist`, `combobox`, `slider`, `alert`, `status`), and tabular numerals (`tabular-nums`) for all timecodes, byte counts, and dimensions.
- **Keep local verification lightweight.** On constrained dev machines, run `pnpm run typecheck`, `pnpm run lint`, and `node scripts/check-i18n.mjs` locally; offload full Rust/Tauri release builds and cross-platform checks to GitHub Actions CI and verify downloaded artifacts live via WebView2 CDP.

## 4. Performance Budgets & Verification Gates

Every change must stay within the budgets in [`PERFORMANCE.md`](./PERFORMANCE.md) (`Cold start <= 2,050 ms`, `Warm <= 400 ms`, `First usable UI <= 500 ms`, `Rust idle CPU <= 0.50% / 0.10%`, `dist/ <= 1,478 KB raw / 694 KB gz`, `perf_4k` non-regression, `0` leaked DOM nodes/listeners). Measure before and after any performance-sensitive edit.

Before pushing or merging, verify locally:
- `pnpm run typecheck && pnpm run lint && node scripts/check-i18n.mjs && pnpm run build`
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo deny check` (and verify no unused workspace deps with `cargo machete`)
