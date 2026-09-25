# 02, Art direction: Live Take

Approved 2026-09-25. The authority on every visual question.

> **Signature move.** The page behaves like a Vuoom recording: a real Vuoom screenshot sits on
> a stage, a smooth red-dot pointer clicks inside it, a ripple fires, and the page's own
> "camera" springs into the click (critically damped, never a cut), while a `REC` timecode in
> the slate counts the take. The visitor experiences auto-zoom before reading about it.

| Field | Decision | Source |
|---|---|---|
| Name | Live Take | original |
| Feeling | Precise, cinematic, quiet, confident, a little playful. It says Vuoom is a camera operator, not a toolbar. | original |
| Palette | `--ground #0B0B0C` (the app's window colour), `--ink #F2F0EB` warm off-white, used at 100 / 70 / 52 / 12% alpha, `--rec #E5484D` record red. Three colours. Red means live, click, record. Never decoration. | steal #4, #5, #8 |
| Type | Display: **Bricolage Grotesque** (Mathieu Triay, OFL) variable, `wdth 75`, weight 720, `opsz 96`. Text: **Geist** (Vercel, OFL). Labels and timecode: **Geist Mono**. | grotesque display + mono accents, craft §1 strategy 3 |
| Type scale | Display `--step-6` 12rem max / body 1.0625rem = **11.3x**. | anti-slop type rule |
| Grid | 12 columns, 24px gutter (16 below 640px), max-width 1360px, 16px side gutter on phones. | original |
| Density | Spacious. Section padding `clamp(6rem, 12vw, 12rem)`. Two sections break density on purpose: the feature index (dense rows) and the footer sitemap. | anti-slop §3 |
| Corners | **2px** for UI chrome (buttons, keycaps, slates). Screenshots sit in a frame with **14px**, matching the app's own window radius, because they are the product, not site chrome. Two values, each argued. | original |
| Borders | Borderless surfaces. Hairlines (`--ink` at 12%) only as rules between index rows and in the slate. | steal #7 |
| Texture | Film grain, SVG `feTurbulence` baked to a 256px tile, 4% opacity, `pointer-events:none`. A viewfinder frame (four corner brackets) around the stage. | original, extends steal #7 |
| Hero | **Product-led, left-aligned**. H1 bottom-left over the stage, slate strip above it, stage fills the right 7 columns and bleeds off the right edge on desktop (grid break). Load: slate types in, h1 lines mask-reveal, stage clip-reveals from its centre like an iris, then the pointer takes its first click. | steal #1, techniques §1 |
| Motion | Default curve `--ease-out-expo cubic-bezier(0.16,1,0.3,1)`. Camera moves port `spring_update` from `crates/vuoom-zoom/src/camera.rs` verbatim: critically damped, half-life zoom 0.30 s, pan 0.22 s, pointer 0.12 s (the app's `config.rs` defaults). Exits use `--ease-in-quart`. | app `vuoom-zoom` |
| Nav | Persistent header, compacts from 72px to 56px after the hero. Links with wipe underlines. The Download button is always visible, including on phones. | techniques §4 |
| Footer | Curtain reveal (sticky) holding an oversized **VUOOM●** wordmark whose dot is the record light, plus a dense sitemap. | techniques §3 |
| Cursor | Desktop only: the native pointer is replaced by the Vuoom smooth pointer (white arrow, dark outline) that lerps behind the real one, grows a red press ring on mousedown. Native cursor kept over text fields and in reduced motion. | steal #6 adapted, original |

## Motion inventory (Showpiece, 13)

1. Scroll reveals, `expo.out`, 0.08s stagger, varied by element class
2. Hover transforms on every interactive element
3. Press feedback, `translateY(1px) scale(0.98)`
4. Wipe underlines on links
5. Lenis wired to the GSAP ticker, off on touch
6. SplitText masked line reveal on every h1 and section h2
7. Pinned scrubbed section: **The Problem** (1920px vs 14px, camera zooms as you scroll)
8. Scroll-linked transform: hero stage scales and dims as it leaves; feature index media parallax
9. Choreographed hero load (timeline in `05-motion-spec.md`)
10. Signature: the live-take stage with pointer, ripple and spring camera
11. Preloader: a 3-2-1 record countdown (the app's own), under 900 ms, first visit per session only, content already in the DOM behind it
12. Custom smooth pointer, desktop only
13. Second signature-grade effect: pinned **editor timeline** that fills with zoom, speed, cut and text blocks as you scroll, with the preview frame responding. Plus the before/after wipe as a bonus interaction

## Anti-list

- No violet, blue, gradients on text, glassmorphism blobs, or 3D shapes
- No three-card icon grids. Features are an editorial index
- No "AI" in any headline. Captions are "offline", "on your computer"
- No stock photography or illustration. The product is the only imagery
- No fake numbers. Stars and version are fetched from GitHub
- No light mode. The product is dark-first and so is the site. (`prefers-color-scheme` is not honoured on purpose; logged in ADR-006)
- No em dashes in copy

## Forbidden (copied from anti-slop, adapted)

No `#6366F1` / `#8B5CF6` or any purple. No gradient text. No Inter, Roboto, Arial as a chosen face.
No three-card icon grids. No 1px gray card borders. No soft drop shadows (the stage uses one
deep ambient shadow at 40% black, argued: it is a lit object on a dark set). No centered hero.
No bounce or elastic easing. No stock photography. No lorem ipsum. No `border-radius: 8px`.
No emoji as icons (icons are hand-drawn 1.5px-stroke SVGs in one set). No unrequested light mode.

## Risk

The signature relies on a screenshot, not live video, so it can look like a slideshow if the
spring timing is off. Mitigation: camera moves reuse the app's spring maths, and each click has
ripple + pointer press so the cause of every zoom is visible. When the owner's launch video
arrives, it replaces the stage loop in the hero and the stage moves to The Problem section.

## Superseded 2026-09-26
The current direction is docs/15-paper-cut-redesign.md, following the owner's redesign request.
