# Vuoom Modernization Log (2026-11-23)

Tracks what was upgraded in `v2.2.0`, what remains outdated (and why), how open Dependabot PRs relate to `main`, and the prioritized next upgrades.

---

## 1. Completed Upgrades (`v2.1.2` -> `v2.2.0`)

### Rust Workspace & Toolchain

| Crate / Config | Before (`v2.1.2`) | After (`v2.2.0`) | Why & Impact |
|---|---|---|---|
| `rust-version` (`Cargo.toml`) | `"1.80"` (stale; code already used 1.88 APIs) | `"1.90"` | Matches `tauri 2.12.7` / `tauri-plugin-updater 2.14.1` declared MSRV |
| `tauri` / `tauri-build` / `tauri-utils` | `2.11.2` / `2.6.3` / `2.9.3` | `2.12.7` / `2.7.6` / `2.10.4` | Lockstep desktop runtime update; drops legacy Windows 7/8 shims |
| `tauri-runtime` / `tauri-runtime-wry` | `2.11.2` / `2.11.2` | `2.12.4` / `2.12.5` | WebView2/Tao fixes paired with Tauri `2.12.7` |
| `tauri-plugin-updater` | `2.10.1` | `2.14.1` | Pulls `zip 9.8.2` (was `4.6.1`), resolving all 6 `zip 4.x` security advisories |
| `tauri-plugin-dialog` / `fs` / `opener` / `process` / `single-instance` | `2.7.1` / `2.5.1` / `2.5.4` / `2.3.1` / `2.4.2` | `2.8.4` / `2.7.1` / `2.7.3` / `2.4.0` / `2.5.5` | Lockstep Tauri `2.12` plugin compatibility |
| `zip` (transitive via `tauri-plugin-updater`) | `4.6.1` | `9.8.2` | Eliminated 6 `RUSTSEC-2026-*` advisories and 6 ignores from `deny.toml` |
| `url` | `2.5.8` | `2.5.9` | Patched `RUSTSEC-2026-0592` (`Url::origin()` stack overflow on nested `blob:`) |
| `cookie` | `0.18.1` | `0.18.3` | Patched `RUSTSEC-2026-0594` |
| `clipboard-win` / `swash` | `5.4.1` / `0.2.8` | `5.4.3` / `0.2.10` | Replaced yanked patch versions flagged by `cargo-deny` |
| `event-listener` / `memmap2` / `rustix` | `5.4.1` / `0.9.10` / `1.1.4` | `5.4.2` / `0.9.12` / `1.1.6` | Patched unsoundness/bugfix releases flagged by `cargo-audit` |
| `color_quant` (via `gif 0.13`) | `1.1.0` + `2.0.0` (duplicate) | `2.0.0` only | Disabled `gif` default features (`raii_no_panic`, `std` kept) |
| Build & linker config | MSVC `link.exe`, full dev debuginfo, `["staticlib","cdylib","rlib"]` | Win64 `rust-lld`, dev `debug="line-tables-only"`, deps `debug=false`, `["rlib"]` | `-31.1%` Clippy, `-32.5%` tests, `-15.4%` isolated inc release |
| Unused direct deps | `tracing` (×4), `thiserror` (×3), `objc2-core-foundation` (`vuoom-camera`), runtime tokio `macros` (`vuoom-preview`) | Removed or moved to `[dev-dependencies]` / macOS target | Clean `cargo-machete` (`0` unused) + fixes Rust `1.100` `unused_dependencies` |

### Frontend (`package.json`, `vite.config.ts`, `tsconfig.json`, `biome.json`)

| Package / Config | Before (`v2.1.2`) | After (`v2.2.0`) | Why & Impact |
|---|---|---|---|
| `@tauri-apps/api` / `@tauri-apps/cli` | `2.11.1` / `2.11.4` | `2.12.5` / `2.12.6` | Lockstep match with Rust `tauri 2.12.7` |
| `@tauri-apps/plugin-{dialog,opener,process,updater}` | `2.7.1` / `2.5.4` / `2.3.1` / `2.10.1` | `2.8.4` / `2.7.3` / `2.4.0` / `2.14.1` | Lockstep match with Rust plugins |
| `vite` / `vite-plugin-solid` | `8.1.4` / `2.11.12` | `8.3.8` / `2.11.17` | Resolves Vite/launch-editor/postcss/nanoid dev advisories; `-31.9%` Vite build time |
| `@biomejs/biome` / `biome.json` | `2.5.3` | `2.6.8` | Latest formatter/linter (`biome check src`: `288 ms`) |
| `typescript` / `tsconfig.json` | `~5.6.2` (`5.6.3`, `ES2020`) | `~5.9.3` (`5.9.3`, `ES2023`) | Modern TS 5.9 checks (`ArrayBuffer` typed-array generics in `src/preview.ts`) |
| `@fontsource-variable/inter` | `5.2.8` | `5.3.0` | Latest variable Inter font files |
| Vite production packaging | Dynamic `./mock/install` shipped in packaged builds + `modulePreload` polyfill | Build-time `!__TAURI_BUILD__` tree-shaking + `modulePreload.polyfill: false` | `-41,608 B` (`-2.8%`) raw / `-14,032 B` (`-2.0%`) gzip packaged `dist/` |

