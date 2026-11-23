# Autonomous Decisions Log

All autonomous technical and workflow decisions made during the Vuoom optimization (Track 1, `v2.2.0`) and UI redesign (Track 2, `v2.3.0`) session are logged here.

---

### D-001 — Strict Separation of Track 1 (`v2.2.0`) and Track 2 (`v2.3.0`)
- **Date**: 2026-11-23
- **Context**: User requested both full optimization/modernization and a full UI redesign while ensuring either track can be rolled back independently.
- **Options Considered**:
  1. Interleave UI and performance fixes together before releasing.
  2. Complete Track 1 (`v2.1.2 -> v2.2.0`) with zero visible UI changes first, tag/release `v2.2.0` as a Draft release, create rollback tag `pre-ui-redesign-2026-11-23`, and then execute Track 2 (`v2.2.0 -> v2.3.0`) on `ui/redesign`.
- **Decision**: Option 2.
- **Rationale**: Preserves clean git blame and exact before/after attribution between engine/frontend performance wins and visual redesign changes.

### D-002 — Preserve Auto-Updater Safety (Draft Releases Only)
- **Date**: 2026-11-23
- **Context**: `src-tauri/tauri.conf.json` polls `releases/latest/download/latest.json`, and `.github/workflows/release.yml` sets `releaseDraft: true`.
- **Decision**: Never publish GitHub releases during autonomous runs; keep `v2.2.0` and `v2.3.0` as Draft releases triggered automatically by pushing version bumps to `main` (without pre-creating tags, which would cause `check-and-bump` to skip).
- **Rationale**: Prevents unverified overnight changes from auto-updating live users.

### D-003 — Local Toolchain & Unsigned Release Test Config
- **Date**: 2026-11-23
- **Context**: `whisper-rs-sys` requires `libclang.dll` + CMake, and local `pnpm tauri build` exits 1 at artifact signing because `TAURI_SIGNING_PRIVATE_KEY` only exists in GitHub Secrets. Furthermore, a previous long path triggered Win32 error 206 (`ERROR_FILENAME_EXCED_RANGE`).
- **Decision**: Provisioned portable `libclang.dll` (`18.1.1`) in `$env:TEMP\vuoom-lc` (short path), VS Build Tools 2022 CMake (`3.31.6`) via `PATH`, and passed `--config scratch/no_updater.json` (`bundle.createUpdaterArtifacts: false`) locally instead of editing tracked `tauri.conf.json`.
- **Rationale**: Enables reproducible clean local `cargo test` and packaged `vuoom.exe` release benchmarking without polluting tracked repository config.

### D-004 — Use `[skip ci]` on Intermediate Phase 5 Commits
- **Date**: 2026-11-23
- **Context**: `.github/workflows/release.yml` triggers multi-platform Windows + macOS release builds on every non-`website/**` push to `main`.
- **Decision**: Use `[skip ci]` on intermediate atomic Tier 1 / Tier 2 commits while running the full local check suite on every commit, and omit `[skip ci]` on milestone verification commits (such as after dependency/CI fixes) and final release pushes.
- **Rationale**: Avoids launching 15+ redundant parallel release workflows while still verifying cross-platform macOS compilation and `cargo-deny` cleanly in CI.

### D-005 — Keep `panic = "unwind"` in Release Profile
- **Date**: 2026-11-23
- **Context**: Evaluating `[profile.release]` options (`panic = "abort"` vs default `"unwind"`).
- **Decision**: Retain default `panic = "unwind"`.
- **Rationale**: Vuoom installs a custom panic hook (`src-tauri/src/lib.rs:151`), recovers from worker thread joins (`zoom_chord.rs:126`), and recovers poisoned mutexes (`.unwrap_or_else(|e| e.into_inner())`). `panic = "abort"` would turn recoverable worker failures into full process crashes.

### D-006 — Retain `objc2-foundation` in `crates/vuoom-input/Cargo.toml`
- **Date**: 2026-11-23
- **Context**: `cargo-machete` flagged `objc2-foundation` in `vuoom-input` because its usage lives inside macOS-only `src/mac/cursor.rs:18-19` (`NSCursor::hotSpot()`, `image().size()`).
- **Decision**: Keep `objc2-foundation` and ignore it in `[package.metadata.cargo-machete]`. Drop genuinely unused `objc2-core-foundation` in `vuoom-camera/Cargo.toml` (0 matches in `crates/vuoom-camera/src`).
- **Rationale**: Removing `objc2-foundation` would break macOS CI compilation (`cargo clippy -p vuoom-input`).

### D-007 — Defer `wgpu`/`glyphon` Major Bump & `solid-js` 2.0 to Tier 3; Defer Home Idle Animation Fix to Track 2
- **Date**: 2026-11-23
- **Context**: Upgrading `wgpu` (25->30), `glyphon` (0.9->0.12), and `solid-js` (1.9->2.0) involves breaking APIs. Meanwhile, fixed baseline idle measurement (`idle_baseline_fixed.json`) showed WebView2 burning 15.88% of a CPU core at idle focused (9.39% minimized) due to infinite CSS animations in `src/styles/home.css` (`.home-inner::before` `glow-drift` + `.rec-dot.live` `box-shadow` `live` pulse).
- **Decision**: Classify `wgpu`/`glyphon` and `solid-js 2.0` as Tier 3 proposals for `MORNING_REPORT.md`. Keep visible Home animations untouched during Track 1 to honor zero visible UI changes, and fix the idle CSS animations during Track 2 (`MOTION.md` / Phase U4–U5).
- **Rationale**: Avoids risky breaking rewrites and keeps performance vs. visual changes cleanly isolated.

