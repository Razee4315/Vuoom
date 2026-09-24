// M08: a draggable, keyboard-operable before/after wipe.
export function initBeforeAfter(root: HTMLElement) {
  const handle = root.querySelector<HTMLElement>('[data-ba-handle]');
  if (!handle) return;
  let split = 50;

  const set = (v: number) => {
    split = Math.min(100, Math.max(0, v));
    root.style.setProperty('--split', `${split}%`);
    handle.setAttribute('aria-valuenow', String(Math.round(split)));
    handle.setAttribute('aria-valuetext', `${Math.round(split)}% flat recording`);
  };

  const fromEvent = (e: PointerEvent) => {
    const r = root.getBoundingClientRect();
    set(((e.clientX - r.left) / r.width) * 100);
  };

  let dragging = false;
  root.addEventListener('pointerdown', (e) => {
    dragging = true;
    handle.classList.add('is-drag');
    root.setPointerCapture(e.pointerId);
    fromEvent(e);
  });
  root.addEventListener('pointermove', (e) => {
    if (dragging) fromEvent(e);
  });
  const end = () => {
    dragging = false;
    handle.classList.remove('is-drag');
  };
  root.addEventListener('pointerup', end);
  root.addEventListener('pointercancel', end);

  handle.addEventListener('keydown', (e) => {
    const big = e.shiftKey ? 10 : 5;
    const map: Record<string, number> = {
      ArrowLeft: split - big,
      ArrowDown: split - big,
      ArrowRight: split + big,
      ArrowUp: split + big,
      Home: 0,
      End: 100,
      PageDown: split - 25,
      PageUp: split + 25,
    };
    if (e.key in map) {
      e.preventDefault();
      set(map[e.key]);
    }
  });
}
