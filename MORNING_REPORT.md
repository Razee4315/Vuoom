# Morning Report — Vuoom `v2.2.0` (Track 1 Wave 1 Wrap-Up)

## Release & Rollback

- **Release**: [`Vuoom v2.2.0`](https://github.com/Razee4315/Vuoom/releases/tag/v2.2.0) (tag `v2.2.0` at `6eebabe`)
- **Status**: **Published (`Latest`)** — published on `2026-11-23T14:19:05Z`; auto-updater endpoint (`https://github.com/Razee4315/Vuoom/releases/latest/download/latest.json`) verified returning `"version": "2.2.0"` (`windows-x86_64` + `darwin-aarch64`).
- **Verified Release Assets (`latest.json` + Windows + macOS Apple Silicon)**:
  - `Vuoom_2.2.0_x64_en-US.msi` (`11,161,600 B`) + `.sig` (`432 B`)
  - `Vuoom_2.2.0_x64-setup.exe` (`8,074,028 B`) + `.sig` (`432 B`)
  - `Vuoom_2.2.0_aarch64.dmg` (`9,276,495 B`)
  - `Vuoom_2.2.0_aarch64.app.tar.gz` (`9,539,390 B`) + `.sig` (`420 B`)
  - `latest.json` (`3,259 B`)
- **CI & Release Runs (All Green)**:
  - **CI** (`45186750099`): `frontend` (`17s`), `cargo-deny` (`33s`), `ci` (`windows-latest`, `3m3s`), `macos` (`macos-15`, `13m2s`) — [View Run](https://github.com/Razee4315/Vuoom/actions/runs/45186750099)
  - **Release** (`45186749194`): `check-and-bump` (`5s`, `v2.2.0`), `release (windows-latest)` (`6m50s`), `release (macos-15)` (`7m18s`) — [View Run](https://github.com/Razee4315/Vuoom/actions/runs/45186749194)
- **Rollback tag**: `pre-optimization-2026-11-23` (`8e07d9b`, pushed on `origin`)
- **Note on scope (`D-012`)**: Concluded Track 1 after completing and verifying **Wave 1** (build toolchain, idle Windows IPCs, Rust/JS dependency & security modernization, packaged bundle mock removal, and Rust `1.100` CI lint fixes), plus documentation, version sync (`2.2.0`), and release cut. **Wave 2** (`F1–F9`, `A1–A4`, `B3/B5`, `C2/C7`), **Tier 2** (`B1/B2`, `C1`, `C5/C6`), and **Track 2 UI Redesign** remain documented and queued in [`OPTIMIZATION_PLAN.md`](./OPTIMIZATION_PLAN.md).

---

## Before / After Metrics (Wave 1 Verified)

| Metric | Baseline (`v2.1.2` / `8e07d9b`) | `v2.2.0` (Wave 1) | Delta |
|---|---|---|---|
| Clean `cargo clippy --workspace --all-targets -- -D warnings` | `155.26 s` | `106.92 s` | **-31.1%** (`-48.3 s`) |
| Clean `cargo test --workspace` (`290` passed, `2` ignored) | `1,417.66 s` | `956.84 s` | **-32.5%** (`-460.8 s`) |
| Isolated incremental `cargo build --release -p vuoom` | `312.68 s` | `264.59 s` (`--timings`, isolated; `481.88 s` under thermal load) | **-15.4%** (`-48.1 s`) |
| Vite internal production build (`dist/`) | `3.57 s` (`88` modules) | `2.43 s` (`81` modules, packaged) | **-31.9%** |
| Packaged `dist/` total size | `1,477,304 B` raw / `693,822 B` gz (`17` files) | `1,435,696 B` raw / `679,790 B` gz (`16` files) | **-41,608 B (-2.8%)** raw / **-14,032 B (-2.0%)** gz |
| Rust host CPU at idle (focused / minimized) | `0.456%` / `0.094%` single-core | `0.188%` / `0.000%` single-core | **-58.8%** focused / **-100%** minimized (`0` Win IPCs/hr) |
| Total CPU incl. WebView2 at idle (focused / minimized) | `16.341%` / `9.482%` single-core | `13.141%` / `7.309%` single-core | **-19.6%** focused / **-22.9%** minimized |
| `cargo-deny check` / `cargo-machete` | **FAIL** / `8` unused flags | **PASS** (all 4 checks ok) / `0` unused | Green CI audit; `color_quant 1.1.0` removed |
| `cargo-audit` / `pnpm audit` | `9` Rust vulns (`23` warns) / `19` npm (`1` crit) | `2` Rust vulns (`13` warns) / `9` npm (dev-only) | **-7** Rust vulns / **-10** npm advisories |
| Release `vuoom.exe` size | `29,784,064 B` (`28.40 MiB`) | `30,217,216 B` (`28.82 MiB`) | `+433 KB` (`+1.45%`) for `tauri 2.12.7` + `zip 9.8.2` (`D-010`) |

> Runtime CDP interaction benchmarks, cold/warm 4-run launch stats, 20x leak cycle, and `perf_4k` GPU benchmarks from baseline `v2.1.2` (`PERFORMANCE.md`) were not re-run in Wave 1 because Wave 2 / Tier 2 runtime/GPU refactors (`F1–F9`, `A1–A5`, `B1/B2`, `C1–C7`) were not yet applied.

---

## What Was Done (Commits on `main`)

1. **`58eca12`** — **Phase 0–4 Baseline, Research & Plan**: Full baseline benchmark suite + dual Opus 5.5 read-only audits (`PERFORMANCE.md`, `RESEARCH.md`, `OPTIMIZATION_PLAN.md`, `BEHAVIOR_NOTES.md`, `DECISIONS.md`).
2. **`f6f0570`** — **Wave 1A (`E1`, `E3`)**: Dropped mobile `staticlib`/`cdylib` crate types in `src-tauri/Cargo.toml`, added Win64 `rust-lld` in `.cargo/config.toml`, set dev `debug = "line-tables-only"` (`debug = false` on deps) in `Cargo.toml` (**-31.1%** Clippy, **-32.5%** tests, **-15.4%** isolated incremental release).
3. **`8871332`** — **Wave 1B (`F4`, `A6`)**: Skipped `permissions` IPC/focus/4s polling on Windows in `src/components/Permissions.tsx`; paused macOS polling while hidden and stopped once granted (**-58.8%** Rust focused CPU, **0.00%** Rust minimized CPU, **-19.6% / -22.9%** total idle CPU).
4. **`dcc6195`** — **Wave 1C (`E5–E9`)**: Removed duplicate `color_quant 1.1.0` via `gif` feature trim, removed unused direct deps, fixed `deny.toml` (`[graph] targets`, `jpeg-encoder` `IJG`), and patched `url 2.5.9`, `cookie 0.18.3`, `clipboard-win 5.4.3`, `swash 0.2.10`, `event-listener 5.4.2`, `memmap2 0.9.12`, `rustix 1.1.6` (`cargo-deny` green, `0` unused deps).
5. **`3b22758`** — **Wave 1D (`F10-T1` + Rust `1.100` CI fixes)**: Excluded `install-*.js` mock chunk (`41.4 KB`) from packaged Tauri builds via `__TAURI_BUILD__` (`vite.config.ts`, `src/index.tsx`), disabled `modulePreload` polyfill, lockstep-upgraded Tauri `2.12.7` / plugins (`zip 9.8.2` clearing all 6 `zip 4.x` CVEs and removing 6 `deny.toml` ignores), upgraded Vite `8.3.8` / Biome `2.6.8` / TS `5.9.3` / Inter `5.3.0`, fixed `src/preview.ts` TS 5.9 generics, and fixed Rust `1.100` CI lints (`session.rs:3173` duplicate allow, `vuoom-input` `tracing` macOS-only, MSRV `1.90`).
6. **`6123dac`** — **Phase 7 Documentation**: Recorded Wave 1 performance summary, modernization log, efficiency rules (`AGENTS.md`), and wrap-up report.
7. **`1ddbb83`** — **Release `2.2.0` Prep**: Synchronized `2.2.0` across `Cargo.toml`, `Cargo.lock`, `package.json`, and `src-tauri/tauri.conf.json`, and updated `CHANGELOG.md`.
8. **`6eebabe`** — **CI Runner Migration (`D-013`)**: Updated `.github/workflows/{ci,release}.yml` from retired `macos-14` to `macos-15`, unlocking full green Windows + macOS Apple Silicon CI and Draft release artifacts.

---

## Upgrades Done & Pending (Summary — Details in [`MODERNIZATION.md`](./MODERNIZATION.md))

- **Upgraded**: Tauri `2.11.2 -> 2.12.7` + all JS/Rust plugins in lockstep (`updater 2.14.1` -> `zip 9.8.2`), `url 2.5.9`, `cookie 0.18.3`, `clipboard-win 5.4.3`, `swash 0.2.10`, `vite 8.3.8`, `vite-plugin-solid 2.11.17`, `@biomejs/biome 2.6.8`, `typescript 5.9.3`, `@fontsource-variable/inter 5.3.0`, MSRV `1.90`.
- **Dependabot PRs**: Left PRs #104 (npm) and #106 (Cargo) unmerged because `3b22758` supersedes #104 completely and ships newer Tauri `2.12.7` + Rust `1.100` CI fixes than #106.
- **Pending**: `wgpu 25 -> 30` & `glyphon 0.9 -> 0.12` (Tier 3 breaking APIs), `solid-js 1.9 -> 2.0` (Tier 3), `typescript 7.0`, `gif 0.14`, `tokio-tungstenite 0.30`, transitive `quick-xml 0.39.4` (upstream Tauri), and dev-only `seroval`/`browserslist` (`0` bytes in `dist/`).

---

## Remaining Work Not Executed Here (`OPTIMIZATION_PLAN.md`)

- **Wave 2 Tier 1** (`F1–F9`, `A1–A4`, `B3/B5`, `C2/C7`): Ruler `<Index>` + rAF wheel zoom, `O(1)` annotation drag selectors, eliminating 60fps forced reflows in `createEditor.ts`, `estimate_gif` coalescing, async `spawn_blocking` commands, parallel `rayon` thumbnails, zero-copy `Source::Binary` bundled fonts.
- **Tier 2 Proposals**:
  - `B1/B2`: Parallel `FontSystem::new()` + `Backends::PRIMARY` / `HighPerformance` GPU selection (fixes dual-GPU systems picking Intel HD 630 Vulkan `17 fps` 4K export over NVIDIA dGPU) — **Gain: High, Effort: M, Risk: Med**.
  - `C1`: `Arc`-wrapped immutable event logs / `CameraTrack` in `vuoom-project` (eliminates deep clones on 60fps seek & 100-step undo) — **Gain: High RAM/CPU, Effort: L, Risk: Med**.
  - `C5/C6`: Double-buffered GPU readback + cached `glyphon::TextBuffer` shaping — **Gain: ~1.5–2x export throughput, Effort: L, Risk: Med**.
  - `B4`, `A1-Overlay`, `F10-T2`, `F7-T2`: `"engine-ready"` boot event, async overlay screen-grab, `lazy()` modal splitting, disconnecting hidden editor `PreviewClient` during recording.
- **Tier 3 Proposals**: `wgpu 30` / `glyphon 0.12`, `solid-js 2.0`, GPU BGRA->NV12 compute shader, native `wgpu` surface / compressed preview, streaming disk audio playback, binary Tauri `Response` IPCs.
- **Track 2 UI Redesign & Motion Pass**: Complete screen-by-screen audit & redesign (`UI_AUDIT.md` -> `v2.3.0`).

---

## UI / Behavior Suggestions (Deferred to Track 2)

1. **Home screen idle CSS animations (`src/styles/home.css`)**: `.home-inner::before` (`620x320` `filter: blur(20px)` infinite `glow-drift`) + `.rec-dot.live` (infinite `box-shadow` repaint) currently keep WebView2 at **12.95% CPU core focused / 7.31% minimized** even after Windows IPC polling is eliminated. Replace with compositor-only finite transitions under `MOTION.md`.
2. **Annotation default font mismatch (`src/editor/constants.ts:14-15`)**: Requests `"Inter, sans-serif"` while `@fontsource-variable/inter` registers `"Inter Variable"`.
3. **Display font format & Inter subsetting**: Converting bundled annotation `.ttf` fonts (~684 KB) to `.woff2` and trimming unused Inter glyph subsets would cut `dist/` roughly in half.

---

## Decisions to Review ([`DECISIONS.md`](./DECISIONS.md))

Review `D-001` through `D-012` — in particular:
- **`D-002`**: Draft-only release policy (ready for you to inspect artifacts and click **Publish release**).
- **`D-010`**: Accepted `+433 KB` binary size growth (`30.22 MB` vs `29.8 MB` budget) to ship `tauri 2.12.7` / `zip 9.8.2` security fixes.
- **`D-012`**: Paused Track 1 at Wave 1 completion to ship `v2.2.0` cleanly before Wave 2 / Tier 2 / Track 2.

---

## Areas Needing Tests Next Session

- `Permissions.tsx`: macOS vs Windows mount behavior, stop-polling-once-granted, and hidden-tab pause (`document.hidden`).
- Build gating (`!__TAURI_BUILD__`): assert `dist/assets/install-*.js` exists in standalone Vite builds and is absent in packaged `tauri build`.
- `recordFlow` state machine (`src/editor/recordFlow.ts` + `commands.rs` sync ordering) and `ExportDialog` `estimate_gif` cancellation/concurrency.
- Dual-GPU adapter preference (`vuoom-render`) and `FrameStore` / `resimulate` concurrency under simultaneous seek + edit + export.

---

## Remaining Risks, Platform Limits & Upstream Issues

- **WebView2 baseline overhead**: 6 `msedgewebview2.exe` processes hold ~345 MB WS / ~113 MB Private RAM at idle and burn ~13% single-core CPU on Home until Track 2 fixes `home.css` continuous `blur`/`box-shadow` animations.
- **Unpatched transitive `quick-xml 0.39.4`**: Ignored in `deny.toml` (`RUSTSEC-2026-0194/0195`); only used on trusted local XML manifests until upstream `tauri-utils` / `plist` bumps to `>=0.41`.
- **Transitive `cargo-audit` warnings**: `lru 0.12.5` (via `glyphon 0.9`), `glib 0.18.5` (Linux-only via `gtk`), `async-executor 1.14.0` (yanked, Linux-only), and `unmaintained` transitive crates (`chrono`, `rustybuzz`, `ttf-parser`, `zvariant`).
- **macOS signing**: As noted in `.github/workflows/release.yml`, Apple Developer ID secrets are not configured, so macOS `.dmg`/`.app` remains ad-hoc signed.
