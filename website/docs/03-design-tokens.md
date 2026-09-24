# 03, Design tokens

Source of truth: `website/src/styles/tokens.css`. This table is for humans. **No raw values in
components**; every colour, size, duration and curve resolves to a token.

## Colour (by role)

| Token | Value | Use |
|---|---|---|
| `--ground` | `#0B0B0C` | Page background, the app's window colour |
| `--ground-raised` | `#141415` | Stage well, code blocks, curtain footer |
| `--ink` | `#F2F0EB` | Primary text, icons |
| `--ink-2` | `rgb(242 240 235 / .70)` | Body copy, secondary text (≈ 9.6:1) |
| `--ink-3` | `rgb(242 240 235 / .52)` | Labels, captions, meta (≈ 5.3:1) |
| `--rule` | `rgb(242 240 235 / .12)` | Hairlines, keycap edges |
| `--rec` | `#E5484D` | Record light, click ripple, primary button, focus ring. Text only ≥ 24px or bold ≥ 18.66px (4.2:1) |
| `--rec-ink` | `#FF7A7E` | Red when it must be small text (6.4:1) |
| `--on-rec` | `#FFFFFF` | Text on a red fill (4.7:1, passes AA at any size) |
| `--grain` | 4% | Grain overlay opacity |

## Type

| Token | Value |
|---|---|
| `--font-display` | `'Bricolage Grotesque Variable', 'Arial Narrow', sans-serif` |
| `--font-text` | `'Geist Variable', ui-sans-serif, 'Segoe UI', sans-serif` |
| `--font-mono` | `'Geist Mono Variable', ui-monospace, 'Cascadia Mono', monospace` |
| `--step--1` | `clamp(0.8125rem, 0.79rem + 0.1vw, 0.875rem)` |
| `--step-0` | `clamp(1rem, 0.98rem + 0.1vw, 1.0625rem)` |
| `--step-1` | `clamp(1.25rem, 1.15rem + 0.5vw, 1.5rem)` |
| `--step-2` | `clamp(1.625rem, 1.4rem + 1.1vw, 2.25rem)` |
| `--step-3` | `clamp(2.25rem, 1.7rem + 2.6vw, 3.75rem)` |
| `--step-4` | `clamp(2.75rem, 1.8rem + 4.6vw, 5.5rem)` |
| `--step-5` | `clamp(3.25rem, 1.6rem + 8vw, 8.5rem)` |
| `--step-6` | `clamp(3.5rem, 0.8rem + 13vw, 12rem)` |
| `--track-display` | `-0.045em` (step 5 to 6) |
| `--track-heading` | `-0.03em` (step 3 to 4) |
| `--track-sub` | `-0.01em` (step 1 to 2) |
| `--track-label` | `0.08em` (mono uppercase labels) |
| `--lh-display` | `0.9` |
| `--lh-heading` | `1.02` |
| `--lh-body` | `1.6` |

## Space (8-based)

`--s-1 4px`, `--s-2 8px`, `--s-3 12px`, `--s-4 16px`, `--s-5 24px`, `--s-6 32px`, `--s-7 48px`,
`--s-8 64px`, `--s-9 96px`, `--section clamp(6rem, 12vw, 12rem)`.

## Form

| Token | Value |
|---|---|
| `--r-ui` | `2px` |
| `--r-frame` | `14px` (screenshots only) |
| `--r-pill` | `999px` (the record light and status dots only) |
| `--hair` | `1px` |
| `--shadow-stage` | `0 40px 120px -20px rgb(0 0 0 / .4)` (the only shadow) |

## Motion

| Token | Value |
|---|---|
| `--ease-out` | `cubic-bezier(0.16, 1, 0.3, 1)` (expo out, default) |
| `--ease-out-quart` | `cubic-bezier(0.25, 1, 0.5, 1)` (micro-interactions) |
| `--ease-in-out` | `cubic-bezier(0.77, 0, 0.175, 1)` (on-screen morphs) |
| `--ease-in` | `cubic-bezier(0.5, 0, 0.75, 0)` (exits) |
| `--dur-press` | `120ms` |
| `--dur-hover` | `240ms` |
| `--dur-reveal` | `1000ms` |
| `--dur-exit` | `700ms` |
| `--hl-zoom` / `--hl-pan` / `--hl-pointer` | `0.30s` / `0.22s` / `0.12s` (JS spring constants) |

## Layout

| Token | Value |
|---|---|
| `--max` | `1360px` |
| `--gutter` | `clamp(16px, 4vw, 48px)` |
| `--col-gap` | `24px` (16px < 640px) |
| Breakpoints | `640px`, `960px`, `1280px` |
| `--z-grain` 90, `--z-nav` 50, `--z-pointer` 100, `--z-countdown` 110 |
