# Vuoom website rules

## Stack
Astro 7 static, GSAP 3.15+ (ScrollTrigger), native scrolling, fonts via @fontsource-variable.
Deployed to GitHub Pages by `.github/workflows/pages.yml`. Base path: `/Vuoom/` (see astro.config.mjs).

## Docs
Art direction: `docs/17-tungsten-darkroom-redesign.md` is current and supersedes `docs/15-paper-cut-redesign.md` and `docs/02-art-direction.md`.
Tokens: `docs/03-design-tokens.md` + `src/styles/tokens.css`
Motion: `docs/05-motion-spec.md` · Pages: `docs/06` · Copy + SEO: `docs/10`

## Hard rules
1. No raw values in components. Everything resolves to a token.
2. Prefer transform and opacity. The FAQ uses a bounded, interruptible height transition.
3. Every interactive element has a visible `:focus-visible` state.
4. Explicit width and height on every image and video.
5. Every animation has a reduced-motion fallback: `gsap.matchMedia()` for JavaScript and media queries for CSS.
6. SplitText only after `await document.fonts.ready`, aria-label on the parent.
7. `ScrollTrigger.refresh()` after fonts and images load.
8. No new image without a row in `docs/08-assets.md`.
9. Code diverging from a doc: update the doc, log an ADR, then code.
10. Never claim a test or audit passed without running it.
11. Internal links go through `url()` from `src/lib/site.ts` so the base path is respected.
12. Copy: no em dashes, "pointer" not cursor, no "AI" headlines, only features that ship (Windows 10/11 & macOS 14+ Apple Silicon).

## Forbidden
No purple (`#6366F1`, `#8B5CF6` or any). No gradient text. No Inter. No dummy mock-bar screenshots.
No soft drop shadows beyond `--shadow-stage`. No bounce or elastic.
No stock imagery. No lorem ipsum. No emoji as icons.

## Signature move
Tungsten Darkroom surfaces paired with `gpt-6.2-astra` tactile layered cut-paper screen-recording graphics (pointer, zoom brackets, timeline tracks, soundwave, export cards), genuine Vuoom captures, and interactive spring-camera / pointer / finisher / audio / export labs. Keep native pointer and native scrolling.
