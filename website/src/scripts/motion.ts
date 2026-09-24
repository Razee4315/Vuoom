// The motion layer (docs/05-motion-spec.md). Loaded only without prefers-reduced-motion.
// One RAF loop: Lenis is driven by the GSAP ticker. Everything lives in gsap.matchMedia so
// it reverts if the visitor switches reduced motion on mid-visit.
import { gsap } from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import { SplitText } from 'gsap/SplitText';
import Lenis from 'lenis';
import { initStage } from './stage';
import { initPointer } from './pointer';
import { camTransform } from './spring';
import type { Keycaps } from './keycaps';

gsap.registerPlugin(ScrollTrigger, SplitText);

const EASE = 'expo.out';
const html = document.documentElement;

export async function initMotion(keys: Keycaps) {
  // ---- M05 Lenis on the GSAP ticker, off on touch ----
  let lenis: Lenis | null = null;
  if (!window.matchMedia('(pointer: coarse)').matches) {
    lenis = new Lenis({ lerp: 0.1, smoothWheel: true, syncTouch: false });
    lenis.on('scroll', ScrollTrigger.update);
    gsap.ticker.add((t) => lenis?.raf(t * 1000));
    gsap.ticker.lagSmoothing(0);
    window.addEventListener('vuoom:lock', () => lenis?.stop());
    window.addEventListener('vuoom:unlock', () => lenis?.start());
    document.querySelectorAll<HTMLAnchorElement>('a[href^="#"]').forEach((a) => {
      a.addEventListener('click', (e) => {
        const id = a.getAttribute('href')!;
        const el = id.length > 1 ? document.querySelector<HTMLElement>(id) : null;
        if (!el) return;
        e.preventDefault();
        lenis?.scrollTo(el, { offset: -80 });
        history.replaceState(null, '', id);
      });
    });
  }

  initPointer();

  const countdown = runCountdown();
  await document.fonts.ready;

  const mm = gsap.matchMedia();
  mm.add('(prefers-reduced-motion: no-preference)', () => {
    // ---- M06 split text: mask lines, aria-label first ----
    const splits = new Map<HTMLElement, SplitText>();
    document.querySelectorAll<HTMLElement>('[data-split]').forEach((el) => {
      el.setAttribute('aria-label', el.textContent?.trim() ?? '');
      const s = SplitText.create(el, { type: 'lines', mask: 'lines', autoSplit: true, linesClass: 'line' });
      s.lines.forEach((l) => l.setAttribute('aria-hidden', 'true'));
      splits.set(el, s);
      gsap.set(el, { visibility: 'visible' });
      if (el.hasAttribute('data-hero-h')) return;
      gsap.from(s.lines, {
        yPercent: 110,
        duration: 1,
        ease: EASE,
        stagger: 0.08,
        scrollTrigger: { trigger: el, start: 'top 85%', once: true },
      });
    });

    // ---- M01/M02 reveals, batched ----
    ScrollTrigger.batch('[data-reveal]', {
      start: 'top 88%',
      once: true,
      onEnter: (els) =>
        gsap.to(els, { opacity: 1, y: 0, duration: 0.8, ease: EASE, stagger: 0.06, overwrite: true }),
    });

    // ---- M09 hero load sequence + M10 stage ----
    const stageEl = document.querySelector<HTMLElement>('[data-stage]');
    const stage = stageEl ? initStage(stageEl, { onZoom: (zin) => zin && keys.press() }) : null;
    const heroH = document.querySelector<HTMLElement>('[data-hero-h]');
    if (heroH) {
      const lines = splits.get(heroH)?.lines ?? [];
      const tl = gsap.timeline({ paused: true, defaults: { ease: EASE } });
      tl.from('[data-nav]', { y: -12, opacity: 0, duration: 0.6 }, 0)
        .from(lines, { yPercent: 110, duration: 1, stagger: 0.08 }, 0.15)
        .from('[data-hero-lead]', { y: 14, opacity: 0, duration: 0.8 }, 0.45)
        .from('[data-hero-cta] > *', { y: 16, opacity: 0, duration: 0.7, stagger: 0.06 }, 0.55)
        .fromTo(
          '.stage__well',
          { clipPath: 'inset(48% 48% 48% 48% round 14px)' },
          { clipPath: 'inset(0% 0% 0% 0% round 14px)', duration: 1.1, clearProps: 'clipPath' },
          0.5,
        )
        .from('.stage__img', { scale: 1.08, duration: 1.4, transformOrigin: '50% 50%' }, 0.5)
        .add(() => stage?.start(), 1.3);
      countdown.then(() => tl.play());

      // ---- M06 hero exit, scroll-linked ----
      gsap.to('[data-hero-stage]', {
        scale: 0.94,
        opacity: 0.5,
        yPercent: -4,
        ease: 'none',
        scrollTrigger: { trigger: '[data-hero]', start: 'top top', end: 'bottom top', scrub: 1 },
      });
    } else {
      stage?.start();
    }

    // ---- M13 parallax ----
    document.querySelectorAll<HTMLElement>('[data-parallax]').forEach((el) => {
      gsap.fromTo(
        el,
        { yPercent: 6 },
        {
          yPercent: -6,
          ease: 'none',
          scrollTrigger: { trigger: el, start: 'top bottom', end: 'bottom top', scrub: 1 },
        },
      );
    });

    // ---- M16 footer wordmark ----
    const mark = document.querySelector<HTMLElement>('[data-wordmark]');
    if (mark) {
      const word = mark.querySelector<HTMLElement>('.foot__word')!;
      const s = SplitText.create(word, { type: 'chars', mask: 'chars' });
      gsap.from(s.chars, {
        yPercent: 100,
        duration: 0.9,
        ease: EASE,
        stagger: 0.03,
        scrollTrigger: { trigger: mark, start: 'top 95%', once: true },
      });
      gsap.from(mark.querySelector('.foot__dot'), {
        scale: 0,
        duration: 0.6,
        ease: EASE,
        delay: 0.35,
        scrollTrigger: { trigger: mark, start: 'top 95%', once: true },
      });
    }

    // ---- oss star count-up (M18) ----
    const big = document.querySelector<HTMLElement>('[data-stars-big]');
    if (big) {
      ScrollTrigger.create({
        trigger: big,
        start: 'top 90%',
        once: true,
        onEnter: () => {
          const n = Number(big.dataset.count);
          if (!n) return;
          const o = { v: 0 };
          gsap.to(o, {
            v: n,
            duration: 0.9,
            ease: EASE,
            onUpdate: () => (big.textContent = String(Math.round(o.v))),
          });
        },
      });
    }

    return () => splits.forEach((s) => s.revert());
  });

  // ---- M07 / M09 pinned sections, only where there is room to pin ----
  mm.add('(prefers-reduced-motion: no-preference) and (min-width: 641px) and (min-height: 560px)', () => {
    problem();
    editor();
  });
  mm.add('(prefers-reduced-motion: no-preference) and (max-width: 640px), (prefers-reduced-motion: no-preference) and (max-height: 559px)', () => {
    // No pin: show the finished states.
    document.querySelector('[data-problem]')?.classList.add('prob--static');
    document.querySelectorAll('.ed__cap').forEach((el) => el.classList.add('is-on'));
    gsap.set('[data-problem-mark]', { opacity: 1 });
    gsap.set('.ed__label, .ed__arrow', { opacity: 1 });
  });

  curtain();
  window.addEventListener('load', () => ScrollTrigger.refresh());
}

