// M12: the Vuoom smooth pointer replaces the native one on desktop. It follows the real
// pointer with the app's pointer spring (half-life 0.12 s), grows over things you can
// press, and rings red on click. Native cursor returns over text fields and on keyboard use.
import { HL, springUpdate, type Spring } from './spring';

export function initPointer() {
  const html = document.documentElement;
  const el = document.querySelector<HTMLElement>('[data-pointer]');
  if (!el || !html.classList.contains('fine')) return;

  const x: Spring = { x: -100, v: 0 };
  const y: Spring = { x: -100, v: 0 };
  let gx = -100;
  let gy = -100;
  let last = performance.now();
  let active = false;

  const loop = (now: number) => {
    const dt = Math.max(0, Math.min((now - last) / 1000, 1 / 20));
    last = now;
    springUpdate(x, gx, HL.pointer * 0.5, dt);
    springUpdate(y, gy, HL.pointer * 0.5, dt);
    el.style.transform = `translate3d(${x.x - 3}px, ${y.x - 2.5}px, 0)`;
    requestAnimationFrame(loop);
  };
  requestAnimationFrame(loop);

  const HOT = 'a, button, summary, [role="slider"], label, [data-hot]';
  const TEXT = 'input, textarea, select, [contenteditable="true"]';

  window.addEventListener(
    'pointermove',
    (e) => {
      if (e.pointerType !== 'mouse') return;
      gx = e.clientX;
      gy = e.clientY;
      if (!active) {
        active = true;
        x.x = gx;
        y.x = gy;
      }
      const t = e.target as Element;
      const overText = !!t.closest?.(TEXT);
      html.classList.toggle('pointer-on', !overText);
      el.classList.toggle('is-hot', !!t.closest?.(HOT));
    },
    { passive: true },
  );
  window.addEventListener('pointerdown', () => {
    el.classList.remove('is-down');
    void el.offsetWidth;
    el.classList.add('is-down');
  });
  window.addEventListener('pointerup', () => setTimeout(() => el.classList.remove('is-down'), 120));
  document.addEventListener('mouseleave', () => html.classList.remove('pointer-on'));
  window.addEventListener('keydown', (e) => {
    if (e.key === 'Tab') html.classList.remove('pointer-on');
  });
}
