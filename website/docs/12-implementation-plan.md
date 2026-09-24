# 12, Implementation plan

| Phase | Includes | Done when |
|---|---|---|
| 1 Foundation | Astro project in `website/`, tokens.css, fonts, reset, layout, SEO head component, JSON-LD helpers, SVG logo, icons, Pages workflow | `pnpm build` in `website/` emits `dist/` with `/`, sitemap, robots; head validates |
| 2 Static skeleton | Every route with real copy, correct type and spacing, zero animation | All 24 routes render; frozen screenshots reviewed at 1440 and 390 wide |
| 3 Scroll layer | Lenis + GSAP ticker, reveal utility, reduced-motion gate | Reveals run; no console errors; OS reduced motion checked |
| 4 Type motion | SplitText on h1/h2 after fonts | Line breaks correct at 3 widths |
| 5 Signature | Stage live take (M10), The Problem (M07), Editor pin (M09), before/after (M08) | Observed running; pause works; static under reduced motion |
| 6 Micro | Buttons, links, keycaps, pointer, nav compaction, mobile menu | Keyboard pass complete |
| 7 Polish | Countdown, curtain footer, OG images, 404, changelog data | Lighthouse targets met |
| 8 Audit | Phase 6 audit | Report delivered |
| 9 Repo docs | CHANGELOG.md, PRIVACY.md, SUPPORT.md, SECURITY.md fix, README website link, funding stub | Files committed |

Most likely to blow the estimate: pinned sections across breakpoints, font fallback metrics
(CLS), OG image generation on Windows.
