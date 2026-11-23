# Vuoom Performance Baseline & Tracking

This document records the measured baseline for **Vuoom v2.1.2** (commit `8e07d9b`), defines performance budgets across Track 1 (efficiency & modernization) and Track 2 (UI redesign), and tracks every verified optimization with before/after measurements.

## Test Environment

| Component | Specification / Version |
|---|---|
| OS | Windows 10 Pro (64-bit, x86_64) |
| CPU | Intel Core i7-7820HQ (4 cores / 8 logical processors @ 2.90 GHz) |
| RAM | 15.9 GB |
| GPUs | Intel HD Graphics 630 (iGPU) + NVIDIA Quadro M1200 (dGPU) |
| Rust toolchain | `rustc 1.99.0` (2026-09-28), `cargo 1.99.0` (2026-08-27) |
| Node / pnpm | `node v24.19.0`, `pnpm 10.34.9` |
| Bundled / system CLI tools | `gh 2.101.0`, `ffmpeg 9.0.1` |
| WebView / CDP harness | Packaged WebView2 (`--remote-debugging-port=9222`) & headless Edge (`msedge.exe`) against `vite preview` (`?mock=1&demo=1`) |

> Note on local unsigned test builds: `TAURI_SIGNING_PRIVATE_KEY` lives in GitHub Secrets, so local release test builds use `--config scratch/no_updater.json` (`bundle.createUpdaterArtifacts: false`) without modifying tracked config. Portable `libclang.dll` (`18.1.1`) and VS Build Tools 2022 `cmake.exe` (`3.31.6`) are supplied via environment variables for `whisper-rs-sys`.

---

## Performance Budgets (Enforced Across Track 1 & Track 2)

| Category | Metric | Budget / Target | Baseline (`v2.1.2`) |
|---|---|---|---|
| Startup | Cold start (window ready) | <= 2,050 ms (target < 1,500 ms) | 2,024.8 ms |
| Startup | Warm start median (window ready) | <= 400 ms | 354.1 ms |
| Startup | First usable UI (CDP reload to splash gone) | <= 500 ms (target < 350 ms) | 432.49 ms |
| Idle Power | Rust host CPU (focused / minimized) | <= 0.50% / <= 0.10% single-core | 0.456% / 0.094% |
| Idle Power | Total CPU incl. WebView2 (focused / minimized) | Target <= 0.50% / <= 0.20% single-core (see note below) | 16.341% / 9.482% |
| Bundle | Main JS chunk (`index-*.js`) | <= 380 KB raw / <= 115 KB gzip (Track 1 excludes 41.4 KB mock) | 377,683 B / 112,913 B gz |
| Bundle | Total `dist/` | <= 1,478 KB raw / <= 694 KB gzip | 1,477,304 B / 693,822 B gz |
| Binary | Release `vuoom.exe` | <= 29.8 MB (target <= 28.5 MB) | 29,784,064 B (28.40 MiB) |
| Installers | Windows `.msi` / NSIS `.exe` | <= 10.95 MB / <= 7.94 MB | 10,948,608 B / 7,932,566 B |
| Interactivity | 1x input-to-next-paint (routine editor actions) | < 100 ms; 0 long tasks (>50 ms) on toggles/scrubs | 24.2–159.9 ms (`add_zoom_and_delete`: 159.9 ms) |
| Leaks | 20x heavy open/close/zoom cycle (live WebView2) | 0 retained DOM nodes or listeners | 767 -> 767 DOM, 63 -> 60 listeners |
| 4K Engine | `perf_4k` store & composite/export | >= baseline across all 6 cases (no regression) | 17.0 / 17.6 fps export; 46–1594 fps store |

