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

### D-013 — Migrate GitHub Actions Workflows from Retired `macos-14` to `macos-15`
- **Date**: 2026-11-23
- **Context**: GitHub Actions retired `macos-14` runners on 2026-11-02, causing `CI` (`macos`) and `Release` (`release (macos-14)`) to cancel immediately with `The configuration 'macos-14-arm64-us-default' is not supported`. The initial `v2.2.0` run (`45183174930`) therefore uploaded Windows-only assets to an untagged `v2.2.0` draft.
- **Decision**: Updated `.github/workflows/ci.yml` and `.github/workflows/release.yml` from `macos-14` to `macos-15`, deleted the partial untagged `v2.2.0` draft before pushing, and re-ran `CI` (`45186750099`) and `Release` (`45186749194`) on `main`.
- **Rationale**: Restores cross-platform Apple Silicon CI verification and bundles both Windows and macOS Apple Silicon artifacts + unified `latest.json` cleanly into Draft `Vuoom v2.2.0`.

---

### D-014 — UI Redesign Track Branching, Rollback Tag & CI-Only Build Verification
- **Date**: 2026-11-27
- **Context**: Following `v2.4.0` (`2c2db09`), Track 2 UI Redesign begins on a low-spec local Windows machine where full Rust/Tauri release builds and heavy test suites must not run locally.
- **Decision**: Created and pushed rollback tag `pre-ui-redesign-2026-11-27` on `main` (`2c2db09`), created dedicated track branch `ui/redesign`, updated `.github/workflows/ci.yml` to run on `ui/**` pushes and upload `vuoom-windows-x64` (`target/release/vuoom.exe` built via `pnpm tauri build --no-bundle --config "$env:RUNNER_TEMP\no_updater.json"`), and verify every UI chunk locally by downloading the compiled CI artifact and inspecting live via WebView2 CDP. Verified chunks merge cleanly to `main` (`--no-ff` with `[skip ci]` on intermediate chunks until Phase U6 bumps to `2.5.0`, preventing `release.yml`'s `check-and-bump` bot from auto-cutting `v2.4.1` mid-redesign).
- **Rationale**: Guarantees full rollback isolation from `v2.4.0` performance work, zero local compile strain, and authentic packaged-binary visual/telemetry verification on every milestone.

### D-015 — Design Direction Choice: Direction A — Tungsten Darkroom & Daylight Archival Studio (Co-Designed with GPT-6.2 Astra)
- **Date**: 2026-11-27
- **Context**: `UI_AUDIT.md` exposed inverted dark/light contrast overrides (`tokens.css:220-230`), light-mode startup splash flash (`index.html`), missing `"System"` OS sync, vertical clipping in `ToolRail`, sparse layouts in `Home` and `Inspector`, and misalignment/wrapping issues in `ExportDialog` and `Settings`. Evaluated three directions with Codex **GPT-6.2 Astra**:
  - **Direction A — Tungsten Darkroom & Daylight Archival Studio**: Smoked obsidian & warm archival paper chrome matching Vuoom's redesigned website, 1px specular `--rim` on raised groups/islands, warm tungsten focus/precision highlights, neutral matte video Stage (`#181818` / `#D8D8D8`) so footage color stays perceptually neutral, and `"System"` auto-resolution by default.
  - **Direction B — Precision Mono Pro**: Achromatic cool-gray high density evolved directly from `mono-dark`/`mono-light`. Strong studio density, but lacks brand continuity and leaves Home feeling sterile.
  - **Direction C — Soft Native Calm**: Airy whitespace and `10–14px` radii inspired by Cap.so / Screen Studio / Things 3. Welcoming on Home, but wastes scarce viewport area at `880×560`.
- **Decision**: **Adopt Direction A (Tungsten Darkroom & Daylight Archival Studio)** as Vuoom's flagship `"system"`-resolved desktop appearance, while preserving calibrated `"mono-dark"`, `"mono-light"`, `"graphite"`, and `"midnight"` fixed appearances (and keeping legacy `"paper"` seamlessly mapped to `"daylight"`). Reserve Record Crimson (`#C62F38` fill / `#FF9497` & `#9D2733` text) exclusively for capture actions, ink for ordinary selection, and distinct timeline track hues for editing layers. Zero purple anywhere.
- **Rationale**: Unifies desktop and web product identity, avoids warm color cast on the video Stage via neutral matte framing, and pairs Direction C's calm onboarding ergonomics with Direction B's dense keyboard-first editor precision.

### D-016 — WCAG 2.2 AA Token Split, Zero-Flicker System Theme & Inter Latin Subsetting
- **Date**: 2026-11-27
- **Context**: Live `v2.4.0` contrast checks proved white text on `#E5484D` (`3.91:1` rest, `3.19:1` hover) and `--ok`/`--warn` on light themes (`1.48–2.59:1`) fail WCAG 2.2 AA. Meanwhile `@fontsource-variable/inter` bundles 7 language subsets (`218.5 KB`) while `constants.ts` requests `"Inter"` instead of `"Inter Variable"`.
- **Decision**:
  1. Separate crimson button fill (`--record: #C62F38`, hover `#D13740`, pressed `#AF2630`, `--on-record: #FFFFFF` -> `5.43:1` / `4.85:1` / `6.68:1` AA) from crimson text (`--record-text: #FF9497` dark / `#9D2733` light), and ensure all 4 text tiers (`--text`..`--faint`) pass `>= 4.5:1` (`4.81:1` minimum across all 6 themes) and all 3 line tiers pass `>= 3.0:1`.
  2. Split chosen theme preference (`system` default in `localStorage['vuoom-theme']`) from resolved palette, resolving synchronously in `index.html` `<head>` before splash paint and syncing via a single cleaned-up `matchMedia('(prefers-color-scheme: dark)')` listener.
  3. Subsetting `@fontsource-variable/inter` to Latin only (with Chinese system fallbacks `PingFang SC` / `Microsoft YaHei UI`), while resolving `"Inter Variable"` properly in canvas/export font lookup without altering existing document line wrapping or glyph metrics.
- **Rationale**: Achieves strict WCAG 2.2 AA compliance across all themes, eliminates light-mode startup flashes, and trims unused font payload from `dist/`.

### D-017 — Structural Layout Architecture: Full-Height ToolRail, 2-Column Home & Opaque Islands
- **Date**: 2026-11-27
- **Context**: In `v2.4.0`, `ToolRail` sits inside `.upper` beside `Stage`, causing bottom tools (`Spotlight`, `Mask`, collapse chevron) to clip off-screen whenever `Timeline` opens (`1280×800` and `880×560`). Home leaves empty vertical voids at `1280×800` yet scrolls at `880×560`. Nine `backdrop-filter: blur()` rules and `100vmax` drag spread shadows tax GPU compositing over live video canvases.
- **Decision**:
  1. Promote `ToolRail` to a full-height left column in a 3-column workspace grid (`ToolRail | center(Stage + Timeline) | Inspector`) so all 11 tools + collapse control fit in `416px` inside `488px` at minimum `880×560` height without scrolling or clipping.
  2. Redesign Home into a balanced top-aligned 2-column workspace (`384px` / `312px` capture column + recent-projects / contact-sheet column) that fits at `1280×800` and `880×560` with zero page scroll when empty.
  3. Replace all `backdrop-filter` blurs over live canvases/modals and `100vmax` spread box-shadows with opaque elevated `.island` surfaces (`inset 0 1px 0 var(--rim)`) and 4 bounded dim panes for `CropEditor` and `RecordOverlay`.
- **Rationale**: Eliminates layout clipping permanently, improves viewport utility without adding scrollbars, and reduces GPU/WebView2 compositing overhead.

### D-018 — Motion Discipline & Commissioned Custom SVG Assets (`GPT-6.2 Astra`)
- **Date**: 2026-11-27
- **Context**: `.insp-empty-icon` ran an infinite `3.2s float` animation and `.meter-fill` animated `width` every `70ms`, while hardcoded durations bypassed `prefers-reduced-motion`. Empty states used generic fallback glyphs.
- **Decision**: Use pure CSS transitions/keyframes + Web Animations API with zero external animation packages (`0 B` bundle tax), enforce compositor-only `transform`/`opacity` animations with zero infinite idle loops and full `prefers-reduced-motion` / `data-motion="reduced"` compliance (`MOTION.md`), and commission 4 theme-reactive custom SVG illustrations (`src/components/StudioArt.tsx`, combined ceiling `<= 9 KB`) from **Codex GPT-6.2 Astra**.
- **Rationale**: Preserves 60fps under 6x CPU throttling and sub-0.5% idle CPU while delivering cohesive studio visual polish.
