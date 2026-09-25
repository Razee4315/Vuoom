// Real takes play only while on screen. Under reduced motion they never autoplay; the
// visitor presses play.
export function initVideos(motion: boolean) {
  document.querySelectorAll<HTMLElement>('[data-video]').forEach((fig) => {
    const v = fig.querySelector('video');
    if (!v) return;
    const btn = fig.querySelector<HTMLButtonElement>('[data-video-toggle]');
    const snd = fig.querySelector<HTMLButtonElement>('[data-video-sound]');
    let userPaused = !motion;
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
