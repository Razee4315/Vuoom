# Ecosystem & Performance Research Log (2026-11-23)

This log captures Phase 2 web and dependency research conducted before modifying code in Vuoom `v2.1.2`. All version checks and advisories reflect crates.io / npm / GitHub ecosystem status as of **2026-11-23**.

---

## 1. Tauri v2 (`2.11.2` -> `2.12.x`) & WebView2 Shell Behavior

- **Sources**: Official Tauri v2 releases & docs, Microsoft WebView2 lifecycle/throttling docs, Vuoom `deny.toml`, `src-tauri/Cargo.toml`, `package.json`.
- **Current versions**: JS `@tauri-apps/api 2.11.1`, `@tauri-apps/cli 2.11.4`, plugins `2.3.1–2.10.1`; Rust `tauri 2.11.2`, `tauri-runtime-wry 2.11.2`, `tauri-plugin-updater 2.10.1`.
- **Latest available**: Tauri `2.12.x` (`@tauri-apps/api 2.12.5`, `@tauri-apps/cli 2.12.6`, `plugin-updater 2.14.1`, etc.).
- **Key findings**:
  1. **Sync vs. Async Command Execution**: Tauri v2 executes non-`async` `#[tauri::command]` functions directly on the main UI thread (Tao event loop). Async commands run on Tokio's worker pool, which Vuoom shares with `PreviewServer` (`tokio-tungstenite` loopback WebSocket). Heavy file/capture/compute commands should be `async` + `tauri::async_runtime::spawn_blocking`, EXCEPT synchronous record-overlay mode commands (`enter_overlay`, `set_region`, `enter_stopbar`, `set_panel_size`, `show_region_border`, `cancel_record_flow`) whose main-thread call ordering `src/editor/recordFlow.ts` explicitly relies on (`docs/State-Machines.md`).
  2. **Tauri 2.12**: Drops Windows 7/8 support (Vuoom already targets Windows 10/11 + macOS 14+), requires MSRV 1.90, updates `wry`/`tao`, and adds minor permission/autofill enhancements. JS and Rust `@tauri-apps/*` packages must be upgraded in lockstep.
  3. **WebView2 Idle CPU & Memory**: Even on an idle screen, WebView2 spawns 6 child processes (~343 MB WS / 113 MB Private in baseline). Continuous CSS animations (especially `box-shadow` repaints and `filter: blur(20px)` layers) force 60 Hz compositor/paint work (`15.88%` single-core CPU focused, `9.39%` even minimized on Vuoom Home).
- **Applies to Vuoom**: Move ~60 heavy sync/blocking commands to `async` + `spawn_blocking` (Tier 1); test Tauri 2.12 lockstep upgrade cleanly or document any blocker; eliminate idle IPC polling (`Permissions.tsx`, Tier 1) and flag Home infinite CSS animations for Track 2 redesign.
- **Expected impact**: Removes UI freezes and preview starvation; eliminates 900 idle IPCs/hr on Windows; sets up Track 2 idle CPU cut from ~16% single-core to <0.5%.
- **Effort**: Small–Medium.

---

## 2. Graphics & GPU Compositing (`wgpu 25.0.2` / `glyphon 0.9.0` / Adapter Selection)

