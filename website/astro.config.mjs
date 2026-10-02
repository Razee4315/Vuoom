// @ts-check
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'astro/config';
import sitemap from '@astrojs/sitemap';

const SITE = 'https://razee4315.github.io';
const BASE = '/Vuoom';
const ROOT = fileURLToPath(new URL('.', import.meta.url));

// Each sitemap <lastmod> is the last commit that touched that page's own content: its page
// file and the data it renders. Shared chrome (layouts, header, footer, styles) is left out,
// so a footer tweak does not mark every page as changed. Needs full git history in CI
// (fetch-depth: 0 in pages.yml); without it a page gets no lastmod rather than a wrong one.
function contentFiles(path) {
  const [section, slug] = path.split('/').filter(Boolean);
  switch (section) {
    case undefined:
      return ['src/pages/index.astro', 'src/data/faq.ts', 'src/assets/shots'];
    case 'features':
      return slug
        ? ['src/pages/features/[slug].astro', 'src/data/featurePages.ts', 'src/assets/shots']
        : ['src/pages/features/index.astro', 'src/data/features.ts', 'src/data/featurePages.ts'];
    case 'compare':
      return [`src/pages/compare/${slug ? '[slug]' : 'index'}.astro`, 'src/data/compare.ts'];
    case 'faq':
      return ['src/pages/faq.astro', 'src/data/faq.ts'];
    case 'guide':
      return ['src/pages/guide.astro', 'src/assets/shots'];
    default:
      return [`src/pages/${section}.astro`];
  }
}

function lastCommit(files) {
  try {
    const out = execFileSync('git', ['log', '-1', '--format=%cI', '--', ...files], {
      cwd: ROOT,
      encoding: 'utf8',
    }).trim();
    return out ? new Date(out) : undefined;
  } catch {
    return undefined;
  }
}

// The changelog and download pages are rebuilt from the latest release, so it counts too.
async function latestRelease() {
  try {
    const token = process.env.GITHUB_TOKEN;
    const res = await fetch('https://api.github.com/repos/Razee4315/Vuoom/releases/latest', {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
    });
    const json = res.ok ? await res.json() : null;
    return json?.published_at ? new Date(json.published_at) : undefined;
  } catch {
    return undefined;
  }
}
const released = await latestRelease();

function lastmod(path) {
  const dates = [lastCommit(contentFiles(path))];
  if (path === '/changelog/' || path === '/download/') dates.push(released);
  const known = dates.filter((d) => d !== undefined);
  return known.length ? new Date(Math.max(...known.map(Number))).toISOString() : undefined;
}

// The two values to change for a custom domain: `site` becomes the domain and `base`
// becomes '/'. Then add public/CNAME with the domain. Everything else follows.
export default defineConfig({
  site: SITE,
  base: BASE,
  trailingSlash: 'always',
  output: 'static',
  build: { format: 'directory', inlineStylesheets: 'auto' },
  prefetch: { prefetchAll: false, defaultStrategy: 'hover' },
  integrations: [
    sitemap({
      filter: (page) => !page.includes('/404'),
      changefreq: 'weekly',
      serialize(item) {
        const path = new URL(item.url).pathname.slice(BASE.length);
        return { ...item, lastmod: lastmod(path) };
      },
    }),
  ],
});
