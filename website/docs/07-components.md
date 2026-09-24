# 07, Components

Every component: tokens only, `:focus-visible` = 2px `--rec` outline, 3px offset (3:1 against
both `--ground` and any surface). Hover styles live under `@media (hover:hover)`; touch gets the
`:active` state instead.

| Component | Anatomy | Default | Hover | Focus-visible | Active | Disabled / loading |
|---|---|---|---|---|---|---|
| **Button primary** (Download) | record light (8px `--rec` dot on white ring) + label | `--rec` fill, `--on-rec` label, 2px radius, 48px tall (44 min on touch), Geist 560 | label rolls up, duplicate rolls in (M03); light brightens | ring | M04 press | `aria-disabled`, 50% ink; loading: light blinks, label "Getting latest…" |
| **Button secondary** (Star, Guide) | optional icon + label | transparent, `--rule` inset 1px, `--ink` label | background `--ink` 6%, label roll | ring | press | same |
| **Text link** | inline | `--ink`, underline via `::after` at 0 scale | M05 wipe in from left | ring + underline shown | none | n/a |
| **Nav link** | label | `--ink-2` | `--ink` + wipe | ring | none | current page: `aria-current="page"`, `--ink`, small red dot before |
| **Keycap** | mono label in a 2px-radius cap with 1px `--rule` + 2px bottom inner shadow | static | none | n/a (not interactive) | M17 press when active | n/a |
| **Slate** | mono uppercase items separated by `·`, optional blinking REC light | `--ink-3` | n/a | n/a | n/a | n/a |
| **Stage** | viewfinder brackets, 16:10 well, `.cam`, pointer, ripple | as 05 M10 | none | stage is `role="img"` with `aria-label` describing the demo; play/pause toggle button (WCAG 2.2.2) bottom right | | reduced motion: static |
| **Index row** | number, title, one-line, arrow, expandable media | hairline top, title `--step-3` | row reveals media + detail (`grid-template-rows 0fr → 1fr`), arrow slides 4px | whole row is a link, ring | press | always open on touch |
| **Before/after** | two images, clip by `--split`, handle | 50% | handle grows | `role="slider"`, arrows ±5%, Home/End | drag | no JS: side by side |
| **Compare table** | `<table>` with caption, sticky first column | `--rule` row hairlines | row `--ink` 4% | n/a | n/a | "Checked" stamp in caption |
| **FAQ item** | `<details><summary>` | `+` icon | `+` turns 45° | ring on summary | | native |
| **Code block** | mono, `--ground-raised`, copy button | | | ring | copy → "Copied" 1.5s | |
| **Nav (header)** | as 06 | | | | | |
| **Mobile menu** | full-height sheet, links step-3, Download at bottom | closed | | focus trapped, Esc closes, focus returns to trigger, `aria-expanded` | | |
| **Footer** | as 06 | | | | | |
| **Toast** | not used (no async actions besides copy) | | | | | |
| **Forms** | none on the site (non-goal) | | | | | |
| **Smooth pointer** | 22px arrow SVG + press ring | follows lerped | grows 1.2 over links/buttons | hidden when keyboard navigating | ring `--rec` on mousedown | off on touch, text inputs, reduced motion |
| **Countdown** | full-screen `--ground` layer, numerals step-6 mono, REC light | 3,2,1 | | Esc / any key / click skips | | never for reduced motion or repeat visit in the session |
