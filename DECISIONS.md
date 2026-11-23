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