> **Baseline Idle Finding (`v2.1.2`)**: An updated idle measurement capturing reparented `msedgewebview2.exe` processes (`idle_baseline_fixed.json`) revealed WebView2 consuming **15.884% single-core CPU (782.6 ctx/s)** when idle focused, **15.730% (825.4 ctx/s)** unfocused, and **9.388% (784.0 ctx/s)** even when minimized on the Home screen. Inspection of `src/styles/home.css` shows two infinite animations on Home: `.home-inner::before` (a `620x320` element with `filter: blur(20px)` animating via `glow-drift`) and `.rec-dot.live` (animating non-composited `box-shadow` continuously via `live`). Because Track 1 enforces zero visible UI changes, visible Home animations remain untouched in Track 1 (`v2.2.0`) and are scheduled for Track 2 (`v2.3.0`) under `MOTION.md`. Track 1 still eliminates the 4s Windows `permissions` IPC poll (`Permissions.tsx`) and improves minimized/hidden polling.

---

## Baseline (`v2.1.2`, commit `8e07d9b`)

### 1. Frontend Build, Check Timings & Bundle Breakdown

| Check / Build Step | Baseline Time | Notes |
|---|---|---|
| `pnpm run typecheck` (`tsc --noEmit`) | 7.440 s | TypeScript 5.6.3 |
| `pnpm run lint` (`biome check src`) | 2.049 s | 503 ms Biome internal across 60 files |
| `node scripts/check-i18n.mjs` | 0.206 s | 1,086 keys |
| `pnpm run build` (clean `vite build`) | 9.229 s | 3.57 s Vite internal, 88 modules (`[PLUGIN_TIMINGS]` warning on `solid`) |
| Vite dev server cold start | 2.267 s ready / 9.09 s HTTP 200 | |

| Asset in `dist/` | Raw Bytes | Gzip Bytes | Category |
|---|---|---|---|
| `assets/Shrikhand-Regular-*.ttf` | 220,804 | 84,156 | Bundled annotation display font |
| `assets/Anton-Regular-*.ttf` | 170,812 | 79,818 | Bundled annotation display font |
| `assets/Poppins-Bold-*.ttf` | 155,996 | 71,257 | Bundled annotation display font |
| `assets/PermanentMarker-Regular-*.ttf` | 74,632 | 40,589 | Bundled annotation display font |
| `assets/BebasNeue-Regular-*.ttf` | 61,400 | 28,952 | Bundled annotation display font |
| `assets/inter-latin-ext-wght-normal-*.woff2` | 85,068 | 85,116 | `@fontsource-variable/inter` |
| `assets/inter-latin-wght-normal-*.woff2` | 48,256 | 48,254 | `@fontsource-variable/inter` |
| `assets/inter-cyrillic-ext-wght-normal-*.woff2` | 25,960 | 25,988 | `@fontsource-variable/inter` |
| `assets/inter-greek-wght-normal-*.woff2` | 18,996 | 19,024 | `@fontsource-variable/inter` |
| `assets/inter-cyrillic-wght-normal-*.woff2` | 18,748 | 18,776 | `@fontsource-variable/inter` |
| `assets/inter-greek-ext-wght-normal-*.woff2` | 11,232 | 11,236 | `@fontsource-variable/inter` |
| `assets/inter-vietnamese-wght-normal-*.woff2` | 10,252 | 10,275 | `@fontsource-variable/inter` |
| `assets/index-*.js` | 377,683 | 112,913 | Main app JS (~93 KB brotli) |
| `assets/zh-CN-*.js` | 65,446 | 24,868 | Simplified Chinese locale chunk |
| `assets/install-*.js` | 41,388 | 13,966 | Browser mock backend (currently shipped in Tauri builds too) |
| `assets/index-*.css` | 88,738 | 17,764 | Stylesheet bundle |
| `index.html` | 1,893 | 870 | Entry HTML + splash |
| **Total (`17` files)** | **1,477,304** (1.41 MiB) | **693,822** (677.6 KiB) | |

### 2. Rust Build, Tests, Binary & Installer Sizes

