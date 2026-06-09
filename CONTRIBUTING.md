# Contributing to Vuoom

Thanks for wanting to make Vuoom better! This is a small, focused project — the
goal is the **easiest way for developers to make a demo GIF on Windows**, and every
contribution should serve that.

## Ground rules

- **Hold the scope line.** v1 is GIF-only, no audio, no webcam, no cloud. The
  out-of-scope list in [`docs/Vuoom-Spec.md`](./docs/Vuoom-Spec.md) §7 is deliberate;
  PRs that expand it will be declined kindly.
- **The webview is the cockpit, Rust is the engine.** Capture, compositing, and
  encoding stay native. Don't move heavy work into the frontend.
- **Quality gates are CI.** Every PR must pass `tsc --noEmit`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.

## Getting started

```sh
git clone https://github.com/Razee4315/Vuoom
cd Vuoom
pnpm install
pnpm tauri dev
```

You'll need: **Windows 10/11**, [Rust stable](https://rustup.rs),
[Node 22+](https://nodejs.org), and [pnpm](https://pnpm.io). The GPU compositor
(wgpu/DX12) and screen capture need a real display — runtime testing happens on
your machine, while CI verifies compilation, lints, and the pure-logic test suite.

## Project layout

| Path | What lives there |
|---|---|
| `src/` | SolidJS editor UI |
| `src-tauri/` | Tauri shell: session orchestration, commands, tray, hotkeys |
| `crates/vuoom-zoom` | Auto-zoom planner + spring camera (pure logic, heavily tested) |
| `crates/vuoom-render` | wgpu compositor (zoom crop, text, shapes) |
| `crates/vuoom-capture` / `vuoom-input` | Windows Graphics Capture + global input log |
| `crates/vuoom-encode` | GIF encoding, frame planning, size estimation |
| `crates/vuoom-project` | The `.vuoom` project model (single source of truth for edits) |
| `docs/` | Design docs — read these before touching a subsystem |

## Making a change

1. Open an issue first for anything non-trivial — saves everyone time.
2. Branch from `main`, keep commits small and descriptive
   (`fix(zoom): …`, `feat(editor): …`, `docs: …`).
3. Pure logic (planner, camera, timing, encoding math) needs unit tests —
   that's what makes this codebase safe to change.
4. Open a PR; CI must be green. Note anything that needs a manual runtime check
   (capture, GPU, input) so it can be verified on a real machine.

## Reporting bugs

Use the bug report template. The most useful bug reports include: your Windows
version, the Vuoom version (title bar / installer name), what you recorded
(full screen vs region, zoom used or not), and what you expected vs got.

## License

By contributing you agree your work is licensed under [Apache-2.0](./LICENSE).
