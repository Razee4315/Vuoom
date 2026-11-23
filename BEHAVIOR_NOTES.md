# Behavior Preservation Notes (Track 1)

Records exact current behavior before each Track 1 optimization edit to ensure zero regressions or unintended visible changes.

---

## Wave 1 Notes

### W1-A: Rust Build & Link Config (`E1` + `E3`)
- `src-tauri/Cargo.toml`: defines `vuoom_lib` (`src-tauri/src/lib.rs`) invoked by `src-tauri/src/main.rs`. Desktop build only needs `rlib`; `staticlib` and `cdylib` are mobile-only artifacts.
- `target/release/vuoom.exe` must keep its embedded icon/manifest resources (`tauri-winres`), launch cleanly on Windows, open maximized main window, pass all 290 Rust tests, and pass `clippy -D warnings`.

### W1-B: Permissions Card (`F4 / A6` — `src/components/Permissions.tsx`)
- **Current Windows behavior**: Renders nothing (`!isMac`), yet mounts `PermissionsCard`, immediately invokes `permissions`, attaches a `focus` listener calling `refetch()`, and schedules a `window.setInterval(..., 4000)` indefinitely while Home is mounted.
- **Preserved visible behavior**: On Windows, still renders nothing (`null`). On macOS (`isMac`), still fetches `permissions` on mount, refreshes on window `focus`, shows missing Screen Recording / Accessibility cards with working `Allow` buttons, stops polling automatically once all permissions are granted (`missing().length === 0`), and pauses background timer polling while `document.hidden`.

### W1-C: Rust Dependency Cleanup, Deduplication & Security (`E5–E9`)
- **Current behavior**:
  - `vuoom-encode`: uses `gif::Encoder`, `gif::Frame`, `gif::DisposalMethod::Keep`, `gif::Repeat::Infinite`, plus `color_quant = "2.0"` (`NeuQuant`). Dropping `gif` default features (`color_quant 1.1.0`) keeps identical GIF encoding API and output.
  - `deny.toml`: currently fails `cargo-deny check` due to `jpeg-encoder`'s `(MIT OR Apache-2.0) AND IJG` license on macOS and un-ignored `zip 4.6.1` advisories / un-updated `url 2.5.8`.
- **Preserved behavior**: Zero runtime API or output changes; all 290 unit/integration tests in `cargo test --workspace` pass; macOS-only `objc2-foundation` retained cleanly.

### W1-D: Tauri Mock Bundle Exclusion & Safe Dep Updates (`F10-T1`)
- **Current behavior**: `src/index.tsx:39` loads `./mock/install` when `isMock` is true; Vite statically cannot prove `isMock` false for Tauri builds and emits `dist/assets/install-*.js` (`41,388 B` raw / `13,966 B` gz) into packaged builds.
- **Preserved behavior**:
  - Standalone `pnpm dev`, `pnpm build`, and `pnpm preview` (`?mock=1`) have `__TAURI_BUILD__ === false` and continue loading `./mock/install` identically.
  - Packaged `tauri build` sets `TAURI_ENV_PLATFORM` (`__TAURI_BUILD__ === true`), dead-code-eliminating the mock chunk from the shipped installer while Tauri runtime uses native `invoke`.