| Check / Build Step | Baseline Time | Result |
|---|---|---|
| `cargo fmt --all --check` | 1.313 s | Pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | 155.262 s | Pass (0 warnings) |
| `cargo test --workspace` (clean) | 1,417.661 s (23m 38s) | 290 passed, 0 failed, 2 ignored |
| `pnpm tauri build` (clean, failed on missing local signing key) | 772.597 s (12m 09s) | Exit 1 at updater signing; installers built |
| `pnpm tauri build --config scratch/no_updater.json` | 312.905 s | Exit 0 |
| Incremental `cargo check -p vuoom` (`commands.rs` comment edit) | 32.626 s | Pass |
| Incremental `cargo build --release -p vuoom` (`commands.rs` comment edit) | 312.682 s (5m 13s) | Pass (`crate-type = ["staticlib","cdylib","rlib"]` + MSVC `link.exe` + thin LTO) |

| Artifact | Size (Bytes) | Size (Human) | PE Section Breakdown (`vuoom.exe`) |
|---|---|---|---|
| `target/release/vuoom.exe` | 29,784,064 | 28.40 MiB | `.text` 20.32 MiB, `.rdata` 7.11 MiB, `.pdata` 734 KiB, `.data` 173 KiB |
| `Vuoom_2.1.2_x64_en-US.msi` | 10,948,608 | 10.44 MiB | WiX MSI installer |
| `Vuoom_2.1.2_x64-setup.exe` | 7,932,566 | 7.56 MiB | NSIS installer |

### 3. 4K Engine Benchmarks (`cargo test --profile perf -p vuoom --lib perf_4k -- --ignored --nocapture`)

Total compile + run: `473.88 s` (`perf_4k` execution time: `32.65 s`).

| Benchmark | Write / Export Throughput | Read / Composite | Seek / Encode | Memory / Frame | Adapter Selected |
|---|---|---|---|---|---|
| `store 4K typing` | 210 fps | 571 fps | 24 ms | 0.0 MB/frame | — |
| `store 4K typing, no rows` | 115 fps | 1,594 fps | 13 ms | 0.0 MB/frame | — |
| `store 4K scrolling` | 46 fps | 101 fps | 126 ms | 0.1 MB/frame | — |
| `store 4K video` | 51 fps | 102 fps | 115 ms | 8.4 MB/frame | — |
| `export plain 3840x2160` | 17.0 fps | 51 ms composite | 6 ms/frame encode | — | Intel HD Graphics 630 (Vulkan, IntegratedGpu) |
| `export framed 4094x2304` | 17.6 fps | 46 ms composite | 9 ms/frame encode | — | Intel HD Graphics 630 (Vulkan, IntegratedGpu) |

> **GPU Adapter Finding**: `wgpu::RequestAdapterOptions::default()` (`PowerPreference::None`) + `InstanceDescriptor::default()` (`Backends::all()`) picked `Intel(R) HD Graphics 630 (Vulkan, IntegratedGpu)` instead of DX12 / discrete NVIDIA Quadro M1200.

### 4. Startup, Idle Power & Process Footprints (`target/release/vuoom.exe`)

**Launch timings** (`runtime_baseline.json`):
- `cold_1`: **2,024.8 ms** (HWND visible)
- `warm_1`: **354.1 ms**
- `warm_2`: **480.1 ms**
- `warm_3`: **213.3 ms** (median warm: **354.1 ms**)

**30-second Idle Measurements** (`idle_baseline_fixed.json`, fixed to capture all 6 `msedgewebview2.exe` processes matching `dev.vuoom.desktop`):

| Window State | Duration | Rust CPU (1-core / Sys) | WebView2 CPU (1-core / Sys) | Total CPU (1-core / Sys) | Rust Ctx/s | WebView2 Ctx/s | Rust WS / Priv | WebView2 WS / Priv (6 procs) | Total WS / Priv |
|---|---|---|---|---|---|---|---|---|---|
| Focused | 34.23 s | 0.456% / 0.057% | 15.884% / 1.986% | **16.341%** / 2.043% | 67.5 | 782.6 | 252.93 / 175.11 MB | 342.81 / 113.30 MB | 595.74 / 288.41 MB |
| Unfocused | 34.07 s | 0.275% / 0.034% | 15.730% / 1.966% | **16.005%** / 2.001% | 68.4 | 825.4 | 252.88 / 175.07 MB | 345.30 / 111.13 MB | 598.18 / 286.20 MB |
| Minimized | 33.29 s | 0.094% / 0.012% | 9.388% / 1.173% | **9.482%** / 1.185% | 68.9 | 784.0 | 252.84 / 175.05 MB | 344.05 / 109.41 MB | 596.89 / 284.46 MB |

