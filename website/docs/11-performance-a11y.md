# 11, Performance and accessibility

## Budgets

| Metric | Target | Alert |
|---|---|---|
| LCP (mobile, Slow 4G, mid-range) | < 2.0 s | 1.8 s |
| INP | < 200 ms | 160 ms |
| CLS | < 0.05 | 0.03 |
| Home transfer (first view) | < 700 KB | 600 KB |
| Inner page transfer | < 300 KB | 250 KB |
| JS home / inner (gz) | < 110 KB / < 45 KB | |
| Largest image | < 140 KB | |

Lab-measured with Lighthouse at launch; real-user numbers are not collected (no analytics), so
Search Console's Core Web Vitals report is the field source once traffic exists.

## Contrast (on `--ground #0B0B0C`)

| Pair | WCAG | Use allowed |
|---|---|---|
| `--ink` #F2F0EB | 17.4:1 | all |
| `--ink-2` (70%) | ≈ 9.6:1 | all |
| `--ink-3` (52%) | ≈ 5.3:1 | all text |
| `--rec` #E5484D | 4.2:1 | large text ≥ 24px, UI, focus ring (≥ 3:1) |
| `--rec-ink` #FF7A7E | ≈ 6.4:1 | small red text |
| `--on-rec` #FFF on `--rec` | 4.7:1 | button labels |

Values are verified with a script in the build phase and reported at audit.

## Accessibility (WCAG 2.2 AA)

- Landmarks: `header`, `nav[aria-label]`, `main#content`, `footer`; skip link
- One h1 per page, no skipped levels
- Split text: `aria-label` on parent set before SplitText; child spans `aria-hidden`
- Stage animation > 5 s: visible pause button (2.2.2), paused state remembered for the session
- Custom pointer: decorative, `aria-hidden`, native cursor restored on keyboard use
- Before/after: `role="slider"` with `aria-valuenow`, keyboard operable
- Pinned sections: all text is in the DOM in reading order; nothing is only visible mid-scroll
- Reduced motion: Lenis off, no pins, no countdown, no loop, no parallax; reveals become 200ms opacity
- Target size ≥ 24×24 (2.5.8), primary buttons 48px
- Focus order equals visual order; mobile menu traps focus and restores it
