// Build-time GitHub data: the latest release (for the download button) and the release
// history (for the changelog). Fetched once per build and cached in module scope. Every
// field has a fallback so an offline or rate-limited build still produces a working site.

import { SITE } from './site';

export interface Asset {
  name: string;
  size: number;
  url: string;
}

export interface Release {
  tag: string;
  version: string;
  name: string;
  date: string;
  body: string;
  url: string;
  exe?: Asset;
  msi?: Asset;
}

const FALLBACK: Release = {
  tag: 'v2.0.0',
  version: '2.0.0',
  name: 'Vuoom 2.0.0',
  date: '2026-09-24T18:37:21Z',
  body: '',
  url: SITE.latestUrl,
  exe: {
    name: 'Vuoom_2.0.0_x64-setup.exe',
    size: 7744679,
    url: `${SITE.releasesUrl}/download/v2.0.0/Vuoom_2.0.0_x64-setup.exe`,
  },
  msi: {
    name: 'Vuoom_2.0.0_x64_en-US.msi',
    size: 10715136,
    url: `${SITE.releasesUrl}/download/v2.0.0/Vuoom_2.0.0_x64_en-US.msi`,
  },
};

interface GhAsset {
  name: string;
  size: number;
  browser_download_url: string;
}
interface GhRelease {
  tag_name: string;
  name: string | null;
  published_at: string | null;
  body: string | null;
  html_url: string;
  draft: boolean;
  prerelease: boolean;
  assets: GhAsset[];
}

function toRelease(r: GhRelease): Release {
  const find = (re: RegExp): Asset | undefined => {
    const a = r.assets.find((x) => re.test(x.name));
    return a && { name: a.name, size: a.size, url: a.browser_download_url };
  };
  return {
    tag: r.tag_name,
    version: r.tag_name.replace(/^v/, ''),
    name: r.name || r.tag_name,
    date: r.published_at ?? '',
    body: r.body ?? '',
    url: r.html_url,
    exe: find(/setup\.exe$/i),
    msi: find(/\.msi$/i),
  };
}

let cache: Promise<Release[]> | undefined;

export function releases(): Promise<Release[]> {
  cache ??= (async () => {
    try {
      const headers: Record<string, string> = { Accept: 'application/vnd.github+json' };
      const token = process.env.GITHUB_TOKEN;
      if (token) headers.Authorization = `Bearer ${token}`;
      const res = await fetch(`https://api.github.com/repos/${SITE.repo}/releases?per_page=30`, {
        headers,
      });
      if (!res.ok) throw new Error(`GitHub ${res.status}`);
      const list = ((await res.json()) as GhRelease[]).filter((r) => !r.draft && !r.prerelease);
      return list.length ? list.map(toRelease) : [FALLBACK];
    } catch (e) {
      console.warn(`[github] using fallback release data: ${(e as Error).message}`);
      return [FALLBACK];
    }
  })();
  return cache;
}

export async function latest(): Promise<Release> {
  return (await releases())[0] ?? FALLBACK;
}

/** "7.4 MB" style, the way Windows Explorer rounds. */
export function mb(bytes: number | undefined): string {
  if (!bytes) return '';
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function humanDate(iso: string): string {
  if (!iso) return '';
  return new Date(iso).toLocaleDateString('en-GB', {
    day: 'numeric',
    month: 'long',
    year: 'numeric',
    timeZone: 'UTC',
  });
}