---

## 2. Dependabot PRs Status

- **PR #104** (`dependabot/npm_and_yarn/npm-minor-patch-24ed665fdd`, open): **Superseded** — Wave 1 Commit D (`3b22758`) upgrades all 10 packages to equal or newer versions (`vite-plugin-solid 2.11.17` vs `2.11.16`, `@tauri-apps/cli 2.12.6` vs `2.12.5`) and fixes CI.
- **PR #106** (`dependabot/cargo/cargo-minor-patch-906a784778`, open): **Superseded on Tauri/core patches & CI fixes** — `main` ships `tauri 2.12.7` (newer than PR #106's `2.11.6`), matching plugin updates, and the Rust `1.100` Clippy/Cargo.toml fixes that failed PR #106's CI. Remaining safe Cargo minor/patch updates (`windows-capture 2.0.1`, `serde_json 1.0.154`, etc.) can be picked up once Dependabot rebases.

---

## 3. Still Outdated / Deferred & Why

| Dependency | Current | Available | Reason Deferred |
|---|---|---|---|
| `wgpu` (`vuoom-render`) | `25.0.2` | `29.x` / `30.x` | **Tier 3 breaking rewrite**: breaks `DeviceDescriptor`, `BufferSlice::map_async`, `wgpu-hal` DX12/Metal interop, and WGSL layout rules. |
| `glyphon` (`vuoom-render`) | `0.9.0` | `0.12.0` | Coupled to `wgpu` major bump (Dependabot `glyphon-0.12.0` branch cannot land until `wgpu` upgrades). Also keeps transitive `lru 0.12.5` (`unmaintained`/`unsound` warnings), `rustybuzz 0.14.1`, `ttf-parser 0.20/0.21`. |
| `solid-js` | `1.9.14` | `2.0.3` | **Tier 3 breaking rewrite**: new `createAsync`/reactivity model; keep `1.9.x` across Track 1 & Track 2 until ready for dedicated migration. |
| `typescript` (dev) | `5.9.3` | `6.0.x` / `7.0.2` | `7.0` (`tsgo` native compiler) is a major toolchain shift; evaluate in a dedicated dev-tooling PR after `v2.2.0`. |
| `quick-xml` (transitive) | `0.39.4` | `>=0.41.0` | Pulled transitively by `tauri-utils` / `plist` for local trusted XML/manifests (`RUSTSEC-2026-0194/0195` ignored in `deny.toml` until Tauri loosens constraint). |
| `browserslist`, `baseline-browser-mapping`, `seroval` (npm dev/SSR) | transitive | — | Pulled only by build/dev tooling (`solid-js` SSR / Vite plugins); `0` bytes shipped in `dist/` (verified `pnpm audit` `19 -> 9` remaining advisories, all build-time only). |
| `gif` (`vuoom-encode`) | `0.13.3` | `0.14.2` | Minor breaking API audit needed; keep on `0.13` without default features for Track 1 stability. |
| `tokio-tungstenite` (`vuoom-preview`) | `0.29.0` | `0.30.0` | Protocol/API check deferred cleanly until after `v2.2.0`. |

---

## 4. Prioritized Next Upgrades

1. **P1 — Biome config schema cleanup**: Run `pnpm exec biome migrate` to replace deprecated `linter.rules.recommended` field syntax flagged during `biome check src`.
2. **P2 — `gif 0.14.x` & `tokio-tungstenite 0.30.x`**: Low-risk engine crate bumps once Wave 2 / Tier 2 runtime changes start.
3. **P3 — `wgpu` + `glyphon` major upgrade (Tier 3)**: Eliminates remaining `lru` / `ttf-parser` / `windows` duplicate transitive trees and enables modern GPU APIs.
4. **P4 — `typescript 7.x` (`tsgo`) evaluation**: Faster typechecking once `vite-plugin-solid` and ecosystem types confirm compatibility.
5. **P5 — `solid-js 2.0` migration (Tier 3)**: Dedicated migration after Track 2 UI Redesign (`v2.3.0`).
