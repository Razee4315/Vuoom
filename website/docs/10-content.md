# 10, Content and SEO

## Voice

**Plain, precise, a little dry.** Talk like the person who built it, to a person who ships
things.

| Do | Don't |
|---|---|
| "Press Ctrl+Shift+Z and the camera glides to your pointer." | "Unlock cinematic storytelling with AI-powered zoom." |
| "8 MB installer. No account." | "Lightweight and blazing fast!" |

Rules: no em dashes (owner rule), no "AI" headlines, no exclamation marks, numbers as digits,
"pointer" not cursor, CTAs name the action and the objection ("Download for Windows, free").

## SEO strategy

Search intent clusters and the page that owns each (one owner per cluster, no cannibalising):

| Cluster (example queries) | Owner |
|---|---|
| vuoom, vuoom screen recorder, vuoom download | `/`, `/download/` |
| free screen recorder for windows with zoom, auto zoom screen recorder, screen recorder that zooms on click, cursor zoom screen recording windows | `/features/auto-zoom/` (home also targets the head term) |
| screen studio alternative windows, screen studio for windows, free screen studio alternative | `/compare/screen-studio/` |
| open source screen recorder windows, cap alternative, obs alternative for demos | `/compare/cap/`, `/compare/obs/`, `/open-source/` |
| screen to gif, gif screen recorder, record gif for github readme, screentogif alternative | `/features/gif-export/`, `/compare/screentogif/` |
| auto captions screen recording offline, subtitles for screen recording, whisper captions | `/features/captions/` |
| screen recording editor cut speed up annotate, blur screen recording, spotlight | `/features/editor/` |
| record screen with webcam and microphone windows, remove background noise screen recording | `/features/audio-and-webcam/` |
| how to use vuoom, vuoom shortcuts | `/guide/`, `/shortcuts/` |

Technical SEO (every page): unique `<title>` ≤ 60 chars and description 140 to 160 chars;
canonical; `og:*` + `twitter:card=summary_large_image` with a 1200×630 image; one h1; logical
h2/h3; descriptive alt; internal links from every feature and compare page to Download and to
two siblings; `BreadcrumbList`; site-wide `Organization` + `WebSite`; `SoftwareApplication` on
home, download and feature pages; `FAQPage` where there is a FAQ; `HowTo` on the guide.
`sitemap-index.xml`, `robots.txt` pointing at it, `llms.txt` (a plain summary for AI search
engines, which now drive a share of software discovery), `manifest.webmanifest`, favicons
(svg + 32 png + apple-touch 180), `theme-color #0B0B0C`, `lang="en"`.

