// Live GitHub star count, cached for the session. Stays hidden if the API is unavailable.
import { SITE } from '../lib/site';

export async function initStars() {
  const small = document.querySelectorAll<HTMLElement>('[data-stars]');
  const big = document.querySelectorAll<HTMLElement>('[data-stars-big]');
  if (!small.length && !big.length) return;
  let n: number | null = null;
  try {
    const cached = sessionStorage.getItem('vuoom-stars');
    if (cached) n = Number(cached);
  } catch {}
  if (n === null) {
    try {
      const r = await fetch(`https://api.github.com/repos/${SITE.repo}`);
      if (r.ok) n = (await r.json()).stargazers_count as number;
      if (n !== null) sessionStorage.setItem('vuoom-stars', String(n));
    } catch {}
  }
  if (n === null || Number.isNaN(n)) return;
  const text = n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
  small.forEach((el) => {
    el.textContent = text;
    el.hidden = false;
  });
  big.forEach((el) => {
    el.textContent = text;
    el.dataset.count = String(n);
  });
}
