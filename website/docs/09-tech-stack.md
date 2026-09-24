# 09, Tech stack

| Layer | Choice | Why here | Cost |
|---|---|---|---|
| Framework | **Astro 7**, static output | 24 routes of mostly content: ships zero JS by default, per-page `<head>` control for SEO, file routing, Markdown, image pipeline, build-time data fetch (GitHub releases). Output is plain HTML for GitHub Pages | One more toolchain in the repo (isolated in `website/`) |
| Styling | Hand-written CSS with custom properties (`tokens.css`) + scoped `<style>` in components | Tokens are the rule; no utility framework to fight the 10x scale | Discipline required |
| Animation | GSAP 3.13+ (core, ScrollTrigger, SplitText), Lenis 1.x, npm | Library floor for Showpiece; free licence incl. SplitText | ~95 KB gz on the home page only; inner pages load ScrollTrigger + reveals only |
| Fonts | `@fontsource-variable/*` self-hosted | No third-party request, subset Latin | ~120 KB woff2 total, 2 preloaded |
| Images | `astro:assets` (sharp) → AVIF/WebP, width/height always set | CLS and weight | Build time |
| Sitemap | `@astrojs/sitemap` | SEO | none |
| OG images | Build script: `satori` + `@resvg/resvg-js` → PNG per page | Unique social cards | ~5 s build |
| Data | GitHub REST at build (releases, stars), client refresh for version/stars | Always-current download link without a server | Unauthenticated rate limit 60/h per visitor IP; falls back gracefully |
| Hosting | GitHub Pages via Actions (`.github/workflows/pages.yml`, `actions/deploy-pages`), triggered on changes under `website/**` and on release publish | Free, owner's choice | Repo setting Pages → Source: GitHub Actions (one click, owner) |
| Analytics | None | Owner delegated; privacy page can say "none" | No traffic numbers; use Search Console + release downloads |
| CMS / forms / embeds | None | Non-goals | |

**Rejected: plain hand-written HTML files.** Zero toolchain, but 24 pages would duplicate the
head, nav and footer 24 times, and per-page OG images, sitemap and image conversion become
manual. Drift across copies is the failure mode.

**Rejected: Next.js static export.** Ships a React runtime to every page for no benefit here.
