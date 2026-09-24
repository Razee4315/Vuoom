// M10, the signature: a scripted "take" on the hero stage. The pointer springs to each
// target, clicks (press + ripple), and the camera springs in with the app's half-lives.
// Ctrl+Shift+Z over the stage zooms at the visitor's own pointer, like the real shortcut.
import { HL, camTransform, clampCamera, springUpdate, type Spring } from './spring';

interface Beat {
  x: number;
  y: number;
  z: number;
}

// Targets on screenshot-editor.png (1440×900): Play, the 1.8× zoom block, Auto zooms, Export.
const TAKE: Beat[] = [
  { x: 0.047, y: 0.727, z: 1.8 },
  { x: 0.247, y: 0.802, z: 2.0 },
  { x: 0.843, y: 0.652, z: 2.2 },
  { x: 0.868, y: 0.026, z: 2.4 },
  { x: 0.46, y: 0.4, z: 1 },
];
const TRAVEL = 0.75; // seconds from beat start to the click
const HOLD = 1.75; // seconds after the click

export function initStage(root: HTMLElement, opts: { onZoom?: (zoomIn: boolean) => void } = {}) {
  const well = root.querySelector<HTMLElement>('.stage__well')!;
  const cam = root.querySelector<HTMLElement>('[data-cam]')!;
  const ptr = root.querySelector<SVGElement>('[data-stage-pointer]')!;
  const ripple = root.querySelector<HTMLElement>('[data-ripple]')!;
  const readout = root.querySelector<HTMLElement>('[data-zoom-readout]');
  const toggle = root.querySelector<HTMLButtonElement>('[data-stage-toggle]');
  const tc = document.querySelector<HTMLElement>('[data-timecode]');

  const cx: Spring = { x: 0.5, v: 0 };
  const cy: Spring = { x: 0.5, v: 0 };
  const cz: Spring = { x: 1, v: 0 };
  const px: Spring = { x: 0.62, v: 0 };
  const py: Spring = { x: 1.08, v: 0 };
  let goal = { x: 0.5, y: 0.5, z: 1 };
  let pgoal = { x: 0.62, y: 1.08 };

  let w = well.clientWidth;
  let h = well.clientHeight;
  new ResizeObserver(() => {
    w = well.clientWidth;
    h = well.clientHeight;
  }).observe(well);

  let running = false;
  let paused = false;
  let visible = true;
  let started = false;
  let raf = 0;
  let last = 0;
  let t = 0; // take clock, seconds
  let beat = -1;
  let clicked = false;
  let manual = 0; // seconds of manual control remaining
  let manualZoomed = false;
  let frames = 0;

  const click = () => {
    ptr.classList.remove('is-down');
    void ptr.getBoundingClientRect();
    ptr.classList.add('is-down');
    ripple.style.left = `${px.x * 100}%`;
    ripple.style.top = `${py.x * 100}%`;
    ripple.animate(
      [
        { opacity: 0.95, transform: 'scale(0.2)' },
        { opacity: 0, transform: 'scale(2.4)' },
      ],
      { duration: 650, easing: 'cubic-bezier(0.16, 1, 0.3, 1)' },
    );
    ptr.animate([{ transform: `${ptrXf()} scale(0.84)` }, { transform: `${ptrXf()} scale(1)` }], {
      duration: 220,
      easing: 'cubic-bezier(0.25, 1, 0.5, 1)',
    });
  };

  const ptrXf = () => `translate3d(${px.x * w}px, ${py.x * h}px, 0)`;

  const step = (now: number) => {
    raf = requestAnimationFrame(step);
    const dt = Math.max(0, Math.min((now - last) / 1000, 1 / 20));
    last = now;
    if (!running) return;

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
        frames = 0;
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
    if (tc) {
      frames++;
      const f = Math.floor(frames / 2); // 60 Hz display, 30 fps timecode
      const ff = f % 30;
      const s = Math.floor(f / 30);
      tc.textContent = `00:00:${String(s % 60).padStart(2, '0')}:${String(ff).padStart(2, '0')}`;
    }
  };

  const setRunning = () => {
    const should = started && visible && !paused && !document.hidden;
    if (should && !running) {
      running = true;
      last = performance.now();
    } else if (!should) running = false;
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

  // Try it: Ctrl+Shift+Z with the pointer over the stage zooms there, like recording.
  let hover: { x: number; y: number } | null = null;
  well.addEventListener('pointermove', (e) => {
    const r = well.getBoundingClientRect();
    // map the screen position back through the current camera into image space
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

  raf = requestAnimationFrame(step);

  return {
    start() {
      started = true;
      ptr.style.opacity = '1';
      setRunning();
    },
    stop() {
      cancelAnimationFrame(raf);
    },
  };
}
