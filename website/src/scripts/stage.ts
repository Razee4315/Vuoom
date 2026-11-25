// Scripted take on the inner Stage component using real take-flat.png (1600x900 Aurora 2.4 launch).
// Ctrl+Shift+Z over the stage zooms at the visitor's own pointer.
import { HL, camTransform, clampCamera, springUpdate, type Spring } from './spring';

interface Beat {
  x: number;
  y: number;
  z: number;
}

// Targets on take-flat.png (1600x900): Beta sign-ups (12,408), Rollout plan (Live), Launch checklist, Ship v2.4, Reset.
const TAKE: Beat[] = [
  { x: 0.205, y: 0.265, z: 2.0 },
  { x: 0.355, y: 0.398, z: 2.0 },
  { x: 0.755, y: 0.415, z: 1.85 },
  { x: 0.958, y: 0.025, z: 2.3 },
  { x: 0.5, y: 0.5, z: 1 },
];
const TRAVEL = 0.75;
const HOLD = 1.75;

export function initStage(root: HTMLElement, opts: { onZoom?: (zoomIn: boolean) => void } = {}) {
  const well = root.querySelector<HTMLElement>('.stage__well')!;
  const cam = root.querySelector<HTMLElement>('[data-cam]')!;
  const ptr = root.querySelector<SVGElement>('[data-stage-pointer]')!;
  const ripple = root.querySelector<HTMLElement>('[data-ripple]')!;
  const readout = root.querySelector<HTMLElement>('[data-zoom-readout]');
  const toggle = root.querySelector<HTMLButtonElement>('[data-stage-toggle]');

  const cx: Spring = { x: 0.5, v: 0 };
  const cy: Spring = { x: 0.5, v: 0 };
  const cz: Spring = { x: 1, v: 0 };
  const px: Spring = { x: 0.62, v: 0 };
  const py: Spring = { x: 0.92, v: 0 };
  let goal = { x: 0.5, y: 0.5, z: 1 };
  let pgoal = { x: 0.62, y: 0.92 };

  let w = well.clientWidth;
  let h = well.clientHeight;
  const resize = new ResizeObserver(() => {
    w = well.clientWidth;
    h = well.clientHeight;
  });
  resize.observe(well);

  let running = false;
  let paused = false;
  let visible = true;
  let started = false;
  let raf = 0;
  let last = 0;
  let t = 0;
  let beat = -1;
  let clicked = false;
  let manual = 0;
  let manualZoomed = false;

  const ptrXf = () => `translate3d(${px.x * w}px, ${py.x * h}px, 0)`;

  const click = () => {
    ripple.style.left = `${px.x * 100}%`;
    ripple.style.top = `${py.x * 100}%`;
    ripple.animate(
      [
        { opacity: 0.95, transform: 'scale(0.2)' },
        { opacity: 0, transform: 'scale(2.4)' },
      ],
      { duration: 600, easing: 'cubic-bezier(0.2, 0.7, 0.2, 1)' },
    );
    ptr.animate([{ transform: `${ptrXf()} scale(0.84)` }, { transform: `${ptrXf()} scale(1)` }], {
      duration: 200,
      easing: 'cubic-bezier(0.2, 0.7, 0.2, 1)',
    });
  };

  const step = (now: number) => {
    raf = 0;
    if (!running) return;
    raf = requestAnimationFrame(step);
    const dt = Math.max(0, Math.min((now - last) / 1000, 1 / 20));
    last = now;

    if (manual > 0) {
      manual -= dt;
      if (manual <= 0 && !manualZoomed) {
        t = 0;
        beat = -1;
      }
    } else {
      t += dt;
      const len = TRAVEL + HOLD;
      const b = Math.floor(t / len);
      if (b < 0 || b >= TAKE.length) {
        t = 0;
        beat = -1;
      } else {
        if (b !== beat) {
          beat = b;
          clicked = false;
          pgoal = { x: TAKE[b].x, y: TAKE[b].y };
        }
        if (!clicked && t - b * len >= TRAVEL) {
          clicked = true;
          const k = TAKE[b];
          if (k.z > 1) click();
          const [gx, gy] = clampCamera(k.x, k.y, k.z);
          goal = { x: k.z > 1 ? gx : 0.5, y: k.z > 1 ? gy : 0.5, z: k.z };
          opts.onZoom?.(k.z > 1);
        }
      }
    }

    springUpdate(cx, goal.x, HL.pan, dt);
    springUpdate(cy, goal.y, HL.pan, dt);
    springUpdate(cz, goal.z, HL.zoom, dt);
    springUpdate(px, pgoal.x, HL.pointer, dt);
    springUpdate(py, pgoal.y, HL.pointer, dt);

    cam.style.transform = camTransform(cx.x, cy.x, cz.x, w, h);
    ptr.style.transform = ptrXf();
    if (readout) readout.textContent = `${cz.x.toFixed(1)}×`;
  };

  const setRunning = () => {
    const should = started && visible && !paused && !document.hidden;
    if (should && !running) {
      running = true;
      last = performance.now();
      if (!raf) raf = requestAnimationFrame(step);
    } else if (!should) {
      running = false;
      if (raf) {
        cancelAnimationFrame(raf);
        raf = 0;
      }
    }
  };

  new IntersectionObserver(([e]) => {
    visible = e.isIntersecting;
    setRunning();
  }).observe(root);
  document.addEventListener('visibilitychange', setRunning);

  if (toggle) {
    toggle.hidden = false;
    toggle.addEventListener('click', () => {
      paused = !paused;
      toggle.setAttribute('aria-pressed', String(paused));
      setRunning();
    });
  }

  let hover: { x: number; y: number } | null = null;
  well.addEventListener('pointermove', (e) => {
    const r = well.getBoundingClientRect();
    const [ccx, ccy] = clampCamera(cx.x, cy.x, cz.x);
    const sx = (e.clientX - r.left) / r.width;
    const sy = (e.clientY - r.top) / r.height;
    hover = { x: ccx + (sx - 0.5) / cz.x, y: ccy + (sy - 0.5) / cz.x };
  });
  well.addEventListener('pointerleave', () => (hover = null));
  window.addEventListener('keydown', (e) => {
    if (!(e.ctrlKey && e.shiftKey && e.key.toLowerCase() === 'z')) return;
    const r = root.getBoundingClientRect();
    if (r.bottom < 0 || r.top > innerHeight) return;
    e.preventDefault();
    manualZoomed = !manualZoomed;
    manual = manualZoomed ? 9999 : 1.2;
    const at = hover ?? { x: px.x, y: py.x };
    if (manualZoomed) {
      pgoal = at;
      const [gx, gy] = clampCamera(at.x, at.y, 2.2);
      goal = { x: gx, y: gy, z: 2.2 };
      click();
    } else {
      goal = { x: 0.5, y: 0.5, z: 1 };
    }
    opts.onZoom?.(manualZoomed);
  });

  return {
    start() {
      started = true;
      ptr.style.opacity = '1';
      setRunning();
    },
    stop() {
      running = false;
      if (raf) cancelAnimationFrame(raf);
      resize.disconnect();
    },
  };
}
