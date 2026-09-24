# 05, Motion spec

Library floor: GSAP 3 + ScrollTrigger + SplitText (all free since 3.13), Lenis 1. One RAF loop:
Lenis is driven by `gsap.ticker`. All timelines live inside
`gsap.matchMedia().add('(prefers-reduced-motion: no-preference)', …)`.

## Scroll architecture

```js
const lenis = new Lenis({ lerp: 0.1, smoothWheel: true, syncTouch: false })
lenis.on('scroll', ScrollTrigger.update)
gsap.ticker.add(t => lenis.raf(t * 1000)); gsap.ticker.lagSmoothing(0)
```

Lenis is not created when `(pointer: coarse)` or reduced motion. `lenis.stop()` while the
mobile menu or countdown is open.

## Hero load timeline (first visit: after countdown; repeat visit: at DOMContentLoaded + fonts)

| t (ms) | What |
|---|---|
| 0 | Nav fades from `y:-8, opacity 0`, 600ms `expo.out` |
| 80 | Slate label types in (chars, 0.012s stagger), `REC ●` light fades to red and starts blinking at 1 Hz |
| 150 | H1 lines mask-reveal `yPercent 110 → 0`, 1000ms `expo.out`, 0.08s stagger |
| 450 | Lead paragraph words fade up `y 12 → 0`, 800ms, 0.02s stagger |
| 600 | CTA row `y 16, opacity 0 → 1`, 700ms; primary button record light pulses once |
| 500 | Stage iris-reveals: `clip-path: inset(50% 50% 50% 50% round 14px) → inset(0 round 14px)`, 1100ms `expo.out`, screenshot inside settles `scale 1.08 → 1` |
| 1400 | Pointer enters the stage, first take loop begins (M10) |

Total perceived sequence ≈ 1.2 s, overlapped.

## Inventory

| ID | Element | Technique | Trigger | Property | From → To | Dur | Ease | Stagger | Reduced motion |
|---|---|---|---|---|---|---|---|---|---|
| M01 | Section h2 | Masked line reveal (SplitText `mask:'lines'`) | top 80% | yPercent | 110 → 0 | 1000 | expo.out | 0.08s | Static |
| M02 | Paragraphs, list rows | Reveal-on-scroll | top 85% | y, opacity | 24, 0 → 0, 1 | 800 | expo.out | 0.06s | opacity only, 200ms |
| M03 | Buttons | Label roll: label slides up, duplicate enters from below | hover | yPercent | 0 → -100 | 240 | quart.out | none | colour change only |
| M04 | Buttons, keycaps | Press | :active | transform | → translateY(1px) scale(.98) | 120 | quart.out | none | kept (not motion-heavy) |
| M05 | Text links | Wipe underline, `::after scaleX`, origin left in, right out | hover/focus | scaleX | 0 → 1 | 240 | quart.out | none | underline always shown |
| M06 | Hero stage | Scroll-linked exit | hero scroll 0 → 100% | scale, opacity, y | 1, 1, 0 → .92, .35, -6% | scrub 1 | none | none | none |
| M07 | **The Problem** (pinned, `+=250%`) | Pinned scrubbed timeline | top top | see below | | scrub 1 | none | | Static final frame + text |
| M08 | Before/after wipe | Draggable clip-path inset | pointer / keyboard / scroll-in | `--split` | 50% | direct | none | | fully usable, no auto-sweep |
| M09 | **Editor timeline** (pinned, `+=300%`) | Pinned scrubbed timeline | top top | block scaleX, preview camera | see below | scrub 1 | none | | Static completed timeline |
| M10 | Stage "live take" | Signature: spring camera + pointer + ripple | in view, loops | camera x,y,zoom | spring hl .30/.22 | spring | spring | | Static screenshot, no loop |
| M11 | Countdown preloader | 3-2-1 then record light | first visit per session | numeral scale/opacity | 1.2 → 1 | 3 × 220ms | expo.out | | Skipped |
| M12 | Smooth pointer | Lerped follower with press ring | pointermove | x, y | spring hl .12 | spring | | | Native cursor |
| M13 | Feature index media | Parallax | row in view | yPercent | 8 → -8 | scrub | none | | none |
| M14 | Nav | Compacts after hero, hides nothing | scroll > 80vh | height token, backdrop | 72 → 56px via transform scaleY on bg | 300 | expo.out | | instant |
| M15 | Footer | Curtain reveal (`position: sticky`) | scroll | none (layout) | | | | | same |
| M16 | Footer wordmark | Char reveal + red dot lights | top 80% | yPercent | 100 → 0 | 900 | expo.out | 0.03s | static |
| M17 | Keycaps `Ctrl` `Shift` `Z` | Press in sync with stage zoom, and on real keypress | event | translateY | 0 → 2px | 120 | quart.out | 0.04s | colour only |
| M18 | Stars counter | Count-up | in view once | text | 0 → n | 900 | expo.out | | final number |

## M10, the signature, buildable

- DOM: `.stage` (viewfinder frame, overflow clip, 16:10) > `.cam` (transform target) > `<img>` screenshot 1440×900 + absolutely placed `.hot` spots in % coordinates, plus `.pointer` SVG and `.ripple` inside `.cam` so they scale with the zoom exactly like the app.
- A "take" is a script: `[{ at: [x%, y%], zoom: 1.8, hold: 1.6s }, …, { zoom: 1 }]`, 4 to 5 beats, ~9 s, loops.
- Each frame: pointer target springs with `hl .12`; on arrival a click: pointer scales .86 for 120ms, ripple (red ring, `scale 0 → 2.4`, opacity `.9 → 0`, 600ms expo.out), camera goal = click point, zoom goal = beat zoom; camera center and zoom run `spring_update` with `hl_pan .22`, `hl_zoom .30`; `clamp_camera` keeps edges in frame. Transform: `translate(-cx*100% …) scale(z)` with origin top-left, computed in JS.
- The slate timecode `00:00:SS:FF` advances at 30 fps while the take plays. `REC ●` blinks.
- Keycaps (M17) depress on every zoom-in beat.
- Loop pauses when the stage is off-screen (IntersectionObserver) and when the tab is hidden.

## M07, The Problem, buildable

Pinned 250% of viewport.
0 → 30%: full 1920×1080 screenshot shown at "GIF width" (640px box), a 14px outline marks the Export button; big type "Your screen is 1920 pixels wide." then "The button you clicked is 14." (`1920` and `14` as `--step-5` tabular numerals).
30 → 70%: the camera zooms onto the button using the same spring look (driven by scrub progress through a pre-computed eased path), the 14 counts up to the on-screen size (e.g. 14 → 38).
70 → 100%: line "Vuoom does this for every click. Automatically." and the red record light.

## M09, Editor timeline, buildable

Pinned 300%. The editor screenshot's timeline area is rebuilt in HTML. As progress runs:
zoom block 1 grows (scaleX from left), preview camera zooms; text block "Click here first" appears on the preview; speed block `3×` grows and the timecode speeds; cut block turns the preview black briefly; final length readout ticks `14.4s → 12.4s`. Four captions on the left swap per quarter (`Zoom`, `Annotate`, `Skim`, `Cut`).

## Budget

- Animated elements on home ≈ 60; WebGL: no; preloader: yes (≤ 900 ms, skippable by any key or click, never blocks content from crawlers since content is in the DOM)
- JS: GSAP core + ST + SplitText + Lenis + site ≈ 95 KB gzip, loaded `type=module` deferred
