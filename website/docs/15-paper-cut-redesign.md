# Paper Cut redesign, 2026-09-26

The owner's request for a new creative direction supersedes the previous dark-only
Live Take direction, custom pointer, and cinematic scroll choreography.

## Direction

Warm paper (#f5f1e8), charcoal ink (#242720), vermilion (#c8412d), cobalt
(#2949d5), butter yellow (#f5dc82), and mint (#c7dccb). Bricolage Grotesque
headlines, Geist text, and monospace timecodes. Paper grain, torn section edges,
tape, offset printed borders, and editing timeline graphics provide texture.
Existing real footage and screenshots remain the product evidence; no new image assets.

## Interaction

Native cursor and scrolling. No preloader or pinned scroll sections. Short,
one-shot opacity/transform reveals are initialized in gsap.matchMedia and revert
under reduced motion. Text remains visible if scripts fail. A keyboard-operable
feature switcher updates the editor preview, and the existing before/after slider
and explicit video playback controls remain. Decorative timeline motion is finite.
Shared navigation, buttons, typography and footer carry the direction to all routes.

## Content and search

Lead with the free Windows screen recorder category, then explain record, edit,
export. Natural search themes: screen recorder without watermark, automatic zoom,
product demo videos, software tutorials, GIF/MP4 export, and local recording.
Keep unique metadata, canonical URLs, sitemap, robots, application schema and
visible FAQ schema. Never guarantee rankings or add keyword stuffing.
Reference: https://developers.google.com/search/docs/fundamentals/seo-starter-guide

## Verification
Production build: 24 pages. Static output audit: 1,150 local references resolve; one h1, description, canonical and parseable JSON-LD per content page. Browser checks: desktop, 390px and 320px widths; no horizontal overflow after correcting waveform flex sizing; workflow click and arrow-key navigation; before/after keyboard adjustment; mobile menu and Escape; FAQ expansion; demo pause; download and feature routes. No browser warning/error logs observed. Reduced-motion fallback implemented, but not separately emulated in the browser. Deployment is not performed by this redesign change.

## Studio navigation and evening palette
Light remains the first-visit default. An explicit saved choice enables a midnight-blue, deep-teal and warm-coral evening palette. Apply the saved theme before paint; storage failure must not break the control. The floating navigation has a persistent download action, theme control and keyboard-accessible mobile menu. Custom inline SVG viewfinder, sparkle, orbit, sun and moon marks use currentColor. FAQ disclosure keeps native details semantics with interruptible 260ms height and answer-opacity animation. This bounded disclosure-height animation is an exception to the transform/opacity-only rule, justified by keeping adjacent questions in place. Reduced motion disables transitions. Other effects use transform/opacity and finite durations.

### Extended verification
Final production build passes for all 24 content pages. Updated static audit checks 1,126 local references, all resolving, and JSON-LD parses. Browser checks confirm light/dark switching, dark-mode persistence across navigation, FAQ expansion and repeated open/close, mobile menu and Escape, and no horizontal overflow at 390px and 320px. Theme crossfade has a native API fallback and reduced-motion opt-out. This revision is authorized for main and GitHub Pages deployment.