Off-site (launch checklist, owner's hands): set the GitHub repo **Website** field to the site
URL; README badge + link; Product Hunt, Show HN, r/Windows, r/opensource, r/SideProject,
AlternativeTo (list as Screen Studio alternative), awesome-lists (awesome-tauri, awesome-rust
apps, awesome-windows), Search Console + Bing Webmaster submit sitemap. Backlinks decide
ranking more than anything on-page.

## Meta per page

| Route | Title | Description |
|---|---|---|
| `/` | Vuoom: free auto-zoom screen recorder for Windows | Free, open-source screen recorder for Windows. The camera zooms into every click, then export a small GIF or MP4. No account, no watermark. |
| `/download/` | Download Vuoom for Windows 10 and 11 (free) | Download the free Vuoom screen recorder for Windows. An 8 MB installer, no account, no watermark, open source under Apache-2.0. |
| `/features/` | Vuoom features: auto-zoom, captions, GIF and MP4 | Everything in Vuoom: auto-zoom on click, a smooth pointer, offline captions, webcam, narration, annotations, cuts and small GIF or MP4 export. |
| `/features/auto-zoom/` | Auto-zoom screen recorder for Windows, free | Vuoom zooms your screen recording into each click with smooth spring motion. Press Ctrl+Shift+Z or let auto zooms plan it. Free and open source. |
| `/features/gif-export/` | Record your screen as a GIF for GitHub and Slack | Turn a screen recording into a small, crisp GIF or MP4 with a live size estimate and a fit-under-10-MB helper. Free Windows app, no watermark. |
| `/features/captions/` | Offline auto captions for screen recordings | Vuoom turns your narration into captions on your own PC with whisper.cpp. Nothing is uploaded. Burn them into a GIF or MP4, or save an .srt file. |
| `/features/editor/` | Screen recording editor: cut, speed up, annotate | Trim, cut, skim idle parts, add text, arrows, a pen, a spotlight and blur masks on one timeline. A free screen recording editor for Windows. |
| `/features/audio-and-webcam/` | Record screen, webcam and microphone on Windows | Record narration, system sound and a webcam bubble with your screen. One switch removes fan hum and evens out your voice. Free and offline. |
| `/compare/` | Vuoom vs Screen Studio, Cap, OBS and ScreenToGif | An honest comparison of screen recorders for product demos: platform, price, licence, auto-zoom, GIF export and captions. Checked September 2026. |
| `/compare/screen-studio/` | Screen Studio alternative for Windows, free | Screen Studio is macOS only. Vuoom brings auto-zoom screen recording to Windows 10 and 11, free and open source. See how they compare. |
| `/compare/cap/` | Vuoom vs Cap: open-source screen recorders compared | Cap and Vuoom are both open-source screen recorders with auto-zoom. How they differ in platform, licence, sharing and export. |
| `/compare/obs/` | OBS alternative for product demos and GIFs | OBS is built for streaming. Vuoom is built for short demos that zoom into clicks and export a small GIF or MP4. When to use which. |
| `/compare/screentogif/` | ScreenToGif alternative with auto-zoom | Like ScreenToGif, Vuoom is free and open source on Windows. It adds auto-zoom, a smooth pointer, captions and MP4 export. |
| `/guide/` | How to use Vuoom: record, zoom, edit, export | A step by step guide to recording your screen with Vuoom: install, first take, zooms, editing, captions, export and troubleshooting. |
| `/shortcuts/` | Vuoom keyboard shortcuts | Every Vuoom keyboard shortcut: start and stop recording, zoom while recording, trim, cut, annotate and export. |
| `/faq/` | Vuoom FAQ: free, safe, private? | Answers about Vuoom: is it free, is it safe, why SmartScreen warns, what data it collects (none), Mac and Linux support and more. |
| `/changelog/` | Vuoom changelog and release notes | What changed in each Vuoom release, newest first. |
| `/open-source/` | Vuoom is open source (Apache-2.0) | How Vuoom is built with Rust, Tauri and wgpu, how to build it from source and how to contribute. |
| `/privacy/` | Privacy policy | Vuoom collects no personal data. Recordings stay on your computer. This website sets no cookies and runs no analytics. |
| `/terms/` | Terms of use | The terms for using the Vuoom app and website. Vuoom is free software under the Apache License 2.0. |
| `/security/` | Security | How to report a security issue in Vuoom and what the app does and does not do on your computer. |
| `/licenses/` | Third-party licences | Open-source software and fonts used by the Vuoom app and website, with their licences. |
| `/press/` | Press kit | Vuoom logos, screenshots, colours and descriptions for articles and videos. |

## Home copy

**Slate:** `SC 01 · TK 1 · REC ● 00:00:04:12`

**H1:** Screen recordings that zoom where it matters.

**Lead:** Vuoom is a free, open-source screen recorder for Windows. Your clicks steer the camera, so the thing you pressed is the thing people see.

**CTA:** `● Download for Windows` / `Star on GitHub`
**Meta line:** Windows 10 & 11 · 8 MB installer · No account · Apache-2.0

**Problem (h2, visually hidden label "The problem"):**
"Your screen is **1920** pixels wide." / "The button you clicked is **14**." /
"Shrink that to a 640 pixel GIF and nobody can read it. So you zoom by hand, frame by frame, or you don't." /
"Vuoom zooms for you. Every click, automatically."

**Before / after (h2):** Same take. Two recordings.

**How it works (h2):** Record. Zoom. Ship.
- 01 Record. Pick a region, a window or a whole screen. Talk if you like.
- 02 Zoom. Press Ctrl+Shift+Z while you record, or let Auto zooms plan it from your clicks.
- 03 Ship. Trim the fumbles, then export a GIF or MP4 and paste it anywhere.

**Editor (h2):** A real editor, not a video suite.
Captions: "Zoom where you mean it." / "Point at things." / "Skim the waiting." / "Cut the fumbles."

**Feature index (h2):** Everything in the box.
01 Auto-zoom on click · 02 A smooth pointer · 03 Narration and system sound · 04 You, in the corner · 05 Studio-clean voice · 06 Captions, made offline · 07 Draw on the video · 08 GIF or MP4, small · 09 Nothing lost in a crash
(details from README features, one to two sentences each)

**Export (h2):** Fits under 10 MB, on purpose.
"Pick GIF or MP4. See the file size before you export, or tell Vuoom the limit and it fits the file under it. Then copy, and paste into GitHub, Slack or a changelog."

**Principles (h2):** Yours. Local. Free.
"No account." Nothing to sign up for. Open it and record.
"No watermark." Your video, not an ad for us.
"No cloud." Recordings, captions and voice clean-up all run on your PC.

**Compare (h2):** How it stacks up.

**FAQ (h2):** Questions, answered.

**Final CTA:** "Your next demo is one click away." `● Download for Windows`

## Open Graph

One template, rendered at build: `--ground` background, the logo mark, page title in Bricolage
at 72px, a crop of the editor screenshot, the URL in mono. 1200×630 PNG per page.
