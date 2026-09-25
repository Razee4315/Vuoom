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
