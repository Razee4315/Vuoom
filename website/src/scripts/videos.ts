// Videos play only while in view (or strictly on demand when data-video="demand").
// Pauses on tab hide, offscreen scroll, or reduced-motion change.
function savingData(): boolean {
  const c = (navigator as Navigator & { connection?: { saveData?: boolean; effectiveType?: string } })
    .connection;
  return !!c && (c.saveData === true || /(^|-)(2g|3g)$/.test(c.effectiveType ?? ''));
}

export function attachVideoSource(v: HTMLVideoElement): void {
  if (v.dataset.src && !v.querySelector('source')) {
    const src = document.createElement('source');
    src.src = v.dataset.src;
    src.type = 'video/mp4';
    v.appendChild(src);
    v.load();
  }
}

export function initVideos(motion: boolean) {
  const frugal = savingData();
  const figs = [...document.querySelectorAll<HTMLElement>('[data-video]')];
  if (!figs.length) return;

  figs.forEach((fig) => {
    const v = fig.querySelector('video');
    if (!v) return;
    const isDemand = fig.dataset.video === 'demand';
    const btn = fig.querySelector<HTMLButtonElement>('[data-video-toggle]');
    const snd = fig.querySelector<HTMLButtonElement>('[data-video-sound]');
    let userPaused = !motion || frugal || isDemand;

    window.matchMedia('(prefers-reduced-motion: reduce)').addEventListener('change', (e) => {
      if (e.matches) {
        userPaused = true;
        v.pause();
      }
    });

    const sync = () => btn?.setAttribute('aria-pressed', String(!v.paused));
    v.addEventListener('play', sync);
    v.addEventListener('pause', sync);

    btn?.addEventListener('click', () => {
      if (v.paused) {
        attachVideoSource(v);
        userPaused = false;
        v.preload = 'auto';
        v.play().catch(() => {});
      } else {
        userPaused = true;
        v.pause();
      }
    });

    v.addEventListener('click', () => btn?.click());

    snd?.addEventListener('click', () => {
      attachVideoSource(v);
      v.muted = !v.muted;
      snd.setAttribute('aria-pressed', String(!v.muted));
      snd.textContent = v.muted ? 'Sound off' : 'Sound on';
      if (!v.muted && v.paused) {
        userPaused = false;
        v.play().catch(() => {});
      }
    });

    document.addEventListener('visibilitychange', () => {
      if (document.hidden && !v.paused) v.pause();
    });

    new IntersectionObserver(
      ([e]) => {
        if (e.isIntersecting && !userPaused && !document.hidden) {
          attachVideoSource(v);
          if (v.preload === 'none') v.preload = 'auto';
          v.play().catch(() => {});
        } else if (!e.isIntersecting && !v.paused) {
          v.pause();
        }
      },
      { threshold: 0.25 },
    ).observe(fig);
  });
}
