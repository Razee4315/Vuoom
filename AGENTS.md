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

## 3. Performance Budgets & Verification Gates

Every change must stay within the budgets in [`PERFORMANCE.md`](./PERFORMANCE.md) (`Cold start <= 2,050 ms`, `Warm <= 400 ms`, `First usable UI <= 500 ms`, `Rust idle CPU <= 0.50% / 0.10%`, `dist/ <= 1,478 KB raw / 694 KB gz`, `perf_4k` non-regression, `0` leaked DOM nodes/listeners). Measure before and after any performance-sensitive edit.

Before pushing or merging, verify locally:
- `pnpm run typecheck && pnpm run lint && node scripts/check-i18n.mjs && pnpm run build`
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo deny check` (and verify no unused workspace deps with `cargo machete`)