- **Sources**: `wgpu` & `glyphon` releases (`wgpu` 25→30, `glyphon` 0.9→0.12), `crates/vuoom-render/src/compositor.rs`, `perf_4k` baseline run.
- **Current versions**: `wgpu 25.0.2` (`Backends::all()` via `InstanceDescriptor::default()`, `PowerPreference::None` via `RequestAdapterOptions::default()`), `glyphon 0.9.0` (pulls `cosmic-text 0.14.2`, `fontdb 0.16.2`, `lru 0.12.5`, `memmap2 0.9.10`, `swash 0.2.8`).
- **Key findings**:
  1. **Adapter & Backend Selection (`compositor.rs:460-464`)**: Using `Backends::all()` initializes Vulkan, WGL/GLES, and DX12 loaders/contexts at boot, and `PowerPreference::None` picked **Intel HD Graphics 630 (Vulkan, IntegratedGpu)** instead of DX12 / discrete NVIDIA Quadro M1200 during `perf_4k_export` (`17.0 fps` plain / `17.6 fps` framed 4K!). Using `Backends::from_env().unwrap_or(Backends::PRIMARY)` + `PowerPreference::HighPerformance` avoids probing extra graphics stacks and prefers discrete/primary hardware (DX12 on Windows, Metal on macOS).
  2. **Parallel Font DB Init (`compositor.rs:792`)**: `FontSystem::new()` scans all OS fonts serially *after* adapter/device/shader init. Spawning `FontSystem::new()` + bundled `fontdb::Source::Binary(Arc::new(BYTES))` concurrently with wgpu initialization cuts 50–300 ms off engine boot readiness.
  3. **Text Shaping Cache**: `compositor.rs:1510, 1723` reshapes every label and caption via `TextBuffer::new` + `Shaping::Advanced` on every single composited frame (preview + every export frame).
  4. **Upgrading `wgpu` 25 -> 29/30 + `glyphon` 0.9 -> 0.12**: Fixes `lru` / `memmap2` audit warnings and removes `windows 0.58` duplicate, **but** brings breaking WGSL/wgpu/hal API changes (`DeviceDescriptor`, `BufferSlice::map_async`, `wgpu-hal` interop) -> classified **Tier 3**.
- **Applies to Vuoom**: `B5` (`Source::Binary` zero-copy bundled fonts) & `B3` in Tier 1; `B1` (parallel `FontSystem`), `B2` (`PRIMARY` + `HighPerformance`), `C6` (text buffer cache) in Tier 2; `wgpu`/`glyphon` major bump documented in Tier 3.
- **Expected impact**: Potential multi-fold speedup on dual-GPU 4K export + 50–300 ms faster boot + smoother multi-caption/label exports.
- **Effort**: Small (`B5`/`B1`/`B2`), Medium (`C6`).

---

## 3. Cargo Build Performance, Binary Size & Linkers

- **Sources**: Cargo Book (`profiles`, `crate-type`), Rust 1.99 release notes (`rust-lld` availability on `x86_64-pc-windows-msvc`), `src-tauri/Cargo.toml`, root `Cargo.toml`.
- **Current setup**: `src-tauri/Cargo.toml` sets `crate-type = ["staticlib", "cdylib", "rlib"]`; root `Cargo.toml` sets `lto = "thin"`, `codegen-units = 1`, `strip = true`, and `rust-version = "1.80"` (stale — code uses `slice::as_chunks` from 1.88). Incremental release rebuild of `vuoom` takes **312.68 s**.
- **Key findings**:
  1. **Desktop `crate-type = ["rlib"]`**: `staticlib` and `cdylib` exist only for mobile targets in Tauri's template and cause Cargo/rustc to perform three full link passes under LTO on every desktop release build.
  2. **Windows Linker (`rust-lld`) & Dev Debug Info**: `rust-lld.exe` ships inside `stable-x86_64-pc-windows-msvc`. Configuring `linker = "rust-lld"` in `.cargo/config.toml` + `debug = "line-tables-only"` (`debug = false` on dependencies) dramatically speeds up incremental linking without requiring LLVM/LLD installation.
  3. **Panic Mode**: Vuoom uses panic hooks (`lib.rs:151`), `.join()` recovery (`zoom_chord.rs:126`), and mutex poison recovery (`.unwrap_or_else(|e| e.into_inner())`). Switching to `panic = "abort"` would break worker fault isolation -> **rejected / Tier 3**.
  4. **Crate Deduplication & Security**: `gif 0.13` with default features pulls `color_quant 1.1.0` alongside Vuoom's direct `color_quant 2.0.0` (`gif` `default-features = false, features = ["std", "raii_no_panic"]` drops `1.1.0`). `deny.toml` lacks `[graph] targets` and `jpeg-encoder` `IJG` license exception added implicitly by macOS port `#102`.
