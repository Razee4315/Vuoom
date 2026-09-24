// Every route's SEO metadata, in one place (docs/10-content.md "Meta per page").
// Titles stay under 60 characters and descriptions between 140 and 160 where possible.
// `og` is the short headline drawn on that page's social card.

export interface PageMeta {
  title: string;
  description: string;
  og: string;
  kicker: string;
}

export const PAGES = {
  '/': {
    title: 'Vuoom: free auto-zoom screen recorder for Windows',
    description:
      'Free, open-source screen recorder for Windows. The camera zooms into every click, then export a small GIF or MP4. No account, no watermark.',
    og: 'Screen recordings that zoom where it matters.',
    kicker: 'Free for Windows 10 and 11',
  },
  '/download/': {
    title: 'Download Vuoom for Windows 10 and 11 (free)',
    description:
      'Download the free Vuoom screen recorder for Windows. A small installer, no account, no watermark, open source under Apache-2.0.',
    og: 'Download Vuoom for Windows.',
    kicker: 'Download',
  },
  '/features/': {
    title: 'Vuoom features: auto-zoom, captions, GIF and MP4',
    description:
      'Everything in Vuoom: auto-zoom on click, a smooth pointer, offline captions, webcam, narration, annotations, cuts and small GIF or MP4 export.',
    og: 'Everything in the box.',
    kicker: 'Features',
  },
  '/features/auto-zoom/': {
    title: 'Auto-zoom screen recorder for Windows, free',
    description:
      'Vuoom zooms your screen recording into each click with smooth spring motion. Press Ctrl+Shift+Z or let auto zooms plan it. Free and open source.',
    og: 'Auto-zoom that follows your clicks.',
    kicker: 'Feature',
  },
  '/features/gif-export/': {
    title: 'Record your screen as a GIF for GitHub and Slack',
    description:
      'Turn a screen recording into a small, crisp GIF or MP4 with a live size estimate and a fit-under-10-MB helper. Free Windows app, no watermark.',
    og: 'GIF or MP4, small on purpose.',
    kicker: 'Feature',
  },
  '/features/captions/': {
    title: 'Offline auto captions for screen recordings',
    description:
      'Vuoom turns your narration into captions on your own PC with whisper.cpp. Nothing is uploaded. Burn them into a GIF or MP4, or save an .srt file.',
    og: 'Captions, made on your PC.',
    kicker: 'Feature',
  },
  '/features/editor/': {
    title: 'Screen recording editor: cut, speed up, annotate',
    description:
      'Trim, cut, skim idle parts, add text, arrows, a pen, a spotlight and blur masks on one timeline. A free screen recording editor for Windows.',
    og: 'A real editor, not a video suite.',
    kicker: 'Feature',
  },
  '/features/audio-and-webcam/': {
    title: 'Record screen, webcam and microphone on Windows',
    description:
      'Record narration, system sound and a webcam bubble with your screen. One switch removes fan hum and evens out your voice. Free and offline.',
    og: 'Your voice, your face, your screen.',
    kicker: 'Feature',
  },
  '/compare/': {
    title: 'Vuoom vs Screen Studio, Cap, OBS and ScreenToGif',
    description:
      'An honest comparison of screen recorders for product demos: platform, price, licence, auto-zoom, GIF export and captions. Checked September 2026.',
    og: 'How it stacks up.',
    kicker: 'Compare',
  },
  '/compare/screen-studio/': {
    title: 'Screen Studio alternative for Windows, free',
    description:
      'Screen Studio is macOS only. Vuoom brings auto-zoom screen recording to Windows 10 and 11, free and open source. See how the two compare.',
    og: 'The Screen Studio alternative for Windows.',
    kicker: 'Vuoom vs Screen Studio',
  },
  '/compare/cap/': {
    title: 'Vuoom vs Cap: open-source screen recorders compared',
    description:
      'Cap and Vuoom are both open-source screen recorders with auto-zoom. How they differ in platform, licence, sharing, captions and export.',
    og: 'Vuoom vs Cap.',
    kicker: 'Compare',
  },
  '/compare/obs/': {
    title: 'OBS alternative for product demos and GIFs',
    description:
      'OBS is built for streaming. Vuoom is built for short demos that zoom into clicks and export a small GIF or MP4. When to use which one.',
    og: 'Vuoom vs OBS for demos.',
    kicker: 'Compare',
  },
  '/compare/screentogif/': {
    title: 'ScreenToGif alternative with auto-zoom',
    description:
      'Like ScreenToGif, Vuoom is free and open source on Windows. It adds auto-zoom, a smooth pointer, offline captions and MP4 export.',
    og: 'ScreenToGif, plus a camera.',
    kicker: 'Compare',
  },
  '/guide/': {
    title: 'How to use Vuoom: record, zoom, edit, export',
    description:
      'A step by step guide to recording your screen with Vuoom: install, first take, zooms, editing, captions, export and troubleshooting.',
    og: 'Your first take, step by step.',
    kicker: 'Guide',
  },
  '/shortcuts/': {
    title: 'Vuoom keyboard shortcuts',
    description:
      'Every Vuoom keyboard shortcut: start and stop recording, zoom while recording, trim, cut, annotate, arrange the layout and export.',
    og: 'Every shortcut.',
    kicker: 'Guide',
  },
  '/faq/': {
    title: 'Vuoom FAQ: is it free, safe and private?',
    description:
      'Answers about Vuoom: is it free, is it safe, why SmartScreen warns, what data it collects (none), Mac and Linux support and more.',
    og: 'Questions, answered.',
    kicker: 'FAQ',
  },
  '/changelog/': {
    title: 'Vuoom changelog and release notes',
    description:
      'What changed in each Vuoom release, newest first, pulled straight from GitHub Releases. Updates install themselves from inside the app.',
    og: 'What changed.',
    kicker: 'Changelog',
  },
  '/open-source/': {
    title: 'Vuoom is open source (Apache-2.0)',
    description:
      'How Vuoom is built with Rust, Tauri and wgpu, how to build it from source, and how to contribute code, bug reports and ideas.',
    og: 'Built in the open.',
    kicker: 'Open source',
  },
  '/privacy/': {
    title: 'Privacy policy',
    description:
      'Vuoom collects no personal data. Recordings stay on your computer. This website sets no cookies and runs no analytics or trackers.',
    og: 'Your recordings stay yours.',
    kicker: 'Privacy',
  },
  '/terms/': {
    title: 'Terms of use',
    description:
      'The terms for using the Vuoom app and this website. Vuoom is free software under the Apache License 2.0, provided as is.',
    og: 'Terms of use.',
    kicker: 'Legal',
  },
  '/security/': {
    title: 'Security',
    description:
      'How to report a security issue in Vuoom privately, and exactly what the app does and does not do on your computer and network.',
    og: 'Security.',
    kicker: 'Security',
  },
  '/licenses/': {
    title: 'Third-party licences',
    description:
      'Open-source software and fonts used by the Vuoom app and this website, with their licences and links to the source.',
    og: 'Standing on open source.',
    kicker: 'Legal',
  },
  '/press/': {
    title: 'Press kit',
    description:
      'Vuoom logos, screenshots, colours and ready-to-use descriptions for articles, videos and listings. Free to use when writing about Vuoom.',
    og: 'Press kit.',
    kicker: 'Press',
  },
  '/404/': {
    title: 'Page not found',
    description: 'This page was cut from the final edit.',
    og: 'Cut.',
    kicker: '404',
  },
} as const satisfies Record<string, PageMeta>;

export type PagePath = keyof typeof PAGES;

export function ogSlug(path: string): string {
  const s = path.replace(/^\/|\/$/g, '').replace(/\//g, '-');
  return s || 'home';
}
