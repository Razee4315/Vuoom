const CUES: ReadonlyArray<{ time: string; text: string }> = [
  { time: '00:01', text: 'I am using Vuoom to record a quick feature demo.' },
  { time: '00:05', text: 'Every zoom glides into the click with zero keyframes.' },
  { time: '00:09', text: 'Captions run offline on your machine with whisper.cpp.' },
];

export function initAudioPanel(root: HTMLElement, motion: boolean) {
  const canvas = root.querySelector<HTMLCanvasElement>('[data-ap-canvas]');
  const modeBtns = [...root.querySelectorAll<HTMLButtonElement>('[data-ap-mode]')];
  const playBtn = root.querySelector<HTMLButtonElement>('[data-ap-play]');
  const subTime = root.querySelector<HTMLElement>('[data-ap-sub-time]');
  const subText = root.querySelector<HTMLElement>('[data-ap-sub-text]');
  const liveEl = root.querySelector<HTMLElement>('[data-ap-live]');

  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  if (!ctx) return;

  let mode: 'original' | 'clean' = 'clean';
  let playing = false;
  let visible = false;
  let raf = 0;
  let phase = 0;
  let last = 0;
  let cw = 480;
  let ch = 104;
  let suspendTimer: number | undefined;
  let cueTimer: number | undefined;
  let cueIdx = 0;
  let colEmerald = '#4fb37f';
  let colTungsten = '#e4a45f';
  let colNoise = '#e5484d';

  function readColors() {
    const style = getComputedStyle(root);
    colEmerald = style.getPropertyValue('--emerald').trim() || '#4fb37f';
    colTungsten = style.getPropertyValue('--tungsten').trim() || '#e4a45f';
    colNoise = style.getPropertyValue('--rec').trim() || '#e5484d';
  }

  let audioCtx: AudioContext | null = null;
  let humGain: GainNode | null = null;
  let voiceGain: GainNode | null = null;
  let masterGain: GainNode | null = null;
  let osc1: OscillatorNode | null = null;
  let noiseSrc: AudioBufferSourceNode | null = null;

  function setupAudio() {
    if (audioCtx) return;
    const AC = window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
    if (!AC) return;
    audioCtx = new AC();
    masterGain = audioCtx.createGain();
    masterGain.gain.value = 0;
    masterGain.connect(audioCtx.destination);

    const len = audioCtx.sampleRate * 2;
    const buf = audioCtx.createBuffer(1, len, audioCtx.sampleRate);
    const data = buf.getChannelData(0);
    let lp = 0;
    for (let i = 0; i < len; i++) {
      const white = Math.random() * 2 - 1;
      lp = lp * 0.92 + white * 0.08;
      data[i] = lp * 0.45 + Math.sin((i / audioCtx.sampleRate) * 2 * Math.PI * 118) * 0.14;
    }
    noiseSrc = audioCtx.createBufferSource();
    noiseSrc.buffer = buf;
    noiseSrc.loop = true;

    humGain = audioCtx.createGain();
    humGain.gain.value = mode === 'original' ? 0.55 : 0.0;
    noiseSrc.connect(humGain);
    humGain.connect(masterGain);
    noiseSrc.start();

    osc1 = audioCtx.createOscillator();
    osc1.type = 'triangle';
    osc1.frequency.value = 148;
    const filter = audioCtx.createBiquadFilter();
    filter.type = 'lowpass';
    filter.frequency.value = 620;
    voiceGain = audioCtx.createGain();
    voiceGain.gain.value = 0.25;
    osc1.connect(filter);
    filter.connect(voiceGain);
    voiceGain.connect(masterGain);
    osc1.start();
  }

  function updateAudioMix() {
    if (!audioCtx || !humGain || !masterGain) return;
    const now = audioCtx.currentTime;
    humGain.gain.setTargetAtTime(mode === 'original' ? 0.55 : 0.002, now, 0.025);
    masterGain.gain.setTargetAtTime(playing ? 0.18 : 0.0, now, 0.03);
  }

  function measureCanvas() {
    const rect = canvas!.getBoundingClientRect();
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    cw = Math.max(240, Math.round(rect.width));
    ch = Math.max(88, Math.round(rect.height));
    canvas!.width = cw * dpr;
    canvas!.height = ch * dpr;
    ctx!.setTransform(dpr, 0, 0, dpr, 0, 0);
    drawWave();
  }

  const resize = new ResizeObserver(measureCanvas);
  resize.observe(canvas);

  function drawWave() {
    if (!ctx) return;
    ctx.clearRect(0, 0, cw, ch);
    const bars = 48;
    const gap = 3;
    const bw = Math.max(2, (cw - gap * (bars - 1)) / bars);
    const midY = ch / 2;
    const colActive = mode === 'clean' ? colEmerald : colTungsten;

    for (let i = 0; i < bars; i++) {
      const x = i * (bw + gap);
      const t = i / bars;
      const speechEnv =
        Math.max(0.08, Math.sin(t * Math.PI * 3.2 + phase) * 0.55 + Math.cos(t * Math.PI * 7.5 - phase * 1.4) * 0.35);

      let amp: number;
      if (mode === 'original') {
        const jitter = 0.16 + 0.1 * Math.sin(i * 19.3 + phase * 5.2);
        const uneven = t > 0.45 && t < 0.72 ? 0.36 : 1.05;
        amp = Math.min(0.95, Math.max(jitter, speechEnv * uneven + jitter * 0.5));
      } else {
        const leveled = speechEnv < 0.22 ? 0.04 : 0.45 + speechEnv * 0.34;
        amp = Math.min(0.84, leveled);
      }

      const barH = Math.max(4, amp * (ch - 14));
      const y = midY - barH / 2;
      ctx.fillStyle = mode === 'original' && amp < 0.28 ? colNoise : colActive;
      ctx.beginPath();
      ctx.roundRect(x, y, bw, barH, 2);
      ctx.fill();
    }
  }

  function clearCueTimer() {
    if (cueTimer !== undefined) {
      clearTimeout(cueTimer);
      cueTimer = undefined;
    }
  }

  function scheduleCueCycle() {
    clearCueTimer();
    if (!motion || !visible || document.hidden || !subTime || !subText) return;
    cueTimer = window.setTimeout(() => {
      cueTimer = undefined;
      if (!motion || !visible || document.hidden || !subTime || !subText) return;
      cueIdx = (cueIdx + 1) % CUES.length;
      const cue = CUES[cueIdx];
      subTime.textContent = cue.time;
      subText.textContent = cue.text;
      scheduleCueCycle();
    }, 3200);
  }

  function wakeWave() {
    if (!motion || !visible || document.hidden) return;
    if (!raf) {
      last = performance.now();
      raf = requestAnimationFrame(step);
    }
    if (cueTimer === undefined) {
      scheduleCueCycle();
    }
  }

  function stopWave() {
    if (raf) {
      cancelAnimationFrame(raf);
      raf = 0;
    }
    clearCueTimer();
  }

  function step(now: number) {
    raf = 0;
    if (!motion || !visible || document.hidden) return;
    const dt = Math.max(0.001, Math.min((now - last) / 1000, 0.05));
    last = now;
    phase += dt * (playing ? 3.1 : 1.65);

    if (playing && audioCtx && voiceGain && osc1) {
      const syll = 0.15 + 0.25 * Math.max(0, Math.sin(phase * 2.8));
      const level = mode === 'clean' ? 0.28 : syll;
      voiceGain.gain.setTargetAtTime(level, audioCtx.currentTime, 0.04);
      osc1.frequency.setTargetAtTime(142 + Math.sin(phase * 1.7) * 22, audioCtx.currentTime, 0.06);
    }

    drawWave();
    raf = requestAnimationFrame(step);
  }

  function setPlaying(next: boolean) {
    playing = next;
    playBtn?.setAttribute('aria-pressed', String(playing));
    const lbl = playBtn?.querySelector('[data-ap-play-label]');
    if (lbl) lbl.textContent = playing ? 'Pause' : 'Listen';
    if (suspendTimer !== undefined) {
      clearTimeout(suspendTimer);
      suspendTimer = undefined;
    }
    if (playing) {
      setupAudio();
      if (audioCtx?.state === 'suspended') audioCtx.resume().catch(() => {});
      updateAudioMix();
      wakeWave();
    } else {
      updateAudioMix();
      if (audioCtx) {
        suspendTimer = window.setTimeout(() => {
          suspendTimer = undefined;
          if (!playing && audioCtx?.state === 'running') {
            audioCtx.suspend().catch(() => {});
          }
        }, 140);
      }
    }
  }

  playBtn?.addEventListener('click', () => setPlaying(!playing));

  modeBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      mode = btn.dataset.apMode === 'original' ? 'original' : 'clean';
      modeBtns.forEach((b) => b.setAttribute('aria-checked', String(b === btn)));
      if (liveEl) {
        liveEl.textContent =
          mode === 'clean'
            ? 'Switched to RNNoise clean and auto-levelled voice.'
            : 'Switched to original room microphone.';
      }
      updateAudioMix();
      drawWave();
    });
  });

  new IntersectionObserver(([e]) => {
    visible = e.isIntersecting;
    if (visible) {
      wakeWave();
    } else {
      stopWave();
      if (playing) setPlaying(false);
    }
  }).observe(root);

  document.addEventListener('visibilitychange', () => {
    if (document.hidden) {
      stopWave();
      if (playing) setPlaying(false);
    } else if (visible) {
      wakeWave();
    }
  });

  window.addEventListener('vuoom:theme', () => {
    readColors();
    drawWave();
  });

  readColors();
  measureCanvas();
}
