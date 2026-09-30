# Homepage design audit, 2026-09-30

Scope: the homepage introduction, demo placement, and the owner's interaction refinements.

| Finding | User impact | Resolution |
| --- | --- | --- |
| Split hero put the headline and product footage at equal visual weight. | The introduction had no clear center of attention. | Centered the category, headline, description, and actions; moved the existing demo to section two. |
| Product category appeared twice above the actions. | More reading before the visitor reaches the download. | Kept one category label and shortened the description. |
| First interaction used a large viewfinder with paper tiles. | The owner found it distracting and wanted smaller flying elements. | Removed it completely. Added six small recording motifs around the desktop copy and four beneath the actions on phones. |
| Initial phone headline wrapped onto three lines. | The introduction pushed the demo too far down. | Added a phone type scale and compact spacing. |

References inspected: [Paperling](https://razee4315.github.io/Paperling/) for small
floating accents, and [Snipflag](https://razee4315.github.io/snipflag/) for centered
copy, a short description, and a direct path to the demo. Kept Vuoom's paper colors,
real footage, download routing, and system/saved theme behavior.

## Result and verification

- Production build succeeds for all 24 pages.
- At 1440px, the hero is approximately 599px tall. At 390px, it is approximately
  626px tall. The real demo follows the hero before the workflow.
- Browser checks at 1440px, 390px, and 320px show no horizontal page overflow.
  The four visible phone motifs have 44px targets and stay within the page width.
- Enter activates a motif's short flight. The resulting transform was observed
  in the browser, and the flight returns to its resting position.
- Scrolling to 500px changes the hero copy wrapper to approximately 0.9165 scale,
  confirming that the scroll shrink runs. Scrolling remains native.
- The demo anchor reaches the dedicated section and the existing footage plays
  when visible. Light and dark palettes were visually checked.
- Reduced motion is handled through GSAP matchMedia: gestures and scroll shrink
  are absent, with static disabled decorative buttons. This preference was
  reviewed in code; it was not separately emulated in the browser.
- Motifs are static and disabled until their interaction initializes, so script
  failure leaves readable content and functioning ordinary links.

This is a focused design review, not a Lighthouse or full-site accessibility audit.

## Floating object refinement

Replaced the outline marks with bespoke colored SVG objects, printed offset edges,
and distinct resting angles. The animation uses the existing critically damped
spring math for a finite introduction, local pointer proximity, curved tap flights,
and bounded dragging. Its frame loop stops when the objects settle or the hero
leaves view. Vertical touch gestures retain `touch-action: pan-y`.

Verification: the 24-page production build and a TypeScript check of the animation
module pass. Browser checks confirm Enter launches a flight with visible focus;
dragging releases pointer capture and returns to within 0.003px of rest; and both
390px and 320px layouts have four visible objects with no horizontal page overflow.
The colored art was visually checked in both page themes. Reduced-motion behavior
remains reviewed in code rather than separately emulated.

## Compact navigation on scroll

The fixed navigation smoothly changes from 1120px wide and 68px high to 920px
wide and 56px high after scrolling 48px. Its logo and wordmark also shrink;
the download and utility controls retain their usable sizes. Returning within
16px of the top restores the full navigation, avoiding flicker at the threshold.

Verification: desktop measurements confirm both sizes and restoration at the
top. At 320px, the compact bar is 56px high without horizontal page overflow;
the mobile menu opens and closes with Escape. The production build passes.
Reduced-motion transition removal was reviewed in code.

## Responsive full-screen hero

The hero now uses a viewport minimum height and vertically centered content.
Browser measurements confirm a 720px hero at 1280x720, 900px at 1440x900,
1024px at 768x1024, and 844px at 390x844. The demo starts directly after it.
At 320x568 and landscape 844x390, the section grows naturally to keep the
content readable. All six sizes have no horizontal page overflow. Phone
objects remain below the copy, and landscape content clears the navigation.
The demo anchor still reaches the second section with the compact navbar.
