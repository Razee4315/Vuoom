# 01, Brief

## What it is

The public website for Vuoom, a free, open-source (Apache-2.0) screen recorder for Windows
whose camera zooms into what you click. It replaces the GitHub README as the first thing a
stranger sees. It has to make someone who searched "auto zoom screen recorder windows" or
"screen studio alternative" download the app within one screen of scrolling.

Archetype: SaaS and developer tools (`site-archetypes.md` §2), with a product-launch edge.
Tier: **Showpiece**, confirmed by the owner ("award winning", 2026-09-25).

## Conversion

**Primary:** Download the Windows installer.

Secondary, ranked:
1. Star the repo on GitHub
2. Read the guide (reduces SmartScreen and first-run drop-off)
3. Report a bug / contribute

## Audience

| Who | Knows | Device | Context |
|---|---|---|---|
| Indie developers and OSS maintainers making README GIFs | Screen Studio exists and is Mac-only / paid | Desktop Windows, 80% | Searching for an alternative, comparing 3 to 5 tabs |
| Product people, PMs, founders recording demos and changelogs | Loom, OBS | Desktop, some mobile via shared link | Want something that looks "pro" without editing |
| Teachers, tutorial makers, support teams | OBS, Snipping Tool video | Desktop | Want captions and readable UI |
| Link-followers from Reddit / X / Product Hunt on phones | Nothing | Mobile, 40% of launch traffic | Will not download on a phone; must be convinced to come back or star |

Mobile visitors cannot convert to the primary action (Windows only). Their conversion is
**Star on GitHub** or **copy the link**. The mobile layout says so honestly.

## Success criteria

- Lighthouse (mobile, simulated): Performance ≥ 90, Accessibility 100, Best Practices 100, SEO 100 on every page
- LCP < 2.5 s, CLS < 0.05, INP < 200 ms on a mid-range phone profile
- Every page has a unique title, description, canonical, Open Graph image and valid JSON-LD
- Indexed by Google within 2 weeks of launch (Search Console)
- Ranks on page 1 for "vuoom" immediately and targets page 1 for the long-tail phrases in `10-content.md` §SEO within 3 months (depends on backlinks, see launch checklist)

## Non-goals

- No accounts, forms that store data, newsletter, comments, or cookies
- No pricing page (it is free), no fake testimonials, no invented numbers
- No blog at launch (a blog without a writing schedule rots; changelog covers "news")
- No Mac or Linux messaging beyond one honest FAQ answer
- No CMS. Content is Markdown/Astro in the repo, edited by PR
- No localisation at launch (English only; `lang` attributes set so it can be added)

## Locked constraints

- Hosted on GitHub Pages at `https://razee4315.github.io/Vuoom/`, with the base path in one config value so a custom domain is a one-line change plus `public/CNAME`
- Brand: the existing logo (white V, red record dot on near-black), record red `#E5484D`, "no purple anywhere"
- No em dashes in user-facing copy (owner preference, `docs/AUDIT-AND-REDESIGN.md`)
- Terminology: "pointer", not cursor; "Camera" means the webcam; zooms are "zooms"
- Real product claims only: every feature named on the site exists in v2.0.0
- Analytics: none (owner delegated, 2026-09-25). A Google Search Console verification tag is supported by config and inert until set
