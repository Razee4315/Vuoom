import { gsap } from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import type { Keycaps } from './keycaps';
gsap.registerPlugin(ScrollTrigger);

// Native scrolling and pointer. Content stays visible until enhancement is ready.
export async function initMotion(_keys: Keycaps) {
  await document.fonts.ready;
  const media = gsap.matchMedia();
  media.add('(prefers-reduced-motion: no-preference)', () => {
    document.querySelectorAll<HTMLElement>('[data-reveal]').forEach(el => {
      gsap.fromTo(el, { opacity: 0, y: 22 }, {
        opacity: 1, y: 0, duration: 0.65, ease: 'power2.out',
        scrollTrigger: { trigger: el, start: 'top 96%', once: true },
        clearProps: 'opacity,transform',
      });
    });
    ScrollTrigger.refresh();
  });
  window.addEventListener('vuoom:layout', () => ScrollTrigger.refresh());
  window.addEventListener('load', () => ScrollTrigger.refresh(), { once: true });
}
