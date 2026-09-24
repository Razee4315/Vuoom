# Vuoom website rules

## Stack
Astro 7 static, GSAP 3.13+ (ScrollTrigger, SplitText), Lenis 1, fonts via @fontsource-variable.
Deployed to GitHub Pages by `.github/workflows/pages.yml`. Base path: `/Vuoom/` (see astro.config.mjs).

## Docs
Art direction: docs/02-art-direction.md, the authority on visual questions
Tokens: docs/03-design-tokens.md + src/styles/tokens.css
Motion: docs/05-motion-spec.md · Pages: docs/06 · Copy + SEO: docs/10

## Hard rules
1. No raw values in components. Everything resolves to a token.
2. Animate transform and opacity only.
3. Every interactive element has a visible :focus-visible state.
4. Explicit width and height on every image and video.
5. Every animation has a reduced-motion fallback, gated with gsap.matchMedia().
6. SplitText only after `await document.fonts.ready`, aria-label on the parent.
7. ScrollTrigger.refresh() after fonts and images load.
8. No new image without a row in docs/08-assets.md.
9. Code diverging from a doc: update the doc, log an ADR, then code.
10. Never claim a test or audit passed without running it.
11. Internal links go through `url()` from src/lib/site.ts so the base path is respected.
12. Copy: no em dashes, "pointer" not cursor, no "AI" headlines, only features that ship.

## Forbidden
No purple (#6366F1, #8B5CF6 or any). No gradient text. No Inter. No three-card icon grids.
No 1px gray card borders. No soft drop shadows beyond --shadow-stage. No bounce or elastic.
No stock imagery. No lorem ipsum. No 8px radius. No emoji as icons. No light mode.

## Signature move
The page behaves like a Vuoom recording: a smooth red-dot pointer clicks inside a real
screenshot and the camera springs into the click, with the app's own spring maths.

## Current phase
See docs/12-implementation-plan.md.