Thread/handle counts at idle focused: Rust `34` threads / `794` handles; WebView2 `138` threads / `2,986` handles.
Before vs. after running the active CDP session (`runtime_baseline.json`): total WS `612.25 MB -> 821.54 MB`, total Priv `321.12 MB -> 507.84 MB` (with sample clip loaded and edited).

### 5. CDP Frontend Startup, Interaction Latencies (1x & 6x CPU) & Leak Check

| Metric / Flow | Live WebView2 (`1x`) | Live WebView2 (`6x`) | Mock Headless Edge (`1x`) | Mock Headless Edge (`6x`) |
|---|---|---|---|---|
| First usable wall (`reload -> splash hidden`) | 432.49 ms | — | 586.44 ms | — |
| `domInteractive` / `DCL` / `load` | 17.7 / 75.2 / 102.5 ms | — | 57.5 / 100.4 / 100.9 ms | — |
| First Paint / FCP | 60.0 / 60.0 ms | — | 84.0 / 84.0 ms | — |
| Startup long tasks (>50 ms) | 0 | — | 2 (`223 ms`, `103 ms`) | — |
| `load_sample_clip` action / next-paint | 1,887.5 / 1,912.2 ms (1 LT: `84 ms`) | — | N/A (`&demo=1`) | — |
| `play_toggle` action / next-paint | 0.5 / 31.5 ms | 3.1 / 45.9 ms | 2.1 / 19.3 ms | 4.3 / 74.9 ms |
| `pause_toggle` action / next-paint | 0.9 / 24.2 ms | 3.6 / 26.5 ms | 1.0 / 18.3 ms | 8.4 / 136.0 ms (`141 ms` LT) |
| `timeline_scrub_click` action / next-paint | 12.5 / 31.4 ms | 29.1 / 92.5 ms | 10.2 / 43.4 ms | 17.0 / 189.4 ms |
| `inspector_tab_switch` action / next-paint | 9.0 / 105.6 ms | 5.2 / 73.3 ms (`70`, `56 ms` LTs) | 8.8 / 37.7 ms | 150.4 / 464.1 ms (`152`, `312 ms` LTs) |
| `add_zoom_and_delete` action / next-paint | 129.6 / 159.9 ms | 328.4 / 360.8 ms (`79 ms` LT) | 130.7 / 157.5 ms | 331.1 / 374.0 ms (`161`, `148 ms` LTs) |
| `open_export_dialog` action / next-paint | 33.1 / 44.5 ms | 53.1 / 87.1 ms (`54 ms` LT) | 29.0 / 36.3 ms | 61.4 / 116.3 ms (`64 ms` LT) |
| `export_switch_mp4_to_gif` action / next-paint | 90.9 / 121.1 ms | 118.5 / 158.3 ms | 85.7 / 91.6 ms | 120.0 / 178.5 ms |

**20x Heavy Cycle Leak Check** (`Performance.getMetrics` + DOM node count):
- **Live WebView2**: `domBefore: 767 -> domAfter: 767` (`+0`), `JSEventListeners: 63 -> 60`, `LayoutObjects: 1001 -> 1001`, `Nodes: 2023 -> 1881`, `JSHeapUsedSize: 2.82 MB -> 3.25 MB`.
- **Mock Headless Edge**: `domBefore: 435 -> domAfter: 555` (`+120` nodes, `+6` nodes/iteration), `JSEventListeners: 70 -> 130`, `JSHeapUsedSize: 2.93 MB -> 3.37 MB`.

