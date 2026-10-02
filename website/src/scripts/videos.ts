// Real takes play only while on screen. Under reduced motion, with the browser's data saver
// on, or on a slow connection they never autoplay (and so never download): the visitor
// presses play.
function savingData(): boolean {
  const c = (navigator as Navigator & { connection?: { saveData?: boolean; effectiveType?: string } })
    .connection;
  return !!c && (c.saveData === true || /(^|-)(2g|3g)$/.test(c.effectiveType ?? ''));
}

export function initVideos(motion: boolean) {
  const frugal = savingData();
  document.querySelectorAll<HTMLElement>('[data-video]').forEach((fig) => {
    const v = fig.querySelector('video');
    if (!v) return;
    const btn = fig.querySelector<HTMLButtonElement>('[data-video-toggle]');
    const snd = fig.querySelector<HTMLButtonElement>('[data-video-sound]');
    let userPaused = !motion || frugal;
    window.matchMedia('(prefers-reduced-motion: reduce)').addEventListener('change', e => {
      if (e.matches) { userPaused = true; v.pause(); }
    });

    const sync = () => btn?.setAttribute('aria-pressed', String(!v.paused));
    v.addEventListener('play', sync);
    v.addEventListener('pause', sync);

    btn?.addEventListener('click', () => {
      if (v.paused) {
        userPaused = false;
        v.play().catch(() => {});
      } else {
        userPaused = true;
        v.pause();
      }
    });
    snd?.addEventListener('click', () => {
      v.muted = !v.muted;
      snd.setAttribute('aria-pressed', String(!v.muted));
      snd.textContent = v.muted ? 'Sound off' : 'Sound on';
      if (!v.muted && v.paused) v.play().catch(() => {});
    });

    new IntersectionObserver(
      ([e]) => {
        if (e.isIntersecting && !userPaused) {
          if (v.preload === 'none') v.preload = 'auto';
          v.play().catch(() => {});
        } else if (!e.isIntersecting) v.pause();
      },
      { threshold: 0.35 },
    ).observe(fig);
  });
}
