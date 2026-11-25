// Build-time GitHub data: the latest release (for the download buttons) and release history.
// Falls back cleanly for offline builds; CI fails if GitHub is unreachable so deploys stay accurate.

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
  dmg?: Asset;
}

const FALLBACK: Release = {
  tag: 'v2.2.0',
  version: '2.2.0',
  name: 'Vuoom 2.2.0',
  date: '2026-11-23T18:00:00Z',
  body: '',
  url: SITE.latestUrl,
  exe: {
    name: 'Vuoom_2.2.0_x64-setup.exe',
    size: 8150000,
    url: `${SITE.releasesUrl}/download/v2.2.0/Vuoom_2.2.0_x64-setup.exe`,
  },
  msi: {
    name: 'Vuoom_2.2.0_x64_en-US.msi',
    size: 11100000,
    url: `${SITE.releasesUrl}/download/v2.2.0/Vuoom_2.2.0_x64_en-US.msi`,
  },
  dmg: {
    name: 'Vuoom_2.2.0_aarch64.dmg',
    size: 10900000,
    url: `${SITE.releasesUrl}/download/v2.2.0/Vuoom_2.2.0_aarch64.dmg`,
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
    dmg: find(/\.dmg$/i),
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
      if (!list.length) throw new Error('no published release');
      return list.map(toRelease);
    } catch (e) {
      const reason = (e as Error).message;
      if (process.env.CI) throw new Error(`[github] could not load the releases: ${reason}`);
      console.warn(`[github] using fallback release data: ${reason}`);
      return [FALLBACK];
    }
  })();
  return cache;
}

export async function latest(): Promise<Release> {
  return (await releases())[0] ?? FALLBACK;
}

/** "7.8 MB" style, the way file browsers round. */
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
