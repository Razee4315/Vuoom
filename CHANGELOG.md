# Changelog

All notable changes to Vuoom. Newest first. Full notes for every version are on
[GitHub Releases](https://github.com/Razee4315/Vuoom/releases) and on the
[website](https://razee4315.github.io/Vuoom/changelog/).

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions
follow [Semantic Versioning](https://semver.org/). Vuoom updates itself from inside the app.

## [Unreleased]

### Added
- The Vuoom website at <https://razee4315.github.io/Vuoom/>: download, features, comparisons,
  guide, shortcuts, FAQ, changelog, privacy, terms, security, licences and a press kit.
- `CHANGELOG.md`, `PRIVACY.md` and `SUPPORT.md`.
- Auto zoom: a take recorded without pressing Ctrl+Shift+Z now gets a few calm zooms where
  you clicked (at most four a minute, none longer than 8 seconds), with a Remove button on
  the notice. Press the hotkey even once and only your own zooms are used. Switch it off in
  Home > Options, the record panel's Options menu or Settings.
- The smooth pointer changes shape like the real one: a text beam over text, a hand over a
  link, resize and move arrows, a crosshair.
- The zoom and stop hotkeys can be changed (Settings > Shortcuts), and the app you are
  recording no longer receives them: Ctrl+Shift+Z used to also trigger Redo in it.
- The region picker offers the area you recorded last time, and Record remembers whether you
  last recorded a region, the full screen or a window.
- MP4 exports keep the take's own size, up to 4K, and its frame rate. "Balanced" is 1080p at
  30 fps and "High quality" is the full size at up to 60 fps (they were 1000 and 1280 px wide).
- GIF gradients (backdrops, shadows, the webcam) are dithered so they don't show bands, with
  a switch in the export card. Flat interface colors are left as they are.

### Changed
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

[Unreleased]: https://github.com/Razee4315/Vuoom/compare/v2.0.0...HEAD
[2.0.0]: https://github.com/Razee4315/Vuoom/releases/tag/v2.0.0
