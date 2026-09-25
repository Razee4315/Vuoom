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

### Fixed
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
