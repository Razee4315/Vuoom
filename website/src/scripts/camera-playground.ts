import {
  SPRING_OMEGA,
  camTransform,
  clampCamera,
  springUpdateOmega,
  type Spring,
  type SpringPreset,
} from './spring';

const CLICK_DEPTH = 1.85;

// Full-screen first (rests 2.8s at 1.00x before zooming in and returning cleanly to 1.00x)
const AUTO_STEPS: ReadonlyArray<{ x: number; y: number; z: number; hold: number }> = [
  { x: 0.285, y: 0.365, z: 1.85, hold: 2000 },
  { x: 0.50, y: 0.48, z: 1.0, hold: 3200 },
  { x: 0.50, y: 0.365, z: 1.85, hold: 2000 },
  { x: 0.50, y: 0.48, z: 1.0, hold: 3200 },
];

export function initCameraPlayground(root: HTMLElement, motion: boolean) {
  const well = root.querySelector<HTMLElement>('[data-pg-well]');
  const cam = root.querySelector<HTMLElement>('[data-pg-cam]');
  const ptr = root.querySelector<SVGElement>('[data-pg-pointer]');
  const ripple = root.querySelector<HTMLElement>('[data-pg-ripple]');
  const reticle = root.querySelector<HTMLElement>('[data-pg-reticle]');
  const readout = root.querySelector<HTMLElement>('[data-pg-readout]');
  const hint = root.querySelector<HTMLElement>('[data-pg-hint]');
  const live = root.querySelector<HTMLElement>('[data-pg-live]');
  const presetBtns = [...root.querySelectorAll<HTMLButtonElement>('[data-pg-preset]')];

  if (!well || !cam || !ptr || !ripple) return;

  let preset: SpringPreset = 'smooth';
  let zoomed = false;
  let w = well.clientWidth || 880;
  let h = well.clientHeight || 550;

  const cx: Spring = { x: 0.5, v: 0 };
  const cy: Spring = { x: 0.5, v: 0 };
  const cz: Spring = { x: 1, v: 0 };
  const px: Spring = { x: 0.52, v: 0 };
  const py: Spring = { x: 0.45, v: 0 };

  let goal = { x: 0.5, y: 0.5, z: 1 };
  let pgoal = { x: 0.52, y: 0.45 };
  let gridCol = 1;
  let gridRow = 1;
  let raf = 0;
  let last = 0;
  let visible = true;
  let announceOnSettle = false;

  let autoActive = motion;
  let autoIdx = 0;
  let autoTimer: number | undefined;

  const layerRear = root.querySelector<HTMLElement>('[data-parallax="rear"]');
  const layerFront = root.querySelector<HTMLElement>('[data-parallax="front"]');
  const parX: Spring = { x: 0, v: 0 };
  const parY: Spring = { x: 0, v: 0 };
  let parGoalX = 0;
  let parGoalY = 0;
  let parRaf = 0;
  let parLast = 0;

  const resize = new ResizeObserver(() => {
    w = well.clientWidth || 880;
    h = well.clientHeight || 550;
    if (!raf) renderInstant();
  });
  resize.observe(well);

  function clearAutoTimer() {
    if (autoTimer !== undefined) {
      clearTimeout(autoTimer);
      autoTimer = undefined;
    }
  }

  function updateUIState() {
    root.classList.toggle('is-zoomed', zoomed);
    if (hint) {
      hint.textContent = zoomed ? 'Click anywhere to zoom out' : 'Click anywhere to zoom';
    }
  }

  function renderInstant() {
    cam!.style.transform = camTransform(cx.x, cy.x, cz.x, w, h);
    ptr!.style.transform = `translate3d(${(px.x * w).toFixed(2)}px, ${(py.x * h).toFixed(2)}px, 0)`;
    if (readout) readout.textContent = `${cz.x.toFixed(2)}×`;
  }

  function fireRipple(nx: number, ny: number) {
    if (!motion) return;
    ripple!.style.left = `${(nx * 100).toFixed(2)}%`;
    ripple!.style.top = `${(ny * 100).toFixed(2)}%`;
    ripple!.animate(
      [
        { opacity: 0.95, transform: 'scale(0.2)' },
        { opacity: 0, transform: 'scale(2.5)' },
      ],
      { duration: 540, easing: 'cubic-bezier(0.2, 0.7, 0.2, 1)' },
    );
  }

  function wake() {
    if (raf || !visible || document.hidden) return;
    last = performance.now();
    raf = requestAnimationFrame(step);
  }

  function step(now: number) {
    raf = 0;
    if (!visible || document.hidden) return;
    const dt = Math.max(0.001, Math.min((now - last) / 1000, 0.04));
    last = now;

    const omegas = SPRING_OMEGA[preset];
    springUpdateOmega(cx, goal.x, omegas.cam, dt);
    springUpdateOmega(cy, goal.y, omegas.cam, dt);
    springUpdateOmega(cz, goal.z, omegas.zoom, dt);
    springUpdateOmega(px, pgoal.x, omegas.pointer, dt);
    springUpdateOmega(py, pgoal.y, omegas.pointer, dt);

    renderInstant();

    const dxPx = Math.abs(cx.x - goal.x) * w;
    const dyPx = Math.abs(cy.x - goal.y) * h;
    const vxPx = Math.abs(cx.v) * w;
    const vyPx = Math.abs(cy.v) * h;
    const dz = Math.abs(cz.x - goal.z);
    const vz = Math.abs(cz.v);
    const dpx = Math.abs(px.x - pgoal.x) * w;
    const dpy = Math.abs(py.x - pgoal.y) * h;

    const settled =
      dxPx < 0.25 &&
      dyPx < 0.25 &&
      vxPx < 3 &&
      vyPx < 3 &&
      dz < 0.001 &&
      vz < 0.01 &&
      dpx < 0.4 &&
      dpy < 0.4;

    if (settled) {
      cx.x = goal.x;
      cy.x = goal.y;
      cz.x = goal.z;
      px.x = pgoal.x;
      py.x = pgoal.y;
      cx.v = cy.v = cz.v = px.v = py.v = 0;
      renderInstant();
      if (announceOnSettle && live) {
        live.textContent = goal.z > 1.01 ? `Zoomed to ${goal.z.toFixed(2)}x` : 'Returned to full screen 1.00x';
      }
      return;
    }

    raf = requestAnimationFrame(step);
  }

  function zoomTo(nx: number, ny: number, targetZoom: number, fireClick = true, announce = true) {
    const clampedX = Math.max(0.03, Math.min(0.97, nx));
    const clampedY = Math.max(0.03, Math.min(0.97, ny));
    pgoal = { x: clampedX, y: clampedY };
    const [gx, gy] = clampCamera(clampedX, clampedY, targetZoom);
    goal = { x: targetZoom > 1.01 ? gx : 0.5, y: targetZoom > 1.01 ? gy : 0.5, z: targetZoom };
    zoomed = targetZoom > 1.01;
    announceOnSettle = announce;
    updateUIState();
    if (fireClick) fireRipple(clampedX, clampedY);

    if (!motion) {
      cx.x = goal.x;
      cy.x = goal.y;
      cz.x = goal.z;
      px.x = pgoal.x;
      py.x = pgoal.y;
      renderInstant();
      if (announce && live) {
        live.textContent = zoomed ? `Zoomed to ${goal.z.toFixed(2)}x` : 'Returned to full screen 1.00x';
      }
      return;
    }
    wake();
  }

  function scheduleAuto(delay: number) {
    clearAutoTimer();
    if (!autoActive || !motion || !visible || document.hidden) return;
    autoTimer = window.setTimeout(() => {
      autoTimer = undefined;
      if (!autoActive || !motion || !visible || document.hidden) return;
      const s = AUTO_STEPS[autoIdx];
      autoIdx = (autoIdx + 1) % AUTO_STEPS.length;
      zoomTo(s.x, s.y, s.z, s.z > 1.01, false);
      scheduleAuto(s.hold);
    }, delay);
  }

  function stopAuto() {
    autoActive = false;
    clearAutoTimer();
  }

  function resetCamera() {
    stopAuto();
    zoomTo(0.50, 0.48, 1.0, false, true);
  }

  // Simple 1-click toggle: click to zoom into target, click again to return to full screen
  well.addEventListener('pointerdown', (e) => {
    if (e.button !== 0 && e.pointerType === 'mouse') return;
    stopAuto();
    if (zoomed) {
      zoomTo(0.50, 0.48, 1.0, false, true);
      return;
    }
    const rect = well.getBoundingClientRect();
    const sx = (e.clientX - rect.left) / rect.width;
    const sy = (e.clientY - rect.top) / rect.height;
    const [ccx, ccy] = clampCamera(cx.x, cy.x, cz.x);
    const imgX = ccx + (sx - 0.5) / cz.x;
    const imgY = ccy + (sy - 0.5) / cz.x;
    zoomTo(imgX, imgY, CLICK_DEPTH, true, true);
  });

  function syncReticle() {
    if (!reticle) return;
    const rx = (gridCol + 0.5) / 3;
    const ry = (gridRow + 0.5) / 3;
    reticle.style.left = `${(rx * 100).toFixed(1)}%`;
    reticle.style.top = `${(ry * 100).toFixed(1)}%`;
  }
  syncReticle();

  well.addEventListener('keydown', (e) => {
    const k = e.key;
    if (k === 'ArrowRight') {
      e.preventDefault();
      gridCol = Math.min(2, gridCol + 1);
      syncReticle();
    } else if (k === 'ArrowLeft') {
      e.preventDefault();
      gridCol = Math.max(0, gridCol - 1);
      syncReticle();
    } else if (k === 'ArrowDown') {
      e.preventDefault();
      gridRow = Math.min(2, gridRow + 1);
      syncReticle();
    } else if (k === 'ArrowUp') {
      e.preventDefault();
      gridRow = Math.max(0, gridRow - 1);
      syncReticle();
    } else if (k === 'Enter' || k === ' ' || ((e.ctrlKey || e.metaKey) && e.shiftKey && k.toLowerCase() === 'z')) {
      e.preventDefault();
      stopAuto();
      if (zoomed) {
        zoomTo(0.50, 0.48, 1.0, false, true);
      } else {
        zoomTo((gridCol + 0.5) / 3, (gridRow + 0.5) / 3, CLICK_DEPTH, true, true);
      }
    } else if (k === 'Escape') {
      e.preventDefault();
      resetCamera();
    }
  });

  presetBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      const next = btn.dataset.pgPreset as SpringPreset;
      if (!next) return;
      preset = next;
      presetBtns.forEach((b) => b.setAttribute('aria-checked', String(b === btn)));
      updateUIState();
      if (autoActive) {
        autoIdx = 0;
        const s = AUTO_STEPS[0];
        autoIdx = 1;
        zoomTo(s.x, s.y, s.z, true, true);
        scheduleAuto(s.hold);
      } else if (zoomed) {
        zoomTo(pgoal.x, pgoal.y, CLICK_DEPTH, true, true);
      }
    });
  });

  const modal = document.querySelector<HTMLDialogElement>('[data-film-modal]');
  const openers = [...document.querySelectorAll<HTMLButtonElement>('[data-open-film]')];
  const closeBtn = modal?.querySelector<HTMLButtonElement>('[data-close-film]');
  let openerRef: HTMLButtonElement | null = null;

  openers.forEach((btn) => {
    btn.addEventListener('click', () => {
      if (!modal) return;
      openerRef = btn;
      modal.showModal();
      closeBtn?.focus();
    });
  });

  function closeModal() {
    if (!modal || !modal.open) return;
    modal.querySelector('video')?.pause();
    modal.close();
    openerRef?.focus();
  }
  closeBtn?.addEventListener('click', closeModal);
  modal?.addEventListener('click', (e) => {
    if (e.target === modal) closeModal();
  });
  modal?.addEventListener('cancel', () => {
    modal.querySelector('video')?.pause();
    setTimeout(() => openerRef?.focus(), 0);
  });

  const finePointer = window.matchMedia('(pointer: fine)').matches;
  function stepParallax(now: number) {
    parRaf = 0;
    if (!motion || !visible || document.hidden) return;
    const dt = Math.max(0.001, Math.min((now - parLast) / 1000, 0.04));
    parLast = now;
    springUpdateOmega(parX, parGoalX, 8, dt);
    springUpdateOmega(parY, parGoalY, 8, dt);
    if (layerRear) {
      layerRear.style.transform = `translate3d(${(parX.x * 4).toFixed(2)}px, ${(parY.x * 4).toFixed(2)}px, 0)`;
    }
    if (layerFront) {
      layerFront.style.transform = `translate3d(${(parX.x * -8).toFixed(2)}px, ${(parY.x * -8).toFixed(2)}px, 0)`;
    }
    if (
      Math.abs(parX.x - parGoalX) < 0.004 &&
      Math.abs(parY.x - parGoalY) < 0.004 &&
      Math.abs(parX.v) < 0.02 &&
      Math.abs(parY.v) < 0.02
    ) {
      return;
    }
    parRaf = requestAnimationFrame(stepParallax);
  }

  if (motion && finePointer && (layerRear || layerFront)) {
    root.addEventListener(
      'pointermove',
      (e) => {
        const r = root.getBoundingClientRect();
        parGoalX = ((e.clientX - r.left) / r.width - 0.5) * 2;
        parGoalY = ((e.clientY - r.top) / r.height - 0.5) * 2;
        if (!parRaf) {
          parLast = performance.now();
          parRaf = requestAnimationFrame(stepParallax);
        }
      },
      { passive: true },
    );
    root.addEventListener('pointerleave', () => {
      parGoalX = 0;
      parGoalY = 0;
      if (!parRaf) {
        parLast = performance.now();
        parRaf = requestAnimationFrame(stepParallax);
      }
    });
  }

  new IntersectionObserver(([e]) => {
    visible = e.isIntersecting;
    if (!visible) {
      clearAutoTimer();
      if (raf) { cancelAnimationFrame(raf); raf = 0; }
      if (parRaf) { cancelAnimationFrame(parRaf); parRaf = 0; }
    } else if (autoActive && motion && !document.hidden && autoTimer === undefined && !raf) {
      scheduleAuto(1400);
    }
  }).observe(root);

  document.addEventListener('visibilitychange', () => {
    if (document.hidden) {
      clearAutoTimer();
      if (raf) { cancelAnimationFrame(raf); raf = 0; }
      if (parRaf) { cancelAnimationFrame(parRaf); parRaf = 0; }
    } else if (visible && autoActive && motion && autoTimer === undefined && !raf) {
      scheduleAuto(1400);
    }
  });

  renderInstant();
  updateUIState();
  if (motion && visible && !document.hidden) {
    scheduleAuto(2800);
  }
}
