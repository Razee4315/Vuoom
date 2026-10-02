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
On a first visit, follow the operating system's light or dark preference. An explicit saved choice overrides it until cleared; system changes update the page when there is no saved choice. The evening palette uses midnight blue, deep teal and warm coral. Apply the theme before paint; storage failure must not break the control. The floating navigation has a persistent download action, theme control and keyboard-accessible mobile menu. Custom inline SVG viewfinder, sparkle, orbit, sun and moon marks use currentColor. FAQ disclosure keeps native details semantics with interruptible 260ms height and answer-opacity animation. This bounded disclosure-height animation is an exception to the transform/opacity-only rule, justified by keeping adjacent questions in place. Reduced motion disables transitions. Other effects use transform/opacity and finite durations.

The hero eyebrow names the product and its auto-zoom feature. The workflow section explains recording, editing and export. The handwritten note over the demo is removed.

## Centered hero and dedicated demo, 2026-09-30

The homepage opens with centered category, headline, description and download actions.
Small recording motifs float around the copy, inspired by the owner's Paperling
hero accents and Snipflag's compact, centered introduction. They respond gently to
the pointer; tapping or keyboard-activating a mark sends it on a short flight.
The hero copy subtly shrinks as it scrolls away. All movement stops under reduced
motion, leaving static decoration without irrelevant keyboard stops. Use native
scrolling; keep the motifs clear of the headline and actions. On phones, four small
marks sit below the actions rather than alongside the text.
Refinement: draw the marks as small two-tone recording objects with printed offset
edges, rather than uniform outline icons. A finite staggered drift introduces them.
Proximity to the pointer gently pushes individual objects aside. A tap follows a
short curved flight; a bounded drag returns through the existing critically damped
spring implementation. Keep touch scrolling, keyboard activation, and visible focus.
Run the motion loop only while visible and moving; stop it when hidden or settled.
Real demo footage moves into the second section, immediately after the hero, with
its own centered heading and the existing playback controls. The workflow, comparison,
toolkit and supporting sections follow. Both hero demo links target that section.
Keep the paper palette, responsive layouts and system/saved theme behavior.

## Compact navigation on scroll

After scrolling 48px, the floating navigation narrows from 1120px to 920px and
shortens to 56px. Its brand mark and wordmark become smaller while the controls
retain their existing sizes. Returning to within 16px of the top restores the
full bar; the separate thresholds avoid flickering near the switch point.
Use a short interruptible transition on the fixed header's width, height and
brand dimensions. This bounded layout transition affects only the fixed navigation,
not the page flow. Reduced motion applies the compact state immediately.

## Responsive full-screen hero

The owner now requests a hero that fills the opening screen. Use a minimum
height of 100svh with a 100vh fallback, rather than a fixed content-sized panel.
Center the category and copy together in the space below the floating navigation.
Keep fluid headline sizing and generous, bounded spacing. On phones, reserve a
separate lower band for the four interactive objects. On short or landscape
screens, tighten the spacing and let the section grow with its content so the
headline and actions stay readable. Never clip the content to a fixed height.
The existing scroll shrink remains; the demo follows immediately after the hero.

### Extended verification
Final production build passes for all 24 content pages. Updated static audit checks 1,126 local references, all resolving, and JSON-LD parses. Browser checks confirm light/dark switching, dark-mode persistence across navigation, FAQ expansion and repeated open/close, mobile menu and Escape, and no horizontal overflow at 390px and 320px. Theme crossfade has a native API fallback and reduced-motion opt-out. This revision is authorized for main and GitHub Pages deployment.
