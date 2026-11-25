import { attachVideoSource } from './videos';

interface TakeMeta {
  id: string;
  src: string;
  poster: string;
  title: string;
  meta: string;
  hasSound: boolean;
}

export function initScreeningRoom(root: HTMLElement) {
  const video = root.querySelector<HTMLVideoElement>('[data-sr-video]');
  const fig = root.querySelector<HTMLElement>('[data-sr-fig]');
  const playBtn = root.querySelector<HTMLButtonElement>('[data-video-toggle]');
  const soundBtn = root.querySelector<HTMLButtonElement>('[data-video-sound]');
  const captionTitle = root.querySelector<HTMLElement>('[data-sr-title]');
  const captionMeta = root.querySelector<HTMLElement>('[data-sr-meta]');
  const cards = [...root.querySelectorAll<HTMLButtonElement>('[data-sr-card]')];

  if (!video || !fig || !cards.length) return;

  function loadTake(btn: HTMLButtonElement, autoPlay: boolean) {
    const id = btn.dataset.srCard || 'demo';
    const take: TakeMeta = {
      id,
      src: btn.dataset.src || '',
      poster: btn.dataset.poster || '',
      title: btn.dataset.title || '',
      meta: btn.dataset.meta || '',
      hasSound: btn.dataset.sound === 'true',
    };

    cards.forEach((c) => c.setAttribute('aria-pressed', String(c === btn)));
    if (captionTitle) captionTitle.textContent = take.title;
    if (captionMeta) captionMeta.textContent = take.meta;

    // Reset current source cleanly before swapping
    video!.pause();
    video!.querySelectorAll('source').forEach((s) => s.remove());
    video!.poster = take.poster;
    video!.dataset.src = take.src;
    video!.muted = true;

    if (soundBtn) {
      soundBtn.hidden = !take.hasSound;
      soundBtn.setAttribute('aria-pressed', 'false');
      soundBtn.textContent = 'Sound off';
    }
    if (playBtn) playBtn.setAttribute('aria-pressed', 'false');

    if (autoPlay) {
      attachVideoSource(video!);
      video!.preload = 'auto';
      video!.play().catch(() => {});
    }
  }

  cards.forEach((btn) => {
    btn.addEventListener('click', () => loadTake(btn, true));
  });
}
