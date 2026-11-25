# 17, Tungsten Darkroom Redesign (2026-11-25)

Co-designed with GPT-6.2 Sol and executed across `website/`, superseding `docs/15-paper-cut-redesign.md`.

## 1. Visual Direction
- **Identity**: *Tungsten Darkroom* (default darkroom `#110F0E`) and *Daylight Archival Studio* (`#F4F0E8` toggleable via header lighting control and persisted in `localStorage['vuoom-theme']`).
- **Palette**: Smoked obsidian glass, blackened aluminum, archival warm stone, 1px specular top-edge rims (`--rim`), Vuoom Record Crimson (`#E5484D` / `#C62F38`), warm tungsten accent (`#E4A45F`), and emerald status (`#4FB37F`). Zero purple anywhere.
- **Typography**: `Bricolage Grotesque Variable` display, `Geist Variable` body & controls, `Geist Mono Variable` timecodes, specs, and byte readouts.
- **Real Product Proof Only**: Replaced legacy mock-bar editor screenshots with genuine `take-flat.png`, `take-zoom.png`, `screenshot-home.png`, and real video reels (`demo.mp4`, `github.mp4`, `voice.mp4`, `launch.mp4`).

## 2. Bespoke Tactile Cut-Paper Graphics (`website/tools/`)
13 cohesive screen-recording cut-paper graphics, minimalist wallpapers, and theme-matched backdrop plates generated via `codex exec -m gpt-6.2-astra` `image_gen` (`tools/manifest.mjs`, `tools/generate.mjs`) and audited/trimmed/encoded to WebP (`tools/process.py` -> `src/assets/studio/*.webp`), including three pixel-aligned `-i` edit chains (`hero-bg-dark` -> `hero-bg-light`, `graphic-zoom-rest` -> `graphic-zoom-active`, and `closing-bg-dark` -> `closing-bg-light`).

## 3. Signature Interactive Systems
1. **Centered Hero Camera Stage (`CameraPlayground.astro` / `camera-playground.ts`)**: Minimalist centered hero over a full-screen-first dark `screenshot-home.png` spring stage (resting at `1.00×`, demonstrating zoom in and smooth zoom out back to `1.00×`, or single-click toggle in/out + keyboard reticle) paired with flanking `graphic-pointer` and `graphic-zoom-rest` -> `graphic-zoom-active` cutouts and launch-film `<dialog>`.
2. **Minimalist Single-Row Capability Ribbon (`CapabilityRibbon.astro`)**: GPU-composited continuous marquee of 10 shipped-feature glass pills linking directly to their pages, pausing on hover/focus and wrapping cleanly under `prefers-reduced-motion: reduce`.
3. **Dark Cinema Screening Room (`ScreeningRoom.astro` / `screening-room.ts`)**: Centered dark theater switching between `github.mp4` and `voice.mp4` (`preload="none"` at rest).
4. **Gallery Studio Finisher (`StudioFinisher.astro` / `studio-finisher.ts`)**: Clean frameless preview card wrapping dark `<Video name="github" deferSource />` inside `None`/`Subtle`/`Studio` frames and 4 balanced wallpapers (`Crimson`, `Sage`, `Paper`, `Obsidian`) alongside `graphic-timeline`.
5. **Streamlined Voice Cleanup + Export Estimator (`AudioCaptionsPanel.astro`, `ExportEstimator.astro`)**: Ambient RNNoise noise-floor waveform + Web Audio toggle (`graphic-soundwave`), cycling live subtitle strip, and preset-calibrated GIF/MP4 byte calculator (`graphic-export`) with `Fit under 10 MB`.
6. **GSAP Motion Choreography (`motion.ts`)**: Hero staggered entrance, scroll-triggered `[data-reveal]` / `[data-reveal-stagger]` reveals, and subtle scrubbed depth parallax under `gsap.matchMedia()`.

## 4. Multi-Platform v2.2.0 Coverage
Updated copy, download routing (`lib/github.ts`, `scripts/download.ts`, `pages/download.astro`), schema, FAQ, and comparisons for **Windows 10/11** (`.exe`/`.msi`) and **macOS 14+ Apple Silicon** (`.dmg`).
