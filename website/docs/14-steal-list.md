# 14, Steal list

Phase 1 of the website build. Every design decision in `02-art-direction.md` traces to a
numbered move below or is labelled `original`.

Fetched live on 2026-09-25 in a browser, computed styles read from the DOM. FocuSee's
screenshot failed to render in the pane (page text and styles were read); teenage.engineering
exposes no headings, only palette and type were read.

## Sites examined

| # | Site | Stack | Type | Palette | Memorable thing |
|---|---|---|---|---|---|
| 1 | screen.studio | Next.js, Vercel, Plausible | System SF Pro, h1 32px / 650, no tracking | `#000` bg, `#FFF`, violet `#A091FA` / `#4D2FF5` | The product video *is* the hero: auto-zoom demonstrated, not described. H2s double as search phrases ("Automatic zoom for engaging screen recordings"). |
| 2 | cap.so | Next.js | Instrument Sans 57px / 400, -0.03em | `#F2F2F2` bg, `#111` ink, blue `#3D77C2` | Title tag carries the positioning keyword: "Free Screen Recorder & Open Source Loom Alternative". 14 H2s, each a keyword-bearing claim. Sky/cloud hero reads soft and generic. |
| 3 | linear.app | Next.js | Inter Variable 56px / 510, -0.022em | `#08090A` bg, `#F7F8F8`, grays `#8A8F98` `#62666D` | Weight 510, not 500 or 600: a precise, chosen value. Near-black that is not black. Product UI rendered in HTML, crisp at any zoom. |
| 4 | raycast.com | Next.js | Inter 64px / 600 | `#07080A` bg, white at 100 / 60 / 40 / 5% | Opacity ladder instead of named grays: one white, four alphas. Keyboard shortcuts shown as real keycaps. |
| 5 | focusee.imobie.com | WordPress-style | Serif fallback h1 32px | not read | Direct Windows competitor. "AI" in every H2, feature-list page. The thing to be the opposite of: busy, claim-heavy. |
| 6 | teenage.engineering (adjacent: hardware) | custom | proprietary `te-20` grotesk, 16px | `#FFF`, `#000`, `#0F0E12`, one blue `#0071BB` | Instrument-panel labelling: small mono-ish captions, numbered parts, a single signal color. Product as object. |
| 7 | apple.com product pages (adjacent: consumer hardware, from known technique, not re-fetched) | custom | SF Pro | per product | Pinned, scroll-scrubbed product storytelling: the object stays, the story moves past it. |

## Moves to steal

| # | Move | From | Category | How it adapts here |
|---|---|---|---|---|
| 1 | Demonstrate the core feature in the hero instead of describing it | 1 | motion | The hero *is* a Vuoom recording: a real Vuoom screenshot on a stage, and as the page loads the "camera" glides into the clicked button, spring-eased, with a click ripple. The visitor experiences auto-zoom before reading about it. |
| 2 | H2s written as search phrases | 1, 2 | content / SEO | Every H2 names a query people type: "Auto-zoom screen recorder for Windows", "Free Screen Studio alternative", "Offline captions". Headline stays human, H2 carries the keyword. |
| 3 | Positioning keyword in the `<title>` | 2 | SEO | "Vuoom, free auto-zoom screen recorder for Windows (open source)". Title under 60 chars per page. |
| 4 | Near-black that is not black, with a precise weight | 3 | colour / type | `#0B0B0C` ground, which is the app's own window colour; display weight chosen off the 100-grid. |
| 5 | Opacity ladder instead of named grays | 4 | colour | One off-white at 100 / 64 / 42 / 8%. Keeps the palette to three colours. |
| 6 | Real keycaps for shortcuts | 4 | interaction | `Ctrl` `Shift` `Z` rendered as physical keys that depress when the matching zoom fires in the demo. Pressing the real keys on the page triggers it too. |
| 7 | Instrument-panel labelling | 6 | layout / type | Every section carries a mono slate like a film clapper: `SC 03 / TK 1 / 00:00:12:04`. It is also the section index for screen readers. |
| 8 | Single signal colour against monochrome | 6 | colour | The record red `#E5484D` from the logo is the only colour on the site. It means "live", "click", "record". Never decorative. |
| 9 | Pinned object, scrubbed story | 7 | motion | The editor screenshot pins; as you scroll, the timeline under it fills with zoom, cut and speed blocks, and the frame above responds. Scroll *is* editing. |
| 10 | Product UI in HTML rather than a flat image | 3 | technique | The editor timeline and export card are rebuilt in HTML/CSS so they stay crisp under the zoom and can animate part by part. Screenshots are used where fidelity matters. |

## Table stakes

What every competitor does. Must have, earns no credit.

- Download button in the nav, visible at all times
- Product visual above the fold
- Feature sections, each with a visual
- Pricing / "free" statement, FAQ, footer with legal links
- Organization + SoftwareApplication structured data (Screen Studio ships Organization JSON-LD)

## Deliberate deviations

- **No violet, no sky gradients.** Screen Studio is violet, Cap is sky blue. Vuoom is black, white and one red dot, which is the logo.
- **No "AI" in headlines.** FocuSee leads every section with it. Vuoom's captions are local whisper.cpp; say "offline" and "on your computer" instead.
- **Show the before.** No competitor shows the flat, unreadable recording. We open on the problem.
- **No testimonials or download counters we cannot verify.** Real GitHub numbers only, fetched live, or nothing.

## Rejected

| Move | From | Why rejected |
|---|---|---|
| Autoplaying hero video | 1 | No Vuoom demo video exists yet, and a 10 MB MP4 hurts LCP on GitHub Pages. Recreate the zoom with the real screenshot and transforms instead; add a video later if the owner records one. |
| Soft illustrated backdrop (clouds) | 2 | Generic, unrelated to recording. |
| Cookie banner | 1 | We set no cookies. No banner needed, which is itself a privacy statement. |
| Pricing table | 1, 2 | Vuoom is free. A single line says so. |
