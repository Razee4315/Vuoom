// Site-wide constants and helpers. Every internal link goes through `url()` so the
// GitHub Pages base path (/Vuoom/) is respected, and a custom domain is a config change.

export const SITE = {
  name: 'Vuoom',
  tagline: 'Screen recordings that zoom where it matters.',
  description:
    'Free, open-source screen recorder for Windows. The camera zooms into every click, then export a small GIF or MP4. No account, no watermark.',
  repo: 'Razee4315/Vuoom',
  repoUrl: 'https://github.com/Razee4315/Vuoom',
  releasesUrl: 'https://github.com/Razee4315/Vuoom/releases',
  latestUrl: 'https://github.com/Razee4315/Vuoom/releases/latest',
  issuesUrl: 'https://github.com/Razee4315/Vuoom/issues',
  newIssueUrl: 'https://github.com/Razee4315/Vuoom/issues/new/choose',
  discussionsUrl: 'https://github.com/Razee4315/Vuoom/discussions',
  securityUrl: 'https://github.com/Razee4315/Vuoom/security/advisories/new',
  securityEmail: 'anisnur19315@gmail.com',
  author: 'Saqlain Razee',
  authorUrl: 'https://github.com/Razee4315',
  license: 'Apache-2.0',
  themeColor: '#0B0B0C',
  locale: 'en_US',
  gscVerification: import.meta.env.PUBLIC_GSC_VERIFICATION ?? '',
} as const;

const BASE = import.meta.env.BASE_URL.replace(/\/$/, '');

/** Internal path with the base prefix, always ending in a slash for pages. */
export function url(path = '/'): string {
  if (/^(https?:|mailto:|#)/.test(path)) return path;
  const clean = path.startsWith('/') ? path : `/${path}`;
  return `${BASE}${clean}`;
}

/** Absolute URL for canonical tags, sitemap and JSON-LD. */
export function absolute(path = '/'): string {
  return new URL(url(path), import.meta.env.SITE).toString();
}

export const NAV = [
  { href: '/features/', label: 'Features' },
  { href: '/compare/', label: 'Compare' },
  { href: '/guide/', label: 'Guide' },
  { href: '/changelog/', label: 'Changelog' },
] as const;

export const FOOTER = [
  {
    title: 'Product',
    links: [
      { href: '/features/', label: 'All features' },
      { href: '/features/auto-zoom/', label: 'Auto-zoom' },
      { href: '/features/gif-export/', label: 'GIF and MP4 export' },
      { href: '/features/captions/', label: 'Offline captions' },
      { href: '/features/editor/', label: 'Editor' },
      { href: '/features/audio-and-webcam/', label: 'Audio and webcam' },
      { href: '/download/', label: 'Download' },
      { href: '/changelog/', label: 'Changelog' },
    ],
  },
  {
    title: 'Compare',
    links: [
      { href: '/compare/', label: 'All comparisons' },
      { href: '/compare/screen-studio/', label: 'vs Screen Studio' },
      { href: '/compare/cap/', label: 'vs Cap' },
      { href: '/compare/obs/', label: 'vs OBS' },
      { href: '/compare/screentogif/', label: 'vs ScreenToGif' },
    ],
  },
  {
    title: 'Help',
    links: [
      { href: '/guide/', label: 'Guide' },
      { href: '/shortcuts/', label: 'Shortcuts' },
      { href: '/faq/', label: 'FAQ' },
      { href: SITE.newIssueUrl, label: 'Report a bug' },
      { href: SITE.discussionsUrl, label: 'Discussions' },
    ],
  },
  {
    title: 'Project',
    links: [
      { href: '/open-source/', label: 'Open source' },
      { href: SITE.repoUrl, label: 'GitHub' },
      { href: `${SITE.repoUrl}/blob/main/CONTRIBUTING.md`, label: 'Contributing' },
      { href: '/security/', label: 'Security' },
      { href: '/press/', label: 'Press kit' },
    ],
  },
  {
    title: 'Legal',
    links: [
      { href: '/privacy/', label: 'Privacy' },
      { href: '/terms/', label: 'Terms' },
      { href: '/licenses/', label: 'Licences' },
    ],
  },
] as const;
