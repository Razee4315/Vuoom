import { gsap } from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
gsap.registerPlugin(ScrollTrigger);

// Native scrolling and pointer. Content stays visible until enhancement is ready.
export async function initMotion() {
  await document.fonts.ready;
  const media = gsap.matchMedia();
  media.add('(prefers-reduced-motion: no-preference)', () => {
    document.querySelectorAll<HTMLElement>('[data-reveal]').forEach(el => {
      // What the visitor is already looking at stays put: hiding it now, only to fade it
      // back in, reads as a flicker.
      if (el.getBoundingClientRect().top < window.innerHeight) return;
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