// ---- M11 countdown: 3, 2, 1 then the record light. First visit per session only. ----
function runCountdown(): Promise<void> {
  const el = document.querySelector<HTMLElement>('.countdown');
  if (!el || !html.classList.contains('is-counting')) {
    html.classList.remove('is-counting');
    return Promise.resolve();
  }
  const num = el.querySelector<HTMLElement>('.countdown__num')!;
  return new Promise((resolve) => {
    let done = false;
    const finish = () => {
      if (done) return;
      done = true;
      try {
        sessionStorage.setItem('vuoom-take', '1');
      } catch {}
      gsap.to(el, {
        opacity: 0,
        duration: 0.35,
        ease: 'power2.in',
        onComplete: () => {
          html.classList.remove('is-counting');
          gsap.set(el, { clearProps: 'opacity' });
        },
      });
      resolve();
    };
    const tl = gsap.timeline({ onComplete: finish });
    ['3', '2', '1'].forEach((n, i) => {
      tl.call(() => (num.textContent = n), [], i * 0.24).fromTo(
        num,
        { scale: 1.25, opacity: 0 },
        { scale: 1, opacity: 1, duration: 0.2, ease: EASE },
        i * 0.24,
      );
    });
    tl.to({}, { duration: 0.1 });
    const skip = () => {
      tl.kill();
      finish();
    };
    window.addEventListener('keydown', skip, { once: true });
    el.addEventListener('pointerdown', skip, { once: true });
    setTimeout(skip, 1400); // hard cap
  });
}

