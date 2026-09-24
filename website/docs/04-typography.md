# 04, Typography

| Role | Face | Foundry | Licence | Package |
|---|---|---|---|---|
| Display | Bricolage Grotesque, variable (`opsz 12..96`, `wdth 75..100`, `wght 200..800`) | Mathieu Triay / Ateliertriay | SIL OFL 1.1, free for web | `@fontsource-variable/bricolage-grotesque` |
| Text | Geist, variable `wght 100..900` | Vercel + Basement.studio | SIL OFL 1.1 | `@fontsource-variable/geist` |
| Mono | Geist Mono, variable | Vercel + Basement.studio | SIL OFL 1.1 | `@fontsource-variable/geist-mono` |

All three are self-hosted from the build (no Google Fonts request, which is also a privacy
decision: no third-party request on page load). OFL notices are listed on `/licenses/`.

## Scale

| Token | Min → max | Use | Face / weight | Tracking | LH |
|---|---|---|---|---|---|
| `--step-6` | 56 → 192px | Home h1 on desktop, footer wordmark | Display 720, `wdth 75` | -0.045em | 0.9 |
| `--step-5` | 52 → 136px | Section statements ("1920 / 14") | Display 700, `wdth 75` | -0.045em | 0.9 |
| `--step-4` | 44 → 88px | Section h2, inner-page h1 | Display 680, `wdth 80` | -0.03em | 1.0 |
| `--step-3` | 36 → 60px | Feature row titles on hover, sub-page h2 | Display 640, `wdth 85` | -0.03em | 1.02 |
| `--step-2` | 26 → 36px | Lead paragraphs | Text 400 | -0.01em | 1.3 |
| `--step-1` | 20 → 24px | h3, card titles | Text 560 | -0.01em | 1.3 |
| `--step-0` | 16 → 17px | Body | Text 400 | 0 | 1.6 |
| `--step--1` | 13 → 14px | Slates, labels, keycaps, footer | Mono 500, uppercase for slates | +0.08em slates, 0 keycaps | 1.4 |

Weight contrast: display 720 against text 400 and mono 500. The hairline end (`wght 200`) of
Bricolage is used once, for the `/` separator in "1920 / 14".

## Rules

- Body measure `max-width: 62ch`
- `text-wrap: balance` on h1 to h3, `text-wrap: pretty` on paragraphs
- `font-variant-numeric: tabular-nums` on timecodes, sizes, counters
- Mono is for machine text only: timecodes, keycaps, file sizes, slates, code
- Uppercase only in mono slates

## Loading

- Preload the Latin subset woff2 of Bricolage (hero h1) and Geist (hero body). Mono is not preloaded
- `font-display: swap`; fallback metrics via `size-adjust` on local `Arial Narrow` (display) and `Segoe UI` (text), tuned during build to keep CLS < 0.05
- SplitText runs only after `document.fonts.ready`, then `ScrollTrigger.refresh()`
