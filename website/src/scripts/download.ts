// Refresh download links from the GitHub API, so a release published after the site was
// built is picked up. Falls back silently to the build-time links.
import { SITE } from '../lib/site';

interface GhAsset {
  name: string;
  size: number;
  browser_download_url: string;
}

export async function initDownload() {
  const links = document.querySelectorAll<HTMLAnchorElement>('[data-dl]');
  if (!links.length) return;
  try {
    const r = await fetch(`https://api.github.com/repos/${SITE.repo}/releases/latest`);
    if (!r.ok) return;
    const rel = (await r.json()) as { tag_name: string; assets?: GhAsset[] };
    const find = (re: RegExp) => rel.assets?.find((a) => re.test(a.name));
    const exe = find(/setup\.exe$/i);
    const msi = find(/\.msi$/i);
    const version = String(rel.tag_name).replace(/^v/, '');
    links.forEach((a) => {
      const asset = a.dataset.dl === 'msi' ? msi : exe;
      if (asset) a.href = asset.browser_download_url;
    });
    document
      .querySelectorAll<HTMLElement>('[data-dl-version]')
      .forEach((el) => (el.textContent = version));
    document.querySelectorAll<HTMLElement>('[data-dl-size]').forEach((el) => {
      const asset = el.dataset.dlSize === 'msi' ? msi : exe;
      if (asset) el.textContent = `${(asset.size / 1024 / 1024).toFixed(1)} MB`;
    });
  } catch {}
}
