# 08, Assets

Treatment recipe: genuine product recordings (`demo.mp4`, `github.mp4`, `voice.mp4`, `launch.mp4`) and real high-resolution captures (`take-flat.png`, `take-zoom.png`, `screenshot-home.png`), paired with bespoke art-directed `gpt-6.2-astra` tactile layered cut-paper screen-recording graphics, wallpapers, and transparent alpha cutouts (`src/assets/studio/*.webp`). Dummy mock-bar screenshots are banned from the live site.

## Product & Video Assets

| ID | Slot | Subject | Aspect | Source dims | Format | Source | Status | Alt |
|---|---|---|---|---|---|---|---|---|
| IMG01 | Home camera playground, Stage, Pointer lab, Before/after flat | Real Aurora 2.4 launch dashboard | 16:9 | 1600×900 | AVIF/WebP | `src/assets/shots/take-flat.png` | ready | Full-screen capture of the Aurora 2.4 launch dashboard |
| IMG02 | Before/after Vuoom, OG cards, features | Real Aurora 2.4 launch dashboard zoomed 2.5x | 16:9 | 1600×900 | AVIF/WebP | `src/assets/shots/take-zoom.png` | ready | Vuoom auto-zoom locked onto the sign-up counter reading 12,408 |
| IMG03 | Home native craft & crash recovery, Guide, Features | Vuoom Home screen | 16:10 | 1440×900 | AVIF/WebP | `src/assets/shots/screenshot-home.png` | ready | Vuoom's home screen with Region, Full screen, Window and crash recovery |
| LOGO | Header, favicon, press | Logo mark | 1:1 | 1024 | SVG + PNG | `.github/assets/logo.png` | ready | Vuoom |
| OG | Every page | Generated Tungsten Darkroom card | 1200×630 | 1200×630 | PNG | `src/pages/og/[slug].png.ts` | generated | (meta, no alt) |
| VID01 | Screening room, Studio Finisher, Guide, GIF export | Aurora dashboard take with auto-zoom | 16:9 | 1600×900 30 fps | MP4 + poster | `public/media/demo.mp4` | ready | A dashboard recording where the camera zooms into each click |
| VID02 | Screening room, Auto-zoom feature page | Browser take with zoom and motion blur | 16:9 | 1000×562 | MP4 + poster | `public/media/github.mp4` | ready | Zooming into a GitHub page in a browser, with motion blur |
| VID03 | Screening room, Captions feature page | Narrated take with burned-in captions | 16:9 | 1000×562 | MP4 + poster | `public/media/voice.mp4` | ready | The Vuoom README recorded with captions reading "I am using Vuoom" |
| VID04 | Launch film modal, Download page, Press | Full Vuoom launch film | 16:9 | 1920×1080 | MP4 + poster | `public/media/launch.mp4` | ready | Vuoom launch film, 1 minute 57 seconds |

## Tactile Cut-Paper Graphics & Wallpapers (`src/assets/studio/*.webp`)

| ID | Slot | Transparent | Dims | Bytes | Pipeline | Alt |
|---|---|---|---|---|---|---|
| `hero-bg-dark` | Hero darkroom background plate | `false` | 1914×822 | 30,254 | `gpt-6.2-astra` `image_gen` -> `process.py` WebP | Minimalist dark matte layered cut-paper backdrop with warm geometric light depth |
| `hero-bg-light` | Hero daylight background plate | `false` | 1913×822 | 30,528 | Edit chain from `hero-bg-dark` (`-i`) | Warm ivory and soft sand architectural cut-paper hero backdrop |
| `graphic-pointer` | Hero corner pointer cutout | `true` | 560×673 | 77,366 | `gpt-6.2-astra` `image_gen` -> `process.py` WebP | Tactile cut-paper cursor arrow with frosted acrylic click ripple rings |
| `graphic-zoom-rest` | Hero right foreground zoom frame (rest) | `true` | 560×554 | 77,382 | `gpt-6.2-astra` `image_gen` -> `process.py` WebP | Layered paper-cut 4-corner viewport crop brackets in resting state |
| `graphic-zoom-active` | Hero right foreground zoom frame (zoomed) | `true` | 560×554 | 78,128 | Edit chain from `graphic-zoom-rest` (`-i`), union-bbox aligned | Tightly focused cut-paper viewport brackets with crimson lock dot |
| `wallpaper-crimson` | Studio Finisher wallpaper 01 | `false` | 1400×788 | 27,154 | `gpt-6.2-astra` `image_gen` -> `process.py` WebP | Minimalist abstract charcoal and warm terracotta-crimson layered desktop wallpaper |
| `wallpaper-paper` | Studio Finisher wallpaper 02 | `false` | 1400×788 | 26,210 | `gpt-6.2-astra` `image_gen` -> `process.py` WebP | Clean sunlit ivory and sand layered paper waves desktop wallpaper |
| `wallpaper-sage` | Studio Finisher wallpaper 03 | `false` | 1400×788 | 26,370 | `gpt-6.2-astra` `image_gen` -> `process.py` WebP | Calm muted forest sage and charcoal cut-paper desktop wallpaper |
| `graphic-timeline` | Studio Finisher timeline graphic | `true` | 680×531 | 49,982 | `gpt-6.2-astra` `image_gen` -> `process.py` WebP | Layered paper-cut video editing timeline tracks with crimson playhead |
| `graphic-soundwave` | Voice & captions studio graphic | `true` | 560×275 | 27,580 | `gpt-6.2-astra` `image_gen` -> `process.py` WebP | Sculpted cut-paper soundwave equalizer bars paired with clean caption card layer |
| `graphic-export` | Export estimator studio graphic | `true` | 640×537 | 44,906 | `gpt-6.2-astra` `image_gen` -> `process.py` WebP | Layered glossy cut-paper video clip cards and crisp download arrow |
| `closing-bg-dark` | Closing download CTA plate (dark) | `false` | 1914×822 | 35,378 | `gpt-6.2-astra` `image_gen` -> `process.py` WebP | Minimalist dark matte cut-paper stage waves for closing download band |
| `closing-bg-light` | Closing download CTA plate (light) | `false` | 1914×822 | 27,580 | Edit chain from `closing-bg-dark` (`-i`) | Minimalist warm paper stage waves for closing download band |

Icons: one hand-drawn set in `src/components/Icon.astro`, 20px grid, 1.5px stroke, round caps. No emoji.