### D-008 — Keep `src-tauri/src/mock` Available for Browser Preview & Track 2 Audits
- **Date**: 2026-11-23
- **Context**: `dist/assets/install-*.js` (`41.4 KB` raw / `14.0 KB` gz) is bundled into `dist/` even for Tauri desktop builds.
- **Decision**: Gate `import("./mock/install")` behind build-time `!__TAURI_BUILD__` (`Boolean(process.env.TAURI_ENV_PLATFORM)`) in `vite.config.ts` / `src/index.tsx` rather than removing mock mode.
- **Rationale**: Packaged Windows/macOS `.exe`/`.app` builds shed the dead mock chunk, while `pnpm build`, `pnpm dev`, and `pnpm serve ?mock=1` remain 100% intact for development and automated headless Edge screenshots/profiling.

### D-009 — Disk Hygiene on C: Build Partition (`15.73 GB` Reclaimed)
- **Date**: 2026-11-23
- **Context**: Accumulation across clean debug/test/perf/release Cargo profiles filled `C:` down to `0.01 GB`, causing `rust-lld` to fail linking with `no space on device`.
- **Decision**: Keep `D:` untouched unless needed, and periodically clean stale `target/debug`, `target/perf`, dead `vuoom_lib.{dll,lib,exp}` release artifacts left from `E1`, and temporary perf/Edge directories in `%LOCALAPPDATA%\Temp` (preserving `target/release` compiled `whisper-rs-sys`), reclaiming `15.73 GB` free on `C:`. After Windows cleaned `%LOCALAPPDATA%\Temp\vuoom-lc` mid-build, moved portable `libclang.dll` into the gitignored `.ci-lc` short folder (to be deleted cleanly before final release completion).
- **Rationale**: `line-tables-only` + dep `debug = false` rebuilds `target/debug` significantly smaller, keeping all build artifacts cleanly inside the workspace while maintaining ample disk headroom.

### D-010 — Accept `+433 KB` Binary Size Increase for `tauri 2.12.7` / `zip 9.8.2` Security Fix
- **Date**: 2026-11-23
- **Context**: Upgrading Tauri in lockstep (`2.11.2 -> 2.12.7` + `tauri-plugin-updater 2.14.1`) upgraded `zip 4.6.1 -> 9.8.2`, eliminating all 6 `zip` CVEs (`RUSTSEC-2026-0367/0368/0369/0429/0545/0548`) and raising `rust-version` requirement to `1.90`, while growing `target/release/vuoom.exe` from `29,784,064 B` (`28.40 MiB`) to `30,217,216 B` (`28.82 MiB`, `+433 KB` / `+1.45%`, slightly above the `29.8 MB` baseline budget).
- **Decision**: Keep the `tauri 2.12.7` + `zip 9.8.2` upgrade and adjust `Cargo.toml` `rust-version = "1.90"`.
- **Rationale**: Follows the non-negotiable rule "never trade security for speed" — keeping an unpatched `zip 4.x` tree solely to avoid `+433 KB` would leave 6 security advisories in `Cargo.lock`.

### D-011 — Fix Pre-Existing Rust `1.100` CI Lints Found in Dependabot PR #106
- **Date**: 2026-11-23
- **Context**: GitHub Actions `dtolnay/rust-toolchain@stable` runs Rust `1.100.0` (newer than local `1.99.0`). Inspecting PR #106 CI logs revealed `clippy::duplicated_attributes` (`-D warnings` error on `src-tauri/src/session.rs:3172-3173`) and `cargo::unused_dependencies` warning for `tracing` in `crates/vuoom-input/Cargo.toml` on Windows (since `vuoom-input` only calls `tracing::warn!` in `src/mac/recorder.rs`).
- **Decision**: Removed the duplicate `#[allow(clippy::too_many_arguments)]` line in `src-tauri/src/session.rs` and moved `tracing.workspace = true` in `crates/vuoom-input/Cargo.toml` into `[target.'cfg(target_os = "macos")'.dependencies]`.
- **Rationale**: Prevents `CI` (`windows-latest` + `macos-14`) from failing on Rust `1.100.0` while keeping macOS warning logging intact.

### D-012 — Wrap Up Track 1 at Wave 1 Completion and Ship `v2.2.0` Draft Release
- **Date**: 2026-11-23
- **Context**: User directed: "continue please and wrap up what is done until now and a merge everything and cut a new release." No unmerged local feature branches exist (`refactor/explicit-state-machines` is already squash-merged as `0cc4666` via PR #101), and open Dependabot PRs #104 / #106 are superseded by Wave 1 Commits C & D.
- **Decision**: Conclude Track 1 cleanly after Wave 1 (`v2.2.0` Draft release via `release.yml`), document remaining Wave 2 / Tier 2 / Tier 3 optimizations and Track 2 UI Redesign in `OPTIMIZATION_PLAN.md`, `PERFORMANCE.md`, `MODERNIZATION.md`, and `MORNING_REPORT.md` for follow-up sessions, and keep open Dependabot PRs #104/#106 unmerged so Dependabot auto-closes/rebases cleanly after `main` pushes.
- **Rationale**: Ships verified build/idle/bundle/security wins safely without rushing unbenchmarked Tier 2 engine refactors into the release.
