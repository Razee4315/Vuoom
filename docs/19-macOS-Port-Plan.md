# 19, macOS Port Plan

How to make Vuoom run natively on macOS (Apple Silicon first) while keeping the Windows
build exactly as it is. Written after reading the whole codebase at `v2.1.1` (commit
`3c191ff`), then implemented on the `feat/macos-support` branch. Section 0 says what is
done and how it was verified.

## 0. Status (2026-11-11)

Every phase is implemented. Decisions taken from section 9's recommendations: macOS 14
minimum, Apple Silicon first, `Cmd+Shift+R/Z/X` hotkeys, ad-hoc signing until there is a
Developer ID.

| Area | Where | Verified on the M1 Pro |
|---|---|---|
| Builds on macOS (no Windows-only imports left unguarded) | whole workspace | `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt --check`, `cargo test --workspace` (291 tests) |
| Screen capture (ScreenCaptureKit, Retina pixels, crop, Vuoom's windows left out) | `crates/vuoom-capture/src/sck.rs` | `cargo run -p vuoom-capture --example grab`: 3024×1964 full display, 640×360 crop, timestamps on the input clock |
| Host clock, points → pixels, key code map | `crates/vuoom-input/src/mac/` | unit tests (mixed 2x/1x displays, key table) |
| Input log + hotkeys (Quartz event tap, Command chords, pointer shapes) | `crates/vuoom-input/src/mac/recorder.rs`, `chords.rs` | compiles and unit tested; live clicks/keys need the app run by hand |
| Engine end to end (Metal compositor, sample → GIF + MP4, real recording → MP4) | `src-tauri/src/macos/e2e.rs` | `cargo test -p vuoom --lib macos_e2e -- --ignored`: release build records 85 frames in 3 s at 30 fps, 0 dropped |
| MP4 (AVAssetWriter, VideoToolbox H.264 High, BT.709, AAC) | `src-tauri/src/macos/mp4.rs` | `mac_encodes_an_mp4_with_sound` + ffprobe: h264 High, bt709 tv range, AAC 48 kHz stereo, durations exact |
| Microphone (Core Audio via cpal) and system sound (ScreenCaptureKit audio) | `crates/vuoom-audio/src/mac.rs` | `cargo run -p vuoom-audio --example listen`: both record with real levels |
| Webcam (AVFoundation) + pure Rust JPEG | `crates/vuoom-camera/src/mac.rs`, `jpeg.rs` | JPEG round trip tested; camera opening stops at the Camera permission when run from a terminal (the app asks for its own) |
| Captions (whisper.cpp on Metal) | `crates/vuoom-captions` | `--example caption` on `say` speech: every word right, timed |
| Record flow windows (cover the display without a fullscreen Space, floating panel, region border) | `src-tauri/src/macos/window.rs`, `commands.rs` | compiles; **needs a hand test** (see below) |
| Permissions (Screen Recording, Accessibility) card + clear errors | `components/Permissions.tsx`, `commands::permissions` | seen in the release build |
| Mac UI: traffic lights, ⌘ shortcuts and key caps, Mac wording, zh-CN | `src/platform.ts` and callers | screenshots of dev and release builds; `tsc`, Biome, i18n check |
| Copy exported file to the clipboard | `windows_ext.rs` (NSPasteboard) | compiles; hand test |
| Packaging: `.app`, `.dmg`, updater archive, Info.plist, entitlements, ad-hoc signing | `tauri.macos.conf.json`, `Info.plist`, `Entitlements.plist` | `pnpm tauri build --bundles app dmg`; `codesign --verify` passes; app launches |
| CI and release on macOS | `.github/workflows/ci.yml`, `release.yml` | YAML valid; runs on the next push |

**Windows safety:** the Windows code paths were only moved, never rewritten. The engine
crates (`vuoom-capture`, `vuoom-input`, `vuoom-audio`, `vuoom-camera`) type-check for
`x86_64-pc-windows-msvc` from this Mac; the app shell can only be compiled on Windows (its
C dependencies), so the Windows CI job is the check for that.

### Hand test checklist (the parts a script can't click through)

1. Region, Full screen and Window takes from the Home screen: the selector covers the whole
   display (menu bar included) with no Space animation; the panel floats while recording;
   the red border frames the region; nothing of Vuoom shows in the take.
2. ⇧⌘Z zooms live and doesn't reach the app being recorded (needs Accessibility); ⇧⌘X stops
   from any app.
3. Microphone and system sound in a take, webcam bubble, captions.
4. Export GIF and MP4, then Copy and paste into Slack or Finder.
5. ⌘Z / ⇧⌘Z / ⌘S / ⌘E / ⌘K in the editor (macOS's own Edit menu must not swallow them).
6. On an external monitor too.


> Rule for every phase: **Windows must keep building and behaving the same.** All macOS code
> goes behind `#[cfg(target_os = "macos")]`, and the Windows CI job stays green on every
> step. We work on a branch (`feat/macos-support`), never on `main`.

---

## 1. What the codebase is

Vuoom is a Tauri 2 screen recorder with cinematic auto-zoom (a free Screen Studio style
tool). About 29k lines of Rust plus a SolidJS + Vite UI.

```
src/            SolidJS editor UI. Talks to Rust via invoke() and a localhost WebSocket
                that streams preview frames (drawn on a 2D canvas).
src-tauri/      The app shell: session state machine, commands, record flow windows,
                frame store (LZ4 XOR deltas on disk), MP4 export, bundles, recovery.
crates/
  vuoom-zoom      auto-zoom planner + spring camera           pure Rust, portable
  vuoom-project   .vuoom project model, edits, timeline        pure Rust, portable
  vuoom-render    wgpu compositor (text, shapes, blur, cursor) portable (wgpu picks Metal)
  vuoom-encode    GIF encoder + size estimation                pure Rust, portable
  vuoom-preview   localhost WebSocket preview server           portable (tokio)
  vuoom-captions  whisper.cpp captions + SRT                   portable (CMake build)
  vuoom-capture   screen capture: WGC + DXGI Duplication       WINDOWS ONLY
  vuoom-input     global input hooks, QPC clock, hotkeys       WINDOWS ONLY (core is pure)
  vuoom-audio     WASAPI mic + loopback, mixer, RNNoise        capture WINDOWS ONLY, rest pure
  vuoom-camera    Media Foundation webcam, WIC JPEG            capture WINDOWS ONLY, rest pure
```

### The good news

The heavy, valuable part is already portable: the zoom planner, the project model, the
wgpu compositor, the GIF encoder, the preview server, the frame store codec, the mixer and
voice cleanup. `wgpu::Instance::new(&InstanceDescriptor::default())` will pick **Metal** on
a Mac with no code change. The UI's preview is a plain 2D canvas fed over WebSocket, which
WKWebView handles fine.

`vuoom-audio` and `vuoom-camera` already have a clean seam: a `mod imp` for Windows and a
`#[cfg(not(windows))] mod imp` stub with the same functions. A macOS backend is "fill in a
third `imp`".

### The bad news

The **app shell does not compile on macOS today.** The crates mostly gate their Windows
code, but `src-tauri` imports `windows::...` unconditionally in several modules, and the
`windows` crate is only a dependency on Windows targets:

| File | Unguarded Windows use |
|---|---|
| `src-tauri/src/displays.rs` | `EnumDisplayMonitors`, `GetMonitorInfoW` (whole module) |
| `src-tauri/src/windows_list.rs` | `EnumWindows`, DWM frame bounds, `ClientToScreen` (whole module) |
| `src-tauri/src/live_preview.rs` | `GetCursorPos` |
| `src-tauri/src/zoom_chord.rs` | `GetCursorPos`, `Chord::is_down` (Windows only) |
| `src-tauri/src/hotkey.rs` | `vuoom_input::ChordWatch` (Windows only) |
| `src-tauri/src/session.rs` | `vuoom_capture::*` and `InputRecorder` (Windows only exports), `screenshot_window` (`PrintWindow`) |
| `src-tauri/src/commands.rs` | `vuoom_capture::CropRegion`, HWND based window targets |

The non-Windows stubs that do exist say "keeps `cargo check` portable", but nobody has
ever run that check, so it has rotted. (I could not run `cargo check` here: the sandbox
blocked building downloaded code. Phase 0 starts by running it.)

---

## 2. Windows to macOS mapping

Every Windows-specific piece, and what replaces it on macOS.

| Area | Windows today | macOS replacement | Min macOS |
|---|---|---|---|
| Screen capture (display) | DXGI Desktop Duplication, WGC fallback (`vuoom-capture`) | **ScreenCaptureKit** `SCStream` with `SCContentFilter(display:excludingWindows:)` | 12.3 |
| Screen capture (window) | WGC window capture by HWND | `SCContentFilter(desktopIndependentWindow:)` by `CGWindowID` | 12.3 |
| Keep Vuoom out of the recording | `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` + layered guard | Exclude our own windows in the `SCContentFilter`. We own the capture, so this is exact and needs no hacks | 12.3 |
| Win10 black box workaround | `drag_wall.rs` (`WM_MOVING` subclass) | **Not needed.** SCK composites the desktop behind excluded windows. Becomes a no-op | |
| Frame pacing | `MinUpdateInterval` + `FramePacer` | `SCStreamConfiguration.minimumFrameInterval` + keep `FramePacer` | 12.3 |
| Cursor in frames on/off | `CursorCaptureSettings` | `SCStreamConfiguration.showsCursor` | 12.3 |
| Dirty rows hint | DXGI move/dirty rects | `SCStreamFrameInfo.dirtyRects` attachment | 12.3 |
| Frozen backdrop screenshot (display) | first frame of a capture | same (first SCK frame) | |
| Frozen backdrop screenshot (window) | `PrintWindow` | `SCScreenshotManager.captureImage` (`CGWindowListCreateImage` is gone in macOS 15) | 14.0 |
| Master clock | `QueryPerformanceCounter` | `mach_absolute_time` scaled to ns (SCK and AVFoundation sample times are on the same host clock) | |
| DPI awareness | `SetProcessDpiAwarenessContext` | no-op; instead convert points to pixels per display (see section 4) | |
| Global input log | `WH_MOUSE_LL` / `WH_KEYBOARD_LL` hooks | `CGEventTap` on a CFRunLoop thread (active tap, so hotkeys can be swallowed) | |
| Key codes | Win32 VK codes everywhere | Translate macOS `kVK_*` codes to VK codes inside the recorder, so `keys.rs`, `chords.rs` and the keystroke overlay stay unchanged | |
| Hotkey key state polling | `GetAsyncKeyState` | `CGEventSourceKeyState(.combinedSessionState, keycode)` + `CGEventSourceFlagsState` | |
| Pointer shape (I-beam, hand...) | `GetCursorInfo` handle vs `LoadCursorW` | `NSCursor.currentSystemCursor`, matched against the standard cursors by hotspot + image hash. Fallback `Arrow` | |
| Cursor position | `GetCursorPos` | `CGEventGetLocation(CGEventCreate(nil))` (points) converted to pixels | |
| Display list | `EnumDisplayMonitors` | `SCShareableContent.displays` / `CGGetActiveDisplayList` + `CGDisplayBounds` + pixel size from the display mode | |
| Window list | `EnumWindows` + DWM cloaked filter | `SCShareableContent.windows` (on screen, layer 0, has a title, not ours) | 12.3 |
| Region border (4 strip windows) | Win32 layered popups | One borderless, click-through `NSPanel` (or 4) above everything, excluded from the SCK filter | |
| MP4 export | Media Foundation sink writer, NVENC/QSV/AMF | **AVAssetWriter** + VideoToolbox H.264 (hardware on Apple Silicon), AAC audio | |
| Microphone | WASAPI capture | `cpal` (Core Audio) or `AVCaptureSession` audio; stamp packets on the host clock | |
| System audio | WASAPI loopback | ScreenCaptureKit audio (`capturesAudio`, `excludesCurrentProcessAudio`) | 13.0 |
| Webcam | Media Foundation source reader | `AVCaptureSession` + `AVCaptureVideoDataOutput` (BGRA) | |
| Webcam JPEG | WIC | Pure Rust (`jpeg-encoder` + `zune-jpeg`), which also removes an OS dependency on Windows if we want | |
| Copy exported file to clipboard | `CF_HDROP` via `clipboard-win` | `NSPasteboard.writeObjects([NSURL fileURL])` | |
| Free disk space | `GetDiskFreeSpaceExW` | `statvfs` (libc) | |
| Background thread priority | `SetThreadPriority(BELOW_NORMAL)` | `pthread_set_qos_class_self_np(QOS_CLASS_UTILITY)` | |
| OS version in diagnostics | `cmd /C ver` | `sw_vers` / `NSProcessInfo.operatingSystemVersionString` | |
| Recovery folder | `%TEMP%\vuoom-recovery` | `~/Library/Caches/dev.vuoom.desktop/recovery` (`$TMPDIR` is purged by macOS after a few days, bad for crash recovery) | |
| Captions | whisper.cpp, AVX2 build | whisper.cpp arm64 build, with the `metal` feature of `whisper-rs` for GPU speed | |
| Installer / updater | NSIS `.exe`, `.msi`, `latest.json` | `.dmg` + `.app.tar.gz` updater bundle, `darwin-aarch64` entry in `latest.json` | |
| Code signing | unsigned (SmartScreen warning) | Developer ID signing + notarization (see section 7, this matters more on Mac) | |

### Bindings

Tauri 2.11 already pulls in **`objc2` 0.6**, so the plan is to use the matching `objc2-*`
framework crates (`objc2-screen-capture-kit`, `objc2-av-foundation`, `objc2-core-media`,
`objc2-core-video`, `objc2-app-kit`, `objc2-core-graphics`, `objc2-core-foundation`).
One consistent, safe-ish binding stack, no second Objective-C runtime layer. If the SCK
delegate plumbing gets painful, the higher level `screencapturekit` crate is the fallback
for the capture crate only. Pin versions when first pulled (same rule as the Windows deps).

---

## 3. Target architecture: a platform seam

Today the Windows code is scattered across `src-tauri`. Before writing Mac code, gather
every OS call behind one module so both backends fill the same functions.

```
src-tauri/src/platform/
  mod.rs          the shared signatures + types, re-exports the active backend
  windows.rs      moved, unchanged: displays, windows_list, windows_ext, drag_wall,
                  region_border, screenshot_window, GetCursorPos, disk/priority helpers
  macos.rs        the same functions for macOS
```

Shared types that stop leaking Win32 concepts:

- `WindowId(u64)` instead of `hwnd: isize`. On Windows it holds the HWND, on macOS the
  `CGWindowID`. The frontend already passes it as a number (`window_hwnd: Option<f64>`), so
  only the name changes. `CaptureSource::Window { hwnd }` becomes `{ id }`.
- `MonitorInfo.name` stays a string but becomes an opaque display key: `\\.\DISPLAY2` on
  Windows, the `CGDirectDisplayID` as text on macOS.
- `Backend::hides_excluded_windows()` returns `true` for the SCK backend, so the panel may
  sit over the recorded area on a Mac (it cannot on Windows 10).

The crates get the same treatment with the pattern `vuoom-audio` already uses:

```rust
#[cfg(windows)]            mod imp;   // existing code, untouched
#[cfg(target_os = "macos")] mod imp;   // new
#[cfg(not(any(windows, target_os = "macos")))] mod imp; // stub
```

Public APIs (`spawn_capture`, `CapturedFrame`, `InputRecorder::start`, `ChordWatch`,
`Recorder::start`, `CameraRecorder::open`, `Mp4Encoder`) keep their exact signatures, so
`session.rs` (5.4k lines) barely changes.

---

## 4. Coordinates, Retina and the clock (get these right once)

These are the bugs that would quietly ruin auto-zoom, so they get their own section.

**Pixels vs points.** Windows code keeps everything in physical pixels of one virtual
desktop. macOS reports mouse positions in **points** (global, origin top-left of the main
display, y down, which luckily matches Windows orientation) and SCK delivers **pixels**.
On this M1 Pro the built-in display is 2x.

- Define a Mac "virtual desktop pixel" as: for the display containing the point,
  `px = display.origin_points * scale + (pt - display.origin_points) * scale`.
  `MonitorInfo.x/y` holds `origin_points * scale`, `w/h` the pixel size. Then the existing
  normalization (`event - monitor origin`, divide by monitor size) works unchanged.
- One helper `points_to_pixels(x, y)` in `vuoom-input` (macOS) used by the event tap, the
  live preview cursor and the zoom chord poller. Unit test it with a 2x main display plus a
  1x external display placed left of it (negative origin).
- Mixed scale multi-monitor setups are the edge case. A take only ever records one
  display, so mapping relative to that display is enough.

**Retina sharpness.** SCK captures at point resolution unless told otherwise. Set
`configuration.width/height` to the display's pixel size (points times
`backingScaleFactor`), and on macOS 14+ `captureResolution = .best`. (Same lesson as the
Islefold port.) Crop with `sourceRect` in points, or crop the pixel buffer ourselves with
the existing `crop_bgra`.

**Row padding.** SCK pixel buffers are IOSurface backed with `bytesPerRow > width * 4`.
`CapturedFrame.bgra` must stay tightly packed, so copy row by row (we already do a copy,
so this costs nothing extra).

**The notch.** The 14"/16" MacBook Pro displays have a notch inside the menu bar area. A
full display capture includes it as black pixels. Offer "full screen below the menu bar"
as the default full screen region on notched displays (`NSScreen.safeAreaInsets`).

**Clock.** `Clock` on macOS returns `mach_absolute_time` converted to nanoseconds
(`mach_timebase_info`, 125/3 on Apple Silicon), `freq = 1_000_000_000`. SCK
`CMSampleBuffer` presentation times, AVFoundation camera and audio timestamps are all on
the host clock, so `CMTimeGetSeconds(pts) * 1e9` lands on the same axis. Stamp input events
on arrival, as the Windows design does. `audio::Anchor::seconds_to` takes "counter in 100 ns
units": on Mac, pass `ns / 100` and it works unchanged.

---

## 5. macOS permissions (new UX the Windows app never needed)

| Permission | Needed for | API to check / request | Notes |
|---|---|---|---|
| Screen Recording | all capture, window list titles | `CGPreflightScreenCaptureAccess` / `CGRequestScreenCaptureAccess` | Often needs an app relaunch after granting. macOS 15+ re-asks periodically for apps that bypass the system picker |
| Accessibility | active `CGEventTap` that swallows the zoom/stop hotkeys | `AXIsProcessTrustedWithOptions` | Without it: fall back to a listen-only tap (Input Monitoring) and the chord leaks to the recorded app |
| Input Monitoring | listen-only event tap (fallback) | `IOHIDCheckAccess` / `IOHIDRequestAccess` | |
| Microphone | narration | `AVCaptureDevice.requestAccess(.audio)` | `NSMicrophoneUsageDescription` |
| Camera | webcam bubble | `AVCaptureDevice.requestAccess(.video)` | `NSCameraUsageDescription` |

Plan: a `permissions_status` / `request_permission` command pair, and a permissions step
in `Welcome.tsx` plus inline "Grant access" buttons where a feature needs one (record
button, mic toggle, camera toggle). Deep link buttons open the right System Settings pane
(`x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture`).

Info.plist strings and hardened runtime entitlements go in `src-tauri/Info.plist` and
`src-tauri/Entitlements.plist`, referenced from `tauri.conf.json > bundle > macOS`.

---

## 6. Phases

Each phase ends with something runnable on this Mac and a green Windows CI. Estimates
assume AI-assisted pace and include testing on the M1 Pro.

### Phase 0: build on macOS, prove the portable half (0.5 to 1 day)

1. `cargo check --workspace --all-targets` on macOS, list every error.
2. Gate or stub every unguarded Windows import in `src-tauri` (table in section 1) so the
   app compiles and runs with capture/input/audio/camera returning "not on macOS yet".
3. `pnpm install`, `pnpm dev`, open with `?mock=1`: confirm the whole UI runs in a browser.
4. `pnpm tauri dev`: the real app boots, the wgpu compositor comes up on **Metal**, and
   **Load sample** works end to end: editor, scrubbing preview, annotations, GIF export.
5. Run `cargo test --workspace` on macOS. Fix any test that assumes Windows paths.

Done when: the app opens on the Mac, the sample take edits and exports a GIF.

### Phase 1: the platform seam (1 day)

1. Create `src-tauri/src/platform/` and move the Windows helpers in, unchanged.
2. `WindowId`, opaque display key, `Backend` variant for SCK (section 3).
3. macOS versions of the easy helpers: free space (`statvfs`), thread QoS, OS version,
   recovery dir under `~/Library/Caches`, clipboard file copy (`NSPasteboard`), display
   list (CoreGraphics), cursor position.
4. `drag_wall` becomes a no-op on macOS.

Done when: the Home screen lists the Mac's displays correctly and Copy puts a GIF on the
clipboard that pastes into Slack/Finder.

### Phase 2: screen capture with ScreenCaptureKit (3 to 4 days)

New `crates/vuoom-capture/src/sck.rs`:

1. `SCShareableContent` fetch, pick display by id or window by `CGWindowID`.
2. `SCContentFilter` that excludes every window of our own process.
3. `SCStreamConfiguration`: pixel size (Retina), BGRA (`kCVPixelFormatType_32BGRA`),
   `minimumFrameInterval` from `max_fps`, `showsCursor` from `opts.cursor`,
   `queueDepth` around 6, colour space sRGB.
4. Output handler: lock the `CVPixelBuffer`, copy tight rows into a recycled `Vec<u8>`,
   crop, `dirty` from `SCStreamFrameInfo.dirtyRects`, stamp `qpc` from the sample PTS,
   push to the bounded channel with the same drop-newest policy. Skip `.idle` frames
   (SCK sends status-only buffers when nothing changed).
5. `CaptureHandle::stop` stops the stream; stream errors end the channel (the drain
   already treats that as "ended early").
6. Window screenshot through `SCScreenshotManager` for the selector backdrop.
7. Screen Recording permission check before starting, with a clear error string the UI
   can turn into a "Grant access" prompt.

Done when: a 60 fps 1080p region and a full Retina display record to the frame store, with
Vuoom's panel absent from the frames, and the editor plays them back.

### Phase 3: input, hotkeys and the smooth pointer (2 to 3 days)

New `crates/vuoom-input/src/mac/`:

1. `recorder.rs`: `CGEventTapCreate` (session tap, head insert, active) on a dedicated
   thread with its own `CFRunLoop`. Mouse move/drag, down/up for left/right/other, scroll
   (convert line/pixel deltas to Windows wheel units, 120 per notch), key down/up and
   `flagsChanged` (modifiers as key down/up so the keystroke overlay sees Ctrl/Shift/Cmd).
   Re-enable the tap on `kCGEventTapDisabledByTimeout`. Stop via `CFRunLoopStop`.
2. `keymap.rs`: `kVK_*` to Win32 VK table (letters, digits, F keys, arrows, Return, Esc,
   Tab, Space, Delete/Backspace, modifiers, Cmd as `VK_LWIN`/`VK_RWIN`). Unit tested.
3. Chords: add a `meta` (Cmd) modifier to `Chord`, `parse("Cmd+Shift+Z")`, swallow by
   returning `NULL` from the tap callback. `is_down` via `CGEventSourceKeyState`.
4. `cursor_watch`: `NSCursor.currentSystemCursor` matched to the standard set by hotspot +
   image hash, on the tap thread, throttled.
5. Points to pixels conversion (section 4).
6. Accessibility / Input Monitoring permission handling, listen-only fallback.

Done when: clicks drive auto-zoom on a Mac take, Cmd+Shift+Z zooms live without reaching
the recorded app, the stop hotkey works from any app, the drawn pointer follows I-beam and
hand shapes, and the keystroke overlay shows `⌘ ⇧ K` style chips.

### Phase 4: the record flow on macOS (2 days)

The record flow reuses the one main window: it goes fullscreen for region selection over a
frozen screenshot, then shrinks to a small always-on-top stop panel. On macOS:

1. Do **not** use native fullscreen (`set_fullscreen(true)` slides into a new Space with an
   animation, and the frozen backdrop would sit in the wrong Space). Use a borderless window
   sized to the display frame with its level raised above the menu bar and Dock
   (`NSWindow.level`, `collectionBehavior = canJoinAllSpaces | fullScreenAuxiliary`) via
   `window.ns_window()`, or Tauri's simple fullscreen where the pinned version has it.
2. Stop panel: `NSPanel`-like behaviour (floating level, does not steal focus, visible on
   all Spaces), parked in a display corner.
3. Region border as a click-through native panel, excluded from capture.
4. Window picker overlay works off `SCShareableContent.windows` frames (points to pixels).
5. Escape and the countdown behave the same; check that focus returns to the previously
   active app after recording starts, so the user's clicks land where they expect.

Done when: Region, Full screen and Window flows all work on the built-in display and on an
external monitor, with no Space switching and nothing of Vuoom in the take.

### Phase 5: MP4 export with AVAssetWriter (2 days)

New `src-tauri/src/mp4/macos.rs` implementing the same `Mp4Encoder` surface (`new`,
`write_rgba`, `write_audio`, `has_audio`, `finish`):

1. `AVAssetWriter` (`.mp4`), video input H.264 High profile, average bitrate, keyframe
   every 2 s, `AVVideoColorPropertiesKey` BT.709 to match the Windows output.
2. Feed NV12 `CVPixelBuffer`s from a `CVPixelBufferPool` using the existing
   `rgba_to_nv12` (keeps colour identical across platforms), or BGRA and let VideoToolbox
   convert if that proves faster. Measure both.
3. AAC 48 kHz stereo 192 kbps audio input fed from the existing mixer; no audio track
   rather than failure if AAC setup fails (same rule as Windows).
4. Respect `readyForMoreMediaData` for back pressure (the Windows code relies on sink
   writer throttling for the same reason: 4K exports must not queue frames in RAM).
5. Write to the `.part` path and rename, as today.

Done when: 1080p60 and 4K exports play in QuickTime and Safari with correct colours and
sound, and the existing NV12 tests still pass.

### Phase 6: audio (2 days)

macOS `imp` in `vuoom-audio/src/capture.rs`:

1. Mic: `cpal` (Core Audio) input stream, device list with the default first, convert to
   the existing `SampleKind` path, timestamps from the callback's host time.
2. System sound: an audio output on the SCK stream (`capturesAudio = true`,
   `excludesCurrentProcessAudio = true`, 48 kHz stereo). For the level meter while framing
   (before capture starts) run a tiny audio-only SCK stream. `has_output()` is always true.
3. Keep the WAV writer, placement/resync logic and level meter untouched.

Done when: narration and system sound record in sync through cuts and speed-ups, waveforms
show, and MP4 exports carry the soundtrack.

### Phase 7: camera (1 to 1.5 days)

1. `AVCaptureDevice.DiscoverySession` for the list (built-in, external, Continuity Camera).
2. `AVCaptureSession` + `AVCaptureVideoDataOutput` BGRA at the format `choose_format`
   picks, frames into the existing `Shared` sink with host-clock stamps.
3. JPEG through a pure Rust encoder/decoder so `vuoom-camera::jpeg` works everywhere.

Done when: the webcam bubble previews live in the record HUD and appears in exports.

### Phase 8: captions on Apple Silicon (0.5 day)

1. Build whisper.cpp on arm64 (no AVX flags), enable `whisper-rs` `metal` feature on macOS.
2. Check the model download path lands in Application Support and the progress UI works.

Done when: one click captions a 2 minute narration in reasonable time on the M1 Pro.

### Phase 9: make it feel like a Mac app (2 days)

1. **Window chrome**: on macOS use native traffic lights (`titleBarStyle: "Overlay"`,
   `hiddenTitle: true` in a `tauri.macos.conf.json` override), hide `WindowControls.tsx`,
   pad the titlebar left for the lights, keep `data-tauri-drag-region`.
2. **Shortcuts**: a single `mod` helper (`isMac ? e.metaKey : e.ctrlKey`) replacing the
   ~27 `ctrlKey` checks (21 of them in `src/editor/createEditor.ts`), and a label formatter
   that renders the ~52 `"Ctrl+..."` strings as `⌘` glyphs on Mac. Redo as `⌘⇧Z`.
   `Delete` already accepts `Backspace`, good.
3. **App menu**: macOS expects one. Build a menu with the app items, File (New recording,
   Open, Save, Export), Edit and View. Careful: native Undo/Copy/Paste items swallow
   `⌘Z/⌘C/⌘V` before the webview's keydown handler, so either route them to the editor's
   own actions via menu events or leave them to text fields only. Verify in practice.
4. **Menu bar icon**: a monochrome template icon for the tray on macOS.
5. **Copy**: "File Explorer" to "Finder", "Windows wouldn't allow writing there" made
   neutral, "Windows versions" in Settings diagnostics, README platform badges. Update
   `src/locales/zh-CN.json` and run `scripts/check-i18n.mjs`.
6. **Defaults**: hotkeys `Cmd+Shift+R/Z/X` on Mac (see open questions), save folder
   `~/Movies/Vuoom`.

### Phase 10: packaging, signing, updates, CI (1.5 to 2 days)

1. `bundle.macOS`: `minimumSystemVersion`, Info.plist usage strings, entitlements
   (`com.apple.security.device.camera`, `com.apple.security.device.audio-input`),
   hardened runtime, `.icns` already exists.
2. Signing and notarization through `tauri-action` with `APPLE_CERTIFICATE`,
   `APPLE_SIGNING_IDENTITY`, `APPLE_ID`/`APPLE_PASSWORD`/`APPLE_TEAM_ID` secrets.
3. Release workflow: add a `macos-14` (arm64) job; optionally `x86_64-apple-darwin` or a
   universal build. Merge `darwin-aarch64` into `latest.json` with the Windows entries.
4. CI: a macOS job running clippy (`-D warnings`) and the test suite, next to the existing
   Windows job, so neither platform can rot again.
5. Website download page and README: a macOS download.

---

## 7. Risks and gotchas

| Risk | Impact | Mitigation |
|---|---|---|
| **Unsigned builds lose permissions on every update.** TCC ties Screen Recording / Accessibility grants to the code signature; an ad-hoc signed app gets a new identity each build | Users re-grant permissions after each auto-update, and Gatekeeper blocks the first launch ("Open Anyway" in Settings) | Apple Developer ID ($99 a year) + notarization. Strongly recommended before any public Mac release |
| macOS 15+ periodic "bypass the private window picker" prompts | Mild nag for screen recorders that use `SCShareableContent` directly | Accept for now; later consider `SCContentSharingPicker` for window picks |
| Purple recording indicator in the menu bar | Visible in full display captures | Expected on macOS; the "below the menu bar" region hides it |
| Points vs pixels on mixed scale monitors | Zooms aim at the wrong spot | Section 4 helper + unit tests + manual test with an external monitor |
| Event tap disabled by the system under load | Clicks stop being logged mid take | Handle `kCGEventTapDisabledByTimeout`, re-enable, keep the callback tiny |
| Menu key equivalents eating `⌘Z` etc. | Editor undo stops working | Phase 9 item 3, test early |
| 4K60 BGRA throughput | Dropped frames | The pipeline already copies once per frame; Apple Silicon unified memory helps. Measure with the existing `engine-4k` style benchmarks |
| `whisper-rs` + Metal build on CI | Release build breaks | Pin the version, cache the CMake build, keep a CPU only fallback |
| Upstream drift | Rebasing pain if upstream keeps shipping Windows features | Small, isolated `cfg` modules; rebase often |

---

## 8. Testing on this Mac

- `cargo test --workspace` on macOS in CI and locally (all pure crates already have tests).
- New unit tests: keycode map, points to pixels, NV12 to `CVPixelBuffer` layout, scroll
  delta conversion, chord parsing with `Cmd`.
- Manual checklist per phase, run on the M1 Pro (built-in Retina display, notch) and an
  external 1x monitor:
  1. Region / full screen / window takes at 24, 30, 60 fps; Vuoom never in frame.
  2. Auto-zoom lands on clicks; manual zoom chord; stop chord from another app.
  3. Smooth pointer shapes; click ripples; keystroke overlay.
  4. Mic + system audio in sync after cuts and speed-ups.
  5. Webcam bubble; captions; GIF and MP4 export; Copy then paste into Slack.
  6. Crash recovery: kill the app mid take, reopen, recover.
  7. Permissions: fresh user account, deny each permission, check the UI explains it.

---

## 9. Open questions (decide before Phase 2)

1. **Fork or upstream?** Is this going back to `Razee4315/Vuoom` as a pull request, or
   living in our own fork? It changes how strictly we keep the diff small.
2. **Minimum macOS version.** Recommendation: **macOS 14 Sonoma**. It gives
   `SCScreenshotManager`, `captureResolution`, and SCK audio with no fallbacks to write.
3. **Apple Silicon only, or Intel too?** Recommendation: arm64 first, add a universal
   build in Phase 10 if wanted (whisper.cpp then needs both builds).
4. **Apple Developer account** for signing and notarization (see risks). Without it, Mac
   users get a rough first launch and repeat permission prompts.
5. **Mac hotkey defaults.** Recommendation: `⌘⇧R` record, `⌘⇧Z` zoom, `⌘⇧X` stop,
   mirroring Windows `Ctrl+Shift`. The zoom chord is swallowed while recording, so its
   clash with Redo is the same trade the Windows build already makes.

---

## 10. Effort summary

| Phase | Days |
|---|---|
| 0 Build on macOS, sample take works | 0.5 to 1 |
| 1 Platform seam + easy helpers | 1 |
| 2 ScreenCaptureKit capture | 3 to 4 |
| 3 Input, hotkeys, pointer | 2 to 3 |
| 4 Record flow windows | 2 |
| 5 MP4 export | 2 |
| 6 Audio | 2 |
| 7 Camera | 1 to 1.5 |
| 8 Captions | 0.5 |
| 9 Mac feel (chrome, shortcuts, menu, copy) | 2 |
| 10 Packaging, signing, CI | 1.5 to 2 |
| **Total** | **about 17 to 21 working days** |

A useful **first milestone** is Phases 0 to 3 plus a minimal Phase 4 (about 9 to 11
days): record a region on the Mac with auto-zoom, edit it, and export a GIF. MP4, audio,
camera and polish follow.
