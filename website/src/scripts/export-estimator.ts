type ExportFormat = 'gif' | 'mp4';

interface ExportState {
  format: ExportFormat;
  seconds: number;
  width: number;
  fps: number;
}

function estimateBytes(s: ExportState): number {
  if (s.format === 'gif') {
    const pixelScale = Math.pow(s.width / 1000, 1.35);
    const fpsScale = Math.pow(s.fps / 15, 0.78);
    const bytesPerSec = 480_000 * pixelScale * fpsScale;
    return Math.round(bytesPerSec * s.seconds);
  }
  const pixelScale = Math.pow(s.width / 1080, 1.45);
  const fpsScale = Math.pow(s.fps / 30, 0.62);
  const bytesPerSec = 145_000 * pixelScale * fpsScale + 16_000;
  return Math.round(bytesPerSec * s.seconds);
}

const WIDTH_OPTIONS = [800, 1080, 1440, 2160];
const FPS_OPTIONS = [15, 24, 30, 60];
const LIMIT_BYTES = 10 * 1024 * 1024;

export function initExportEstimator(root: HTMLElement, motion = true) {
  const fmtBtns = [...root.querySelectorAll<HTMLButtonElement>('[data-ee-format]')];
  const durInput = root.querySelector<HTMLInputElement>('[data-ee-duration]');
  const durOut = root.querySelector<HTMLElement>('[data-ee-dur-val]');
  const sizeOut = root.querySelector<HTMLElement>('[data-ee-size]');
  const specOut = root.querySelector<HTMLElement>('[data-ee-spec]');
  const statusOut = root.querySelector<HTMLElement>('[data-ee-status]');
  const fillBar = root.querySelector<HTMLElement>('[data-ee-fill]');
  const fitBtn = root.querySelector<HTMLButtonElement>('[data-ee-fit]');
  const live = root.querySelector<HTMLElement>('[data-ee-live]');

  if (!durInput || !sizeOut || !fillBar) return;

  const state: ExportState = {
    format: 'gif',
    seconds: Number(durInput.value) || 14,
    width: 1080,
    fps: 15,
  };

  let displayedMb = estimateBytes(state) / (1024 * 1024);
  let targetMb = displayedMb;
  let sizeRaf = 0;

  function formatWidth(w: number): string {
    return w === 2160 ? '4K' : `${w}p`;
  }

  function renderSizeTween(startMb: number, endMb: number) {
    if (sizeRaf) {
      cancelAnimationFrame(sizeRaf);
      sizeRaf = 0;
    }
    if (!motion || Math.abs(endMb - startMb) < 0.05) {
      displayedMb = endMb;
      sizeOut!.textContent = `${endMb.toFixed(1)} MB`;
      return;
    }
    const start = performance.now();
    const dur = 220;
    const tick = (now: number) => {
      sizeRaf = 0;
      const t = Math.min(1, (now - start) / dur);
      const eased = 1 - Math.pow(1 - t, 3);
      displayedMb = startMb + (endMb - startMb) * eased;
      sizeOut!.textContent = `${displayedMb.toFixed(1)} MB`;
      if (t < 1) {
        sizeRaf = requestAnimationFrame(tick);
      } else {
        displayedMb = endMb;
      }
    };
    sizeRaf = requestAnimationFrame(tick);
  }

  function update(instant = false) {
    const bytes = estimateBytes(state);
    const mb = bytes / (1024 * 1024);
    const fits = bytes <= LIMIT_BYTES;
    const prevMb = displayedMb;
    targetMb = mb;

    if (instant) {
      displayedMb = targetMb;
      sizeOut!.textContent = `${targetMb.toFixed(1)} MB`;
    } else {
      renderSizeTween(prevMb, targetMb);
    }

    if (durOut) durOut.textContent = `${state.seconds}s`;
    if (specOut) specOut.textContent = `${formatWidth(state.width)} · ${state.fps} fps`;

    const barPct = Math.min(100, (mb / 16) * 100);
    fillBar!.style.width = `${barPct.toFixed(1)}%`;
    root.dataset.fits = String(fits);

    if (statusOut) {
      statusOut.textContent = fits ? 'Under GitHub 10 MB limit' : 'Over 10 MB · Click Fit under 10 MB';
    }

    fmtBtns.forEach((b) => b.setAttribute('aria-checked', String(b.dataset.eeFormat === state.format)));
  }

  fmtBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      state.format = btn.dataset.eeFormat === 'mp4' ? 'mp4' : 'gif';
      if (state.format === 'mp4') {
        state.width = 2160;
        state.fps = 60;
      } else {
        state.width = 1080;
        state.fps = 15;
      }
      update();
    });
  });

  durInput.addEventListener('input', () => {
    state.seconds = Number(durInput.value) || 14;
    update();
  });

  fitBtn?.addEventListener('click', () => {
    let best: { w: number; f: number; b: number } | null = null;
    for (const w of [...WIDTH_OPTIONS].reverse()) {
      for (const f of [...FPS_OPTIONS].reverse()) {
        if (state.format === 'gif' && f > 30) continue;
        const b = estimateBytes({ ...state, width: w, fps: f });
        if (b <= LIMIT_BYTES) {
          if (!best || w * f > best.w * best.f) {
            best = { w, f, b };
          }
        }
      }
    }
    if (best) {
      state.width = best.w;
      state.fps = best.f;
    } else {
      state.width = WIDTH_OPTIONS[0];
      state.fps = FPS_OPTIONS[0];
    }
    update();
    if (live) {
      live.textContent = `Fitted to ${formatWidth(state.width)} at ${state.fps} fps (${targetMb.toFixed(1)} MB).`;
    }
  });

  update(true);
}
