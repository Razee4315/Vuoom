# 12 — CI/CD & Working Rules

How Vuoom is built, tested, and released — and two standing workflow rules.

---

## Standing rules

1. **All builds and tests run on GitHub Actions, not locally.** We do not run heavy
   `cargo build` / `pnpm tauri build` locally. Push, and let CI compile, lint, test, and (on
   demand) build installers. Local work is editing + fast unit tests at most.
2. **Commit and push after every change.** Every meaningful edit is committed and pushed so we
   always have clean, roll-back-able history.

These adapt the project's existing Tauri v2 CI/CD pipeline guide
(`TAURI_CICD_PIPELINE_GUIDE.md`) to Vuoom's pnpm + Cargo-workspace layout.

## Pipelines

### `.github/workflows/ci.yml` — Continuous Integration
- **Triggers:** push to `main`, PRs to `main`.
- **Runner:** `windows-latest` (Vuoom is Windows-only — WGC capture, wgpu DX12).
- **Steps:** install Rust (rustfmt + clippy) → Rust cache → Node 22 + pnpm 10 →
  `pnpm install --frozen-lockfile` → `pnpm typecheck` (tsc) → `pnpm build` (vite → `dist/`) →
  `cargo fmt --all --check` → `cargo clippy --workspace --all-targets -D warnings` →
  `cargo test --workspace` → `cargo check --workspace --all-targets`.
- **Why build the frontend before cargo:** Tauri's `generate_context!` needs `dist/` to exist
  before the app crate will compile/check.

### `.github/workflows/release.yml` — Release
- **Trigger:** **manual (`workflow_dispatch`) for now.** Vuoom is pre-implementation; we don't
  want to publish the empty scaffold on every push. **Flip to auto-release** by uncommenting the
  `push: branches: [main]` trigger once there's something worth shipping (target: **M4**).
- **Job 1 (`ubuntu-latest`):** read version from `tauri.conf.json` (source of truth); if a tag
  for it exists and there are new commits, auto-bump the patch and sync all three version
  locations, commit `[skip ci]`, push.
- **Job 2 (`windows-latest`):** `tauri-apps/tauri-action@v0` builds the frontend + Rust release
  binary, bundles `.msi` + NSIS `.exe`, tags `v{version}`, and publishes a GitHub Release.

## Version sync (workspace-specific)

Three locations must match. **Note the workspace difference from the generic guide:** the Rust
version lives in the **root `Cargo.toml`** `[workspace.package]`, and `src-tauri/Cargo.toml`
inherits it via `version.workspace = true`.

```
src-tauri/tauri.conf.json   "version": "x.y.z"   ← SOURCE OF TRUTH
package.json                "version": "x.y.z"
Cargo.toml (root)           [workspace.package] version = "x.y.z"
```

The release job's bump edits exactly these three (the root `Cargo.toml`, not `src-tauri/`).

## One-time GitHub setup

- **Settings → Actions → General → Workflow permissions → "Read and write permissions"** (lets
  the release job push the version-bump commit and create releases). `GITHUB_TOKEN` is automatic;
  no secrets needed for the current pipeline.
- After a release bumps the version and pushes, `git pull` before your next push.

## TODO before the first real release (M4)

- [ ] Bundle the **gifski** sidecar binary (out-of-process; see [`06`](./06-Export.md),
      [`10`](./10-Licensing.md)) — GIF export depends on it.
- [ ] Consider adding `cargo-deny` to CI to enforce the license policy (no AGPL/GPL **linked**).
- [ ] Code signing (Azure Trusted Signing) wired into the release build (see [`02`](./02-Architecture.md)).
- [ ] Flip `release.yml` to the `push: [main]` trigger.