// ---- M07 The problem ----
function problem() {
  const root = document.querySelector<HTMLElement>('[data-problem]');
  if (!root) return;
  const pin = root.querySelector<HTMLElement>('[data-problem-pin]')!;
  const num = root.querySelector<HTMLElement>('[data-problem-num]')!;
  const tag = root.querySelector<HTMLElement>('[data-problem-tag]')!;
  const steps = [...root.querySelectorAll<HTMLElement>('[data-problem-step]')];
  const VALUES = ['1920', '82', '27', '68'];
  const TAGS = ['1920 × 1080', '1920 × 1080', 'GIF 640 × 360', 'Vuoom 2.5×'];
  let cur = -1;
  const setStep = (i: number) => {
    if (i === cur) return;
    cur = i;
    steps.forEach((s, k) => s.classList.toggle('is-on', k === i));
    tag.textContent = TAGS[i];
    gsap.fromTo(num, { yPercent: 40, opacity: 0 }, { yPercent: 0, opacity: 1, duration: 0.5, ease: EASE });
    num.textContent = VALUES[i];
    num.parentElement!.classList.toggle('red', i === 3);
  };
  setStep(0);

  const tl = gsap.timeline({
    defaults: { ease: 'none' },
    scrollTrigger: {
      trigger: root,
      pin,
      start: 'top top',
      end: '+=260%',
      scrub: 1,
      onUpdate: (self) => setStep(Math.min(3, Math.floor(self.progress * 4.4))),
    },
  });
  tl.to('[data-problem-mark]', { opacity: 1, duration: 0.2 }, 0.25)
    .to('[data-problem-frame]', { scale: 0.42, duration: 0.25, ease: 'power2.inOut' }, 0.47)
    .to('[data-problem-frame]', { scale: 1, duration: 0.2, ease: 'power2.inOut' }, 0.72)
    .to(
      '[data-problem-cam]',
      { scale: 2.5, xPercent: 0, yPercent: -14.75, duration: 0.2, ease: 'power3.inOut' },
      0.72,
    )
    .to('[data-problem-mark]', { opacity: 0, duration: 0.1 }, 0.9)
    .to({}, { duration: 0.1 });
}

