# 13, Decision log

## ADR-001, Direction: Live Take
2026-09-25, accepted by owner. The page behaves like a Vuoom recording. Alternatives: Cutting
Room (editorial light, timeline playhead) and Flat vs Vuoom (Swiss, before/after centerpiece).
The before/after wipe is kept as one section.

## ADR-002, Tier: Showpiece
2026-09-25. Owner asked for "award winning". 13-item motion floor.

## ADR-003, Hosting at razee4315.github.io/Vuoom
2026-09-25, owner. `site` and `base` in `astro.config.mjs` are the only places to change for a
custom domain, plus `public/CNAME`.

## ADR-004, No analytics
2026-09-25, owner delegated ("whatever is best"). No cookies, no banner, the privacy page is
one honest paragraph for the website. Search Console verification supported via
`PUBLIC_GSC_VERIFICATION` env var, empty by default.

## ADR-005, Screenshots now, video slots ready
2026-09-25, owner has clips and a launch video in production. Stage loop built from the real
screenshot; `media.ts` switches slots to video when files are present.

## ADR-006, Dark only
The product is dark-first; one mode executed well. `color-scheme: dark` set.

## ADR-007, Fonts: Bricolage Grotesque + Geist + Geist Mono
All OFL, self-hosted. Rejected: Instrument Sans (Cap uses it), Inter (unchosen default),
licensed faces (no budget stated; OFL keeps the repo redistributable).

## ADR-008, Astro static
See 09. Rejected hand-written HTML and Next.js.

## ADR-009, Site lives in `website/`, app docs untouched
The app already owns `docs/00..18`. Website design docs live in `website/docs/`.

## ADR-010, Spring maths ported from the app
`spring_update` and `clamp_camera` from `crates/vuoom-zoom/src/camera.rs` are ported to JS so
the site's camera moves exactly like Vuoom's.

## ADR-011, SECURITY.md network statement corrected
The app does make two outbound requests: the update check to GitHub Releases on launch, and a
one-time captions model download from Hugging Face when the user asks for captions. The
privacy policy and SECURITY.md state both.

## ADR-012, Owner clips used
2026-09-25. demo, github and voice clips re-encoded into `public/media`. demo.mp4 gets its own
home section, "A real take", right after the before/after, as proof that the stage animation
is what the app actually produces. x_post.mp4 rejected: third-party people and images.

## ADR-013, Astro 7 instead of 5
The current Astro major at build time is 7.3. Same reasoning as ADR-008.

## ADR-014, Minimal redesign after owner review
2026-09-25. Owner feedback on the first build: too many words, congested, nav not modern.
Changes: floating pill nav (logo, 3 links, Download); centred hero with one line and two
buttons; all slates, kickers and meta lines removed; home cut from 12 sections to 8
(hero, problem, flat vs Vuoom, real take, editor, features, FAQ, end); problem shows one short
line per step; editor captions became four word chips; features became a 6-cell grid of a
title plus a 3 to 6 word line; section spacing raised to clamp(7rem, 14vw, 14rem); display type
relaxed to weight 600, width 92%; buttons are pills. Countdown preloader and keycap hints
dropped from the home page. Supersedes the density and motion-inventory choices in 02 and 05
where they conflict.

## ADR-015, Launch film and the inner pages
2026-09-25. The owner's 1:57 launch film (re-encoded to 1080p30, 10.7 MB, AAC) sits under the
hero as "See it in two minutes." and on the Download page. It never autoplays: poster and one
play button, sound on, native controls after start. All 23 routes are built in the minimal
style: PageHead (one headline, one line), narrow columns, hairline lists. Per-page share cards
are rendered at build with satori. llms.txt summarises the site for answer engines.

## ADR-016, One typeface: Geist
2026-09-25. Owner found Bricolage Grotesque condensed congested and off-brand. Headings now use
Geist (the text face) at weight 540 to 580, tracking -0.028em to -0.035em, line-height 1.04 to
1.1, with smaller heading steps. Bricolage is removed, one fewer font download. The footer
wordmark moves to the very bottom and is cropped by the page edge. Supersedes ADR-007.

## ADR-017, The film is the hero
2026-09-25. Owner asked for the launch film in place of the animated stage under the hero
buttons. The film now sits there with a custom player (big play button on the open right half
of the poster, a pill control bar with scrubber, time, mute and fullscreen that hides while
playing, keyboard support). Poster is the "it zooms where you point." frame at 0:38. The stage
animation moves to /features/auto-zoom/, where it demonstrates exactly that feature.

## 2026-09-26: Paper Cut redesign
Owner requested replacement of the existing design, cursor and animation. Adopt docs/15-paper-cut-redesign.md as current direction, superseding dark-only and custom-pointer rules. Retain genuine product assets, accessible controls, truthful claims and base-path-safe links.

## 2026-09-26: Navigation, themes and microinteractions
Owner approved paper palette and requested modern navigation, optional dark theme, custom SVG art, FAQ motion, and deployment to main. See docs/15-paper-cut-redesign.md for the extended direction and the bounded FAQ height-animation exception.

## 2026-09-26: Product copy and system theme
The owner requested clearer hero labels, removal of the handwritten click note, and system-based initial theme. Saved explicit theme choices still take priority. See docs/15-paper-cut-redesign.md.

## 2026-09-30: Center the introduction and move the demo to section two

The owner requested centered hero text, a fun interactive element, and the demo
directly after the hero. Replace the split hero with a centered introduction and
an accessible paper viewfinder. Move the existing demo into a dedicated second
section, before the workflow. Reuse existing footage and keep the native pointer.
See docs/15-paper-cut-redesign.md for the updated composition and interaction.

### Owner refinement

The owner rejected the paper viewfinder and requested a smaller hero, flying
elements, and inspiration from Paperling and Snipflag. Replace the viewfinder with
small recording motifs around the centered copy and a subtle scroll shrink.
Use finite pointer and tap interactions, with a static reduced-motion fallback.
References inspected: https://razee4315.github.io/Paperling/ and
https://razee4315.github.io/snipflag/.

### Floating object refinement

The owner requested better icon art and animation. Use bespoke two-tone SVG
recording objects, varied resting angles, and the site's critically damped spring
math for finite drift, proximity response, tap flights and bounded dragging.
Keep the compact hero and disable decorative motion under reduced motion.

### Compact navigation

The owner requested a smaller navbar while scrolling. Add a narrower 920px,
56px-high scrolled state with a smaller brand, preserving navigation actions.
Use separate enter/exit thresholds and an interruptible fixed-header transition,
with no transition for reduced motion. See docs/15-paper-cut-redesign.md.

### Responsive full-screen hero

The owner requested that the hero cover the opening screen across screen sizes.
Replace content-only sizing with a small-viewport minimum height and centered
content. Reserve navigation and mobile decoration space, use fluid typography,
and allow natural growth on short screens. This supersedes the earlier compact
section height while preserving the small interactive objects and scroll shrink.