### 6. Baseline Dependency & Security Scans (`v2.1.2`)

- `cargo-deny check`: **FAILED** (`advisories FAILED`, `licenses FAILED`):
  - License error: `jpeg-encoder v0.7.1` (`(MIT OR Apache-2.0) AND IJG`) rejected.
  - Advisories: `url v2.5.8` (`RUSTSEC-2026-0592`), plus `zip v4.6.1` (`RUSTSEC-2026-0429`, `RUSTSEC-2026-0545`, `RUSTSEC-2026-0548` unignored; `[graph]` missing `targets`).
  - Yanked warnings: `clipboard-win 5.4.1`, `swash 0.2.8`, `async-executor 1.14.0` (Linux-only).
- `cargo-audit`: `9` vulnerabilities (`quick-xml` ×2, `url`, `zip` ×6) and `23` warnings (unsound: `event-listener`, `glib`, `lru` ×2, `memmap2`, `rustix`; yanked: `async-executor`, `clipboard-win`, `swash`; unmaintained transitive crates).
- `cargo-machete`: Flagged `tracing` (`vuoom-captions`, `vuoom-encode`, `vuoom-render`), `thiserror` (`vuoom-input`, `vuoom-preview`, `vuoom-render`), `objc2-core-foundation` (`vuoom-camera`), `objc2-foundation` (`vuoom-input` — false positive on macOS `mac/cursor.rs`), `tauri-build` (`vuoom` build-dep).
- `cargo tree -d`: Duplicate `color_quant` (`1.1.0` via `gif` + `2.0.0` direct in `vuoom-encode`), plus transitive duplicates (`base64`, `bitflags`, `foldhash`, `getrandom`, `hashbrown`, `indexmap`, `ttf-parser`, `thiserror`, `toml`, `windows` 0.54/0.58/0.61/0.62, etc.).
- `pnpm audit`: `19` advisories (`1` critical, `11` high, `6` moderate, `1` low) in `seroval` (build-time/SSR only, 0 bytes in `dist/`), `postcss`, `nanoid`, `source-map-js`, `browserslist`, `baseline-browser-mapping`, `vite`.

---

## Optimization Log

| Date / Commit | ID | Area | Change | Metric Before | Metric After | Delta | Verified On |
|---|---|---|---|---|---|---|---|
| 2026-11-23 | E1, E3 | Rust build & link | Drop `staticlib`/`cdylib` in `src-tauri/Cargo.toml`; add `debug="line-tables-only"` + dep `debug=false`; `.cargo/config.toml` `linker="rust-lld"` on Win64 MSVC | `clippy` 155.26 s; `test` 1,417.66 s; inc release 312.68 s; `vuoom.exe` 29,784,064 B | `clippy` 106.92 s; `test` 956.84 s; inc release 264.59 s (`--timings`, isolated; 481.88 s under back-to-back thermal load); `vuoom.exe` 29,754,368 B | **-31.1%** clippy, **-32.5%** test, **-15.4%** inc release (-48.1 s), -29.7 KB binary | Win10 (`cargo fmt`, `clippy`, 290/290 tests, `tauri build --no-bundle`, HWND `83.9 ms`) |
| 2026-11-23 | F4, A6 | Idle power / IPC | Skip `permissions` IPC, `focus` listener & 4s timer on Windows (`!isMac`); on macOS stop polling when granted & skip while `document.hidden` (`Permissions.tsx`) | Rust idle: `0.456%` focused / `0.094%` min; Total idle: `16.341%` focused / `9.482%` min; ~900 Windows IPCs/hr | Rust idle: `0.188%` focused / `0.000%` min; Total idle: `13.141%` focused / `7.309%` min; **0** Windows IPCs | **-58.8%** Rust focused CPU, **0.00%** Rust min CPU, **-19.6%** total focused / **-22.9%** min CPU | Win10 packaged `vuoom.exe` (`idle_after_f4.json`), `tsc`, `biome`, `check-i18n`, `vite build` |
