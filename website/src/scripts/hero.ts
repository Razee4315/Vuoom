import { gsap } from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import { springUpdate, type Spring } from './spring';

gsap.registerPlugin(ScrollTrigger);

const clamp = (value: number, min: number, max: number) => Math.max(min, Math.min(max, value));
const spring = (value = 0): Spring => ({ x: value, v: 0 });

export function initHeroSprites(hero: HTMLElement) {
  const elements = [...hero.querySelectorAll<HTMLElement>('[data-hero-sprite]')];
  const buttons = elements.map(element => element.querySelector<HTMLButtonElement>('button')!);
  const media = gsap.matchMedia();
  media.add({ motion: '(prefers-reduced-motion: no-preference)', reduced: '(prefers-reduced-motion: reduce)' }, context => {
    const motion = !!context.conditions?.motion;
    buttons.forEach(button => { button.disabled = !motion; });
    if (!motion) return;

    const events = new AbortController();
    const options = { signal: events.signal };
    const nodes = elements.map((element, index) => ({
      element, button: buttons[index], index, x: spring(), y: spring(), angle: spring(), scale: spring(1),
      centerX: 0, centerY: 0, minX: 0, maxX: 0, visible: true,
      held: false, dragged: false, pointerId: -1, startX: 0, startY: 0,
      grabX: 0, grabY: 0, dragX: 0, dragY: 0, launchAt: -Infinity,
    }));
    let frame = 0;
    let last = 0;
    let visible = false;
    let introAt = -1;
    let pointer: { x: number; y: number } | undefined;

    const measure = () => {
      const bounds = hero.getBoundingClientRect();
      nodes.forEach(node => {
        node.visible = getComputedStyle(node.element).display !== 'none';
        const width = node.element.offsetWidth;
        node.centerX = node.element.offsetLeft + width / 2;
        node.centerY = node.element.offsetTop + node.element.offsetHeight / 2;
        const travel = bounds.width < 760 ? 18 : 32;
        node.minX = Math.max(-travel, 8 - bounds.left - node.element.offsetLeft);
        node.maxX = Math.min(travel, window.innerWidth - 8 - bounds.left - node.element.offsetLeft - width);
      });
    };
    const wake = () => {
      if (frame || !visible || document.hidden) return;
      last = performance.now();
      frame = requestAnimationFrame(draw);
    };
    const draw = (now: number) => {
      frame = 0;
      if (!visible || document.hidden) return;
      const dt = Math.min((now - last) / 1000, .04);
      last = now;
      const intro = clamp((now - introAt) / 4600, 0, 1);
      // A finite greeting: staggered, slow drifts with a soft start and finish.
      const envelope = Math.sin(Math.PI * intro) ** 2;
      let moving = intro < 1;
      nodes.forEach(node => {
        if (!node.visible) return;
        const phase = node.index * 1.1;
        let x = Math.sin(intro * Math.PI * 2 + phase) * 5 * envelope;
        let y = Math.cos(intro * Math.PI * 2 + phase) * 6 * envelope;
        let angle = Math.sin(intro * Math.PI * 2 + phase) * 3 * envelope;
        let scale = 1;
        if (pointer && !node.held) {
          const dx = node.centerX - pointer.x;
          const dy = node.centerY - pointer.y;
          const distance = Math.hypot(dx, dy);
          const proximity = Math.max(0, 1 - distance / 150);
          const strength = proximity * proximity;
          x += (dx / Math.max(distance, 1)) * strength * 20;
          y += (dy / Math.max(distance, 1)) * strength * 16;
          angle += (node.index % 2 ? 1 : -1) * strength * 5;
          scale += strength * .05;
        }
        const flight = clamp((now - node.launchAt) / 1100, 0, 1);
        if (flight < 1) {
          const arc = Math.sin(flight * Math.PI);
          const direction = node.index % 2 ? 1 : -1;
          x += direction * (24 * arc + 5 * Math.sin(flight * Math.PI * 2));
          y -= 36 * arc;
          angle += direction * 22 * arc;
          scale += .1 * arc;
          moving = true;
        }
        if (node.held) {
          x = node.dragX;
          y = node.dragY;
          angle = node.dragX * .3;
          scale = 1.08;
          moving = true;
        }
        x = clamp(x, node.minX, node.maxX);
        y = clamp(y, -40, 24);
        const halfLife = node.held ? .035 : .09;
        const channels: [Spring, number][] = [[node.x, x], [node.y, y], [node.angle, angle], [node.scale, scale]];
        channels.forEach(([state, goal]) => {
          springUpdate(state, goal, halfLife, dt);
          if (Math.abs(state.x - goal) > .01 || Math.abs(state.v) > .05) moving = true;
        });
        node.button.style.transform = 'translate3d(' + node.x.x.toFixed(3) + 'px,' + node.y.x.toFixed(3) + 'px,0) rotate(' + node.angle.x.toFixed(3) + 'deg) scale(' + node.scale.x.toFixed(4) + ')';
      });
      if (moving) frame = requestAnimationFrame(draw);
    };

    nodes.forEach(node => {
      node.button.addEventListener('click', () => {
        if (node.dragged) { node.dragged = false; return; }
        node.launchAt = performance.now();
        wake();
      }, options);
      node.button.addEventListener('pointerdown', event => {
        if (event.button !== 0 || !event.isPrimary) return;
        node.held = true;
        node.dragged = false;
        node.pointerId = event.pointerId;
        node.startX = event.clientX;
        node.startY = event.clientY;
        node.grabX = node.dragX = node.x.x;
        node.grabY = node.dragY = node.y.x;
        node.launchAt = -Infinity;
        node.button.dataset.held = '';
        node.button.setPointerCapture(event.pointerId);
        wake();
      }, options);
      node.button.addEventListener('pointermove', event => {
        if (!node.held || node.pointerId !== event.pointerId) return;
        const dx = event.clientX - node.startX;
        const dy = event.clientY - node.startY;
        if (Math.hypot(dx, dy) > 5) node.dragged = true;
        node.dragX = clamp(node.grabX + dx, node.minX, node.maxX);
        node.dragY = clamp(node.grabY + dy, -40, 24);
        wake();
      }, options);
      const release = (event: PointerEvent) => {
        if (node.pointerId !== event.pointerId) return;
        node.held = false;
        node.pointerId = -1;
        delete node.button.dataset.held;
        if (event.type === 'pointercancel') node.dragged = false;
        if (node.button.hasPointerCapture(event.pointerId)) node.button.releasePointerCapture(event.pointerId);
        wake();
      };
      node.button.addEventListener('pointerup', release, options);
      node.button.addEventListener('pointercancel', release, options);
      node.button.addEventListener('lostpointercapture', release, options);
    });
    hero.addEventListener('pointermove', event => {
      if (event.pointerType !== 'mouse') return;
      const bounds = hero.getBoundingClientRect();
      pointer = { x: event.clientX - bounds.left, y: event.clientY - bounds.top };
      wake();
    }, { ...options, passive: true });
    hero.addEventListener('pointerleave', () => { pointer = undefined; wake(); }, options);
    const stop = () => {
      cancelAnimationFrame(frame);
      frame = 0;
      pointer = undefined;
      nodes.forEach(node => {
        if (node.pointerId !== -1 && node.button.hasPointerCapture(node.pointerId)) node.button.releasePointerCapture(node.pointerId);
        node.held = false;
        node.pointerId = -1;
        node.launchAt = -Infinity;
        [node.x, node.y, node.angle].forEach(state => { state.x = state.v = 0; });
        node.scale.x = 1;
        node.scale.v = 0;
        node.button.style.removeProperty('transform');
        delete node.button.dataset.held;
      });
    };
    document.addEventListener('visibilitychange', () => { if (document.hidden) stop(); else wake(); }, options);
    const observer = new IntersectionObserver(([entry]) => {
      visible = entry.isIntersecting;
      if (visible) { if (introAt < 0) introAt = performance.now(); measure(); wake(); }
      else stop();
    });
    observer.observe(hero);
    const resize = new ResizeObserver(() => { measure(); wake(); });
    resize.observe(hero);
    measure();

    gsap.to(hero.querySelector('.hero-layout'), {
      scale: .9, y: -12, ease: 'none',
      scrollTrigger: { trigger: hero, start: 'top top', end: 'bottom top', scrub: true },
    });
    return () => { events.abort(); observer.disconnect(); resize.disconnect(); stop(); };
  });
}