// ---- M09 The editor ----
function editor() {
  const root = document.querySelector<HTMLElement>('[data-editor]');
  if (!root) return;
  const DUR = 14.4;
  const pin = root.querySelector<HTMLElement>('[data-editor-pin]')!;
  const cam = root.querySelector<HTMLElement>('[data-editor-cam]')!;
  const preview = cam.parentElement!;
  const blocks = [...root.querySelectorAll<HTMLElement>('[data-editor-block]')].map((el) => ({
    el,
    a: Number(el.dataset.a),
    b: Number(el.dataset.b),
    kind: el.dataset.kind!,
    z: Number(el.dataset.z ?? 1),
    x: Number(el.dataset.x ?? 0.5),
    y: Number(el.dataset.y ?? 0.5),
  }));
  const label = root.querySelector<HTMLElement>('[data-editor-label]')!;
  const arrow = root.querySelector<HTMLElement>('[data-editor-arrow]')!;
  const speed = root.querySelector<HTMLElement>('[data-editor-speed]')!;
  const cut = root.querySelector<HTMLElement>('[data-editor-cut]')!;
  const time = root.querySelector<HTMLElement>('[data-editor-time]')!;
  const len = root.querySelector<HTMLElement>('[data-editor-len]')!;
  const head = root.querySelector<HTMLElement>('[data-editor-playhead]')!;
  const lane = root.querySelector<HTMLElement>('.ed__lane')!;
  const caps = [...root.querySelectorAll<HTMLElement>('[data-editor-cap]')];

  const clamp01 = (v: number) => Math.min(1, Math.max(0, v));
  const smooth = (v: number) => v * v * (3 - 2 * v);
  const RAMP = 0.55;

  const render = (p: number) => {
    const t = p * DUR;
    // blocks draw in behind the playhead
    blocks.forEach((b) => {
      const k = clamp01((t - b.a) / (b.b - b.a));
      b.el.style.transform = `scaleX(${t < b.a ? 0 : Math.max(k, 0.02)})`;
      b.el.style.opacity = t < b.a ? '0' : '1';
    });
    // camera from the zoom blocks
    let z = 1;
    let cx = 0.5;
    let cy = 0.5;
    for (const b of blocks) {
      if (b.kind !== 'zoom') continue;
      const w = smooth(clamp01((t - b.a) / RAMP)) * smooth(clamp01((b.b - t) / RAMP));
      if (w > 0) {
        z = 1 + (b.z - 1) * w;
        cx = 0.5 + (b.x - 0.5) * w;
        cy = 0.5 + (b.y - 0.5) * w;
      }
    }
    cam.style.transform = camTransform(cx, cy, z, preview.clientWidth, preview.clientHeight);
    const on = (kind: string) => blocks.some((b) => b.kind === kind && t >= b.a && t <= b.b);
    label.style.opacity = on('text') ? '1' : '0';
    arrow.style.opacity = on('arrow') ? '1' : '0';
    speed.style.opacity = on('speed') ? '1' : '0';
    cut.style.opacity = on('cut') ? '0.92' : '0';
    // readouts
    time.textContent = `0:${t.toFixed(1).padStart(4, '0')}`;
    const sp = blocks.find((b) => b.kind === 'speed')!;
    const ct = blocks.find((b) => b.kind === 'cut')!;
    const saved =
      clamp01((t - sp.a) / (sp.b - sp.a)) * (sp.b - sp.a) * (2 / 3) +
      clamp01((t - ct.a) / (ct.b - ct.a)) * (ct.b - ct.a);
    len.textContent = `${(DUR - saved).toFixed(1)}s`;
    head.style.transform = `translateX(${p * lane.clientWidth}px)`;
    const ci = t < 4.8 ? 0 : t < 8.6 ? 1 : t < 12.2 ? 2 : 3;
    caps.forEach((c, i) => c.classList.toggle('is-on', i === ci));
  };
  render(0);

  const proxy = { p: 0 };
  gsap.to(proxy, {
    p: 1,
    ease: 'none',
    onUpdate: () => render(proxy.p),
    scrollTrigger: { trigger: root, pin, start: 'top top', end: '+=300%', scrub: 1 },
  });
}

// ---- M15 curtain footer only when it fits the viewport ----
function curtain() {
  const foot = document.querySelector<HTMLElement>('[data-footer]');
  if (!foot) return;
  const fit = () => {
    foot.style.position = foot.offsetHeight <= window.innerHeight ? '' : 'relative';
  };
  fit();
  window.addEventListener('resize', fit);
}