- **Applies to Vuoom**: Wave 1 Commits A & C (Tier 1).
- **Expected impact**: Massive reduction in incremental `cargo build --release` time (baseline 312.7s), green `cargo-deny check`, cleaner `Cargo.lock`, slightly smaller binary.
- **Effort**: Small.

---

## 4. SolidJS (`1.9.14` -> `2.0.3`), Vite (`8.1.4` -> `8.3.8`) & Browser Engine Patterns

- **Sources**: SolidJS 2.0 changelog/migration guide, Vite 8 / Rolldown release notes, `vite-plugin-solid 2.11.x` changelog, Chrome/Edge DevTools rendering/layout-thrashing guidelines, Cap (`CapSoftware/Cap`) open-source Tauri architecture.
- **Key findings**:
  1. **SolidJS 2.0**: Introduces breaking reactivity semantics (`createAsync`, batching/store API changes) -> **Tier 3**, keep `solid-js 1.9.x` for Track 1 & 2.
  2. **Reactivity Granularity & Forced Reflows in `createEditor.ts`**:
     - `Timeline.tsx` uses `<For each={ed.tickMarks()}>` where `tickMarks()` allocates fresh `{ t, major, mid }` objects every frame/zoom step -> tears down & recreates up to ~6,000 DOM nodes. `<Index>` updates attributes in place.
     - Single coarse `drag()` signal + `.find()` in `liveGeom()`/`liveFont()` causes `O(N^2)` re-evaluation across all annotations per drag frame (`Stage.tsx`). Per-row `createMemo` with array equality + `createSelector` isolates updates to the dragged annotation (`O(1)`).
     - Reading `canvasEl.clientWidth` inside `pushSeek` and `tlScrollEl.scrollLeft`/`clientWidth` after reactive DOM updates forces synchronous style/layout recalculation on every 60fps playback frame. Using `stage().w` from existing `ResizeObserver` + cached scroll state removes 1–2 forced reflows/frame.
  3. **Vite 8 / Rolldown Bundle Config & Security**:
     - Updating `vite 8.1.4 -> 8.3.8`, `vite-plugin-solid 2.11.12 -> 2.11.17`, `@biomejs/biome 2.5.3 -> 2.6.8`, `typescript 5.6 -> 5.9` resolves Vite/PostCSS/nanoid/launch-editor dev advisories safely (`Tier 1`).
     - `index.tsx:39` dynamically imports `./mock/install` at runtime via `!isMock`, causing Vite to emit `dist/assets/install-*.js` (`41,388 B` raw / `13,966 B` gzip) into packaged desktop installers. Gating with build-time `__TAURI_BUILD__` (`process.env.TAURI_ENV_PLATFORM`) strips the mock chunk from packaged builds while preserving browser mock builds (`pnpm build`, `pnpm dev`, `pnpm serve`).
     - Setting `build.modulePreload: { polyfill: false }` drops ~1 KB of unnecessary polyfill for modern WebView2 / WKWebView.
     - Heavy non-initial modals/screens (`ExportDialog`, `Settings`, `CommandPalette`, `RecordOverlay`, `WindowPicker`, `SourcePicker`, `CropEditor`, `Workspace`) can be `lazy()`-loaded + idle-prefetched after splash (Tier 2).
  4. **Cap / Desktop Recorder Pattern**: Disconnect editor `PreviewClient` during active recording (`ownsScreen(recordFlow())`) so live capture preview frames don't decode/churn JS heap on the hidden editor canvas (`60–120 MB/s`), and reconnect + seek afterward (Tier 2).
- **Applies to Vuoom**: `F10-T1` in Wave 1 Commit D; `F1–F9` in Wave 2; `F10-T2` in Tier 2.
- **Expected impact**: Eliminates layout thrashing and long tasks during drag/zoom/scrubbing; cuts packaged JS bundle by >41 KB raw; prevents concurrent GIF estimate pileups.
- **Effort**: Small–Medium.
