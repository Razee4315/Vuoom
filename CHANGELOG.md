# Changelog

All notable changes to Vuoom. Newest first. Full notes for every version are on
[GitHub Releases](https://github.com/Razee4315/Vuoom/releases) and on the
[website](https://razee4315.github.io/Vuoom/changelog/).

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions
follow [Semantic Versioning](https://semver.org/). Vuoom updates itself from inside the app.

## [Unreleased]

## [2.3.0] - 2026-11-27

### Added

- **Animated WebP export ([#100](https://github.com/Razee4315/Vuoom/issues/100)):** New WebP export format powered by vendored Google `libwebp` (`crates/vuoom-encode/src/webp.rs`) supporting lossy quality control, pixel-exact Lossless mode, up to 60 fps, debounced size estimation, `Fit under __ MB` target sizing, and format preference in Settings (`v2.3.0` keeps Apache-2.0 compliance).
- **Mini Bar mode & Countdown Start Now:** Turning **Live preview** off collapses the floating recording panel into a compact `420×68` Mini Bar with time, compact audio meters, Pause/Resume, and Stop (disabling live preview frame encoding and WebSocket streaming cleanly); countdown adds an SVG progress ring and a **Start now** control (also bound to `Enter`).
- **Paused frosted veil:** Pausing mid-take overlays a clear status veil with a one-click **Resume** pill inside the monitor stage.

### Fixed

- **Recording startup overlap & inner button collision:** Kept the selector overlay mounted through `preparing` until `enter_stopbar` finishes shrinking and parking the host window so the panel no longer flashes full-screen over the desktop at take start; anchored `.rec-timebox` and `.rec-btn-group` side-by-side inside dedicated header (`36px`) and bottom deck (`54px`) chrome so timer readouts, keyboard hints, Pause/Resume, and Stop never overlap.
- **Work-area & window-aware panel parking:** `enter_stopbar` and `set_panel_size` now park inside `mon.work_area()` (clearing taskbars/Docks) across 8 candidate perimeter positions (`pick_panel_spot`) and avoid recorded regions and captured window bounds (`BorderState::avoid_rect`).
- **Timer & meter background hygiene:** Recording elapsed ticker and `AudioControls` level polling clean up properly and pause when `document.hidden` becomes true.

## [2.2.0] - 2026-11-23

### Performance & Efficiency
- Cut Rust host idle CPU by **58.8%** when focused (`0.456% -> 0.188%` single-core) and to **0.00%** when minimized (`0.094% -> 0.000%`), reducing total app + WebView2 idle CPU by **19.6%** focused and **22.9%** minimized (`Permissions.tsx` now skips macOS permission IPC/focus/4s polling on Windows and pauses/stops macOS polling when granted or hidden).
- Stripped the browser mock backend (`install-*.js`, `41.4 KB` raw / `14.0 KB` gzip) from packaged desktop builds via build-time `!__TAURI_BUILD__` tree-shaking and disabled `modulePreload` polyfill (`dist/` `-41.6 KB` raw / `-14.0 KB` gzip, Vite internal build `-31.9%` faster).

### Security & Dependencies
- Lockstep upgrade to Tauri `2.12.7` (`tauri-plugin-updater 2.14.1`, `@tauri-apps/api 2.12.5`, `@tauri-apps/cli 2.12.6`, and updated plugins), pulling `zip 9.8.2` to eliminate all 6 `zip 4.x` advisories (`RUSTSEC-2026-0367/0368/0369/0429/0545/0548`).
- Patched Rust transitive dependencies (`url 2.5.9`, `cookie 0.18.3`, `clipboard-win 5.4.3`, `swash 0.2.10`, `event-listener 5.4.2`, `memmap2 0.9.12`, `rustix 1.1.6`), cutting `cargo-audit` vulnerabilities from `9 -> 2` (and warnings `23 -> 13`), deduplicating `color_quant 1.1.0`, trimming unused crate dependencies (`cargo-machete` `0`), and making `cargo-deny check` green (`advisories`, `bans`, `licenses`, `sources`).
- Updated frontend tooling to Vite `8.3.8`, `vite-plugin-solid 2.11.17`, Biome `2.6.8`, TypeScript `5.9.3` (`ES2023`), and `@fontsource-variable/inter 5.3.0`, cutting `pnpm audit` advisories from `19 -> 9` (`0` advisories shipped in `dist/`).

### Build & Tooling
- Dropped unused mobile `staticlib`/`cdylib` crate types, enabled `rust-lld` linker on Windows x64 MSVC, and tuned dev debuginfo to `line-tables-only` (`debug = false` on dependencies): **-31.1%** `cargo clippy`, **-32.5%** `cargo test --workspace`, and **-15.4%** isolated incremental release build.
- Updated workspace MSRV to `1.90` and resolved Rust `1.100` Clippy/Cargo lints (`session.rs` duplicate `#[allow]`, macOS-gated `vuoom-input` `tracing`).

### Platform & Reliability (since v2.1.0)
- Native macOS support on Apple Silicon (macOS 14+) (#102).
- Explicit state machines for the record, update, and export flows (#101).

## [2.1.0] - 2026-10-31

### Added
- Try a sample clip, on Home, in the File menu and on the welcome card: the whole editor to play
  with before recording anything.
- A big area at 60 fps offers 30 fps before the take, on a PC that may struggle with it.
- Simplified Chinese: the whole app, the installer, and captions that split properly (#94).
  Vuoom picks it up from Windows, and Settings has a language switch.
- Add a take: File > Add a take to this clip records onto the end of the clip, sharing one
  timeline with its pointer, zooms and sound, so a video can take more than one go (#30).
- Numbered steps: the Step tool (`N`) places 1, 2, 3 in discs for walkthroughs.
- Curved arrows, with weights up to 5%. Hold `Shift` while drawing for 45 degree steps, and
  squares with the Shape tool, which can now draw ellipses too. A new arrow takes the color
  and weight of the last one.
- Hide can pixelate instead of blacking out.
- Slow motion: a speed region goes down to 0.25x.
- An export says how long it has left. Opening a project shows that it is working.
- The Vuoom website at <https://razee4315.github.io/Vuoom/>: download, features, comparisons,
  guide, shortcuts, FAQ, changelog, privacy, terms, security, licences and a press kit.
- `CHANGELOG.md`, `PRIVACY.md` and `SUPPORT.md`.
- Settings > About > Copy report: your Vuoom and Windows versions, GPU, displays and the
  end of the log, ready to paste into a bug report (your user folder is hidden, nothing is
  sent). Open logs folder sits next to it. If Vuoom didn't close normally last time, it
  offers the report on the next launch.
- Auto zoom: a take recorded without pressing Ctrl+Shift+Z now gets a few calm zooms where
  you clicked (at most four a minute, none longer than 8 seconds), with a Remove button on
  the notice. Press the hotkey even once and only your own zooms are used. Switch it off in
  Home > Options, the record panel's Options menu or Settings.
- The smooth pointer changes shape like the real one: a text beam over text, a hand over a
  link, resize and move arrows, a crosshair.
- Your own pointer: Clip > Pointer > Look can use any picture as the pointer (click its
  preview to set where it points) or a simple dot, with an opacity slider and a shadow
  switch. New takes start with the look you chose last.
- Click effects: four animations (Ripple, Rings, Pulse, Burst) in any color, with size,
  length and opacity sliders, under Clip > Overlays.
- The zoom and stop hotkeys can be changed (Settings > Shortcuts), and the app you are
  recording no longer receives them: Ctrl+Shift+Z used to also trigger Redo in it.
- The region picker offers the area you recorded last time, and Record remembers whether you
  last recorded a region, the full screen or a window.
- MP4 exports keep the take's own size, up to 4K, and its frame rate. "Balanced" is 1080p at
  30 fps and "High quality" is the full size at up to 60 fps (they were 1000 and 1280 px wide).
- GIF gradients (backdrops, shadows, the webcam) are dithered so they don't show bands, with
  a switch in the export card. Flat interface colors are left as they are.

### Changed
- 4K is much lighter: unchanged parts of the screen cost nothing to store or read back, and
  an export reads, draws and encodes side by side. Measured on a 4-core machine with no GPU,
  frames store at 96 fps with a video playing on a 4K screen, and 300 to 580 fps while typing.
  A playing preview uses a lighter frame and sharpens when it stops.
- MP4 exports get the bitrate 4K at 60 fps calls for, where they used to stop at 50 Mbps.
- Text labels fade in and out in exports as they do in the editor.
- A frame (Subtle, Studio) is now added around the recording instead of squeezing the
  recording into the same size: the picture keeps its shape and every pixel, so text stays as
  sharp as it was recorded. Annotations on a project saved with a frame may sit a little to
  the side of where they were.
- MP4: frames reach the encoder in its own format with a fixed BT.709 conversion that is
  written into the file, so colors look the same in every player. The encoder uses the High
  profile and variable bitrate, with a keyframe every two seconds.
- Exports and the editor preview are scaled on the GPU, averaging every source pixel, rather
  than on the processor afterwards: faster, and small text no longer breaks up when a take is
  exported smaller than it was recorded.
- Recording is lighter: only the rows of the screen that changed are compressed, frame buffers
  are reused, and the editor preview is drawn at the size it is shown.

### Fixed
- Every take lost its opening frames while the audio devices opened. Frames are now taken from
  the first one, at any resolution.
- A hide mask stays on what it covers through a zoom and never fades. Private details could
  slide out from under one during a zoom, or show through while it faded in.
- A long 4K export can't run the computer out of memory: frames used to pile up whenever the
  encoder fell behind.
- A framed 4K take exports: its picture is wider than H.264 allows, which used to fail.
- Takes of a mostly still screen export at the frame rate they were recorded at, instead of a
  rate worked out from how little moved, which made zooms stutter.
- Window takes: the pointer, clicks and zooms were a title bar's height off.
- A region dragged off the edge of the display keeps its shape, and the pointer stays on target.
- HDR displays record through Windows Graphics Capture, which converts the picture.
- Clicks outside the recorded area no longer zoom or show a ripple.
- The smooth pointer is sized to your screen, so it doesn't shrink on a small region.
- Small text is readable in every theme, and status colors hold up on the light themes.
- The frame's shadow is drawn (the slider and the presets set it, nothing rendered it).
- A frame no longer stretches the recording sideways (about 4% on Subtle, 8% on Studio).
- Recording a window on Windows 10 now holds the frame rate you chose instead of capturing
  every screen refresh.
- The outermost row of pixels of an export is no longer mixed with the backdrop.
- Saving a project is now all-or-nothing: the new save is written beside the folder and
  swapped in only once complete, so a full disk or a crash mid-save can no longer break an
  earlier save. Vuoom checks for enough free space before writing anything (#30).
- Opening a project that can't be opened keeps the clip you had, and says why in plain words
  (folder gone, save cut short, damaged file). A project cut short opens with the frames that
  survive instead of failing outright.
- `SECURITY.md` and `NOTICE` now describe the app as it is: the two network requests it makes
  (the update check and the one-time captions model) and the built-in GIF encoder.

## [2.0.0] - 2026-09-24

A new studio: sound, a webcam, captions made on your PC, and tools to mark up your video.

### Added
- Microphone and system sound, kept in sync with the picture, with voice clean-up (noise
  removal and even volume).
- A webcam bubble in any corner: shape, size and mirroring are editable after recording.
- Offline captions made by whisper.cpp on your PC; fix, retime and restyle them, and save `.srt`.
- A smooth, redrawn pointer that can hide when still, and motion blur on zooms and pans.
- Pen and Marker (`P`), Spotlight (`L`), Zoom and Crop tools, alongside arrows, shapes,
  highlights and Hide for private details.
- Frame and backdrop controls: padding, corners, shadow, custom colours, four new backdrops
  and your own picture as the backdrop.
- Search in the Clip panel (`Ctrl+F`).

### Changed
- A calmer start screen with sound, camera and options on one row.
- A redesigned studio: tool rail grouped by task, a timeline with a frame strip, and panels you
  can show, hide and resize.
- Faster full-screen recording, a real-time live preview, and zooms that appear the moment you
  press `Ctrl+Shift+Z`.
- The recording panel never shows up in the video, even over the recorded area.
- Export settings are remembered and projects save instantly.

## Earlier versions

Versions 0.1.x are listed on [GitHub Releases](https://github.com/Razee4315/Vuoom/releases).

[Unreleased]: https://github.com/Razee4315/Vuoom/compare/v2.2.0...HEAD
[2.2.0]: https://github.com/Razee4315/Vuoom/compare/v2.1.0...v2.2.0
[2.1.0]: https://github.com/Razee4315/Vuoom/compare/v2.0.0...v2.1.0
[2.0.0]: https://github.com/Razee4315/Vuoom/releases/tag/v2.0.0
