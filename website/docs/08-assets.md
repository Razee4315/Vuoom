# 08, Assets

Treatment recipe, applied to every product image: shown exactly as captured (the product is
the colour), inside the 14px frame with `--shadow-stage`, on `--ground-raised`. No filters,
no tilt, no fake reflections. Converted to AVIF + WebP at 1x and 2x by Astro's image pipeline.

| ID | Slot | Subject | Aspect | Source dims | Format | Source | Status | Alt |
|---|---|---|---|---|---|---|---|---|
| IMG01 | Home stage, Problem, Editor pin | Editor, dark theme | 16:10 | 1440×900 | AVIF/WebP | `.github/assets/screenshot-editor.png` | ready | The Vuoom editor with zoom blocks, notes and a cut on the timeline |
| IMG02 | How it works 01, guide | Home screen | 16:10 | 1440×900 | AVIF/WebP | `.github/assets/screenshot-home.png` | ready | Vuoom's home screen with Region, Full screen and Window options |
| IMG03 | How it works 01 alt, features | Region picker HUD | 16:10 | from repo | AVIF/WebP | `.github/assets/screenshot-record.png` | ready | Picking a region to record with the record HUD |
| IMG04 | Export section, GIF page | Export card | 16:10 | from repo | AVIF/WebP | `.github/assets/screenshot-export.png` | ready | The export card with GIF and MP4 presets and a size estimate |
| IMG05 | Editor page | Editor, light theme | 16:10 | from repo | AVIF/WebP | `.github/assets/screenshot-editor-light.png` | ready | The Vuoom editor in the light Paper theme |
| IMG06 | Before/after "flat" | IMG01 at 1x, full frame | 16:10 | derived | CSS | derived | ready | Flat recording: the whole screen, text too small to read |
| IMG07 | Before/after "Vuoom" | IMG01 crop at 2.2x on the zoom target | 16:10 | derived | CSS | derived | ready | Vuoom recording: zoomed in on the clicked control |
| LOGO | Header, favicon, press | Logo mark | 1:1 | 1024 | SVG (redrawn from PNG) + PNG | `.github/assets/logo.png` | redraw as SVG | Vuoom |
| OG | Every page | Generated card | 1200×630 | | PNG | build script | generated | (meta, no alt) |
| VID01 | Home "A real take" section, auto-zoom page | Owner's Aurora dashboard take, made with Vuoom, unedited | 16:9 | 1600×900 30 fps, 2.9 MB | MP4 H.264 CRF 26, faststart, no audio, poster | Desktop/demo-1080p60.mp4 | ready | A dashboard recording where the camera zooms into each click |
| VID02 | Auto-zoom page | Browser take with zoom and motion blur | 16:9 | 1000×562, 0.6 MB | MP4 | Desktop/github.mp4 | ready | Zooming into a GitHub page in a browser, with motion blur |
| VID03 | Captions page | Narrated take with burned-in captions | 16:9 | 1000×562, 0.13 MB, AAC | MP4 | Desktop/voice.mp4 | ready | The Vuoom README recorded with captions reading "I am using Vuoom" |
| VID04 | Hero (future) | Owner's launch video | 16:9 | 1920×1080 | MP4 + poster | owner | pending, slot ready | Vuoom launch video |
| rejected | x_post.mp4 | Shows third-party tweets, names, avatars and a meme | | | | | not used | |

Budget: home page images ≤ 450 KB total transferred at 1x on mobile; LCP is the h1 text.
Videos: `preload="none"`, poster, `muted playsinline loop`, played only in view, never under
reduced motion (poster + play button instead).

Icons: one hand-drawn set in `src/components/Icon.astro`, 20px grid, 1.5px stroke, round caps
(matches the app's lucide-style chrome without importing a library). No emoji.

## Placeholder register

| Slot | Blocker for launch? |
|---|---|
| VID04 launch video | No. The stage loop is the complete fallback |
| SVG logo | Yes, until redrawn (done in build phase 1) |

**Drop-in:** put files in `website/public/media/` with the names in `src/data/media.ts`; the
components switch from screenshot to video automatically.

## Paper Cut inline SVGs
StudioDoodle.astro: original viewfinder/pointer, cut-paper sparkle and play-button orbit; decorative, currentColor, hidden from assistive technology. Header.astro: original sun/moon toggle, download arrow and animated menu strokes. Faq.astro: original plus/minus disclosure mark. All are code-native SVGs with explicit dimensions.
