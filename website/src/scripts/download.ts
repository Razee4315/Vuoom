// Refresh download links from the GitHub API, so a release published after the site was
// built is picked up. Falls back silently to the build-time links.
import { SITE } from '../lib/site';

interface GhAsset {
  name: string;
  size: number;
  browser_download_url: string;
}

/** Visitors who can't run the installer here (a phone, a Mac) get a way to keep the page. */
function initElsewhere() {
  const note = document.querySelector<HTMLElement>('[data-not-windows]');
  if (!note) return;
  const platform =
    (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform ??
    navigator.userAgent;
  if (/win/i.test(platform)) return;
  note.hidden = false;
  const copy = note.querySelector<HTMLButtonElement>('[data-copy-link]');
  copy?.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(location.href);
      copy.textContent = copy.dataset.copied ?? 'Copied';
    } catch {
      // Clipboard unavailable (an insecure context, a denied permission): show the link.
      copy.textContent = location.href;
    }
  });
}

export async function initDownload() {
  initElsewhere();
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
