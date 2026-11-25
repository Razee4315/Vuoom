import { gsap } from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
gsap.registerPlugin(ScrollTrigger);

// Choreography scoped strictly inside gsap.matchMedia(). Content remains visible under reduced motion or no-JS.
export async function initMotion() {
  await document.fonts.ready;
  const media = gsap.matchMedia();

  media.add('(prefers-reduced-motion: no-preference)', () => {
    // 1. Hero entrance stagger
    const heroEls = [...document.querySelectorAll<HTMLElement>('[data-hero-anim]')];
    if (heroEls.length) {
      gsap.from(heroEls, {
        opacity: 0,
        y: 22,
        duration: 0.78,
        stagger: 0.08,
        ease: 'power3.out',
        clearProps: 'opacity,transform',
      });
    }

    const heroOptics = [...document.querySelectorAll<HTMLElement>('.pg-optic')];
    if (heroOptics.length) {
      gsap.from(heroOptics, {
        opacity: 0,
        y: 26,
        scale: 0.94,
        duration: 0.95,
        delay: 0.22,
        stagger: 0.14,
        ease: 'power3.out',
        clearProps: 'opacity,scale',
      });
    }

    // 2. Silky scroll-triggered reveals
    const revealEls = [...document.querySelectorAll<HTMLElement>('[data-reveal]')];
    revealEls.forEach((el) => {
      if (el.getBoundingClientRect().top < window.innerHeight * 0.92) return;
      gsap.fromTo(
        el,
        { opacity: 0, y: 26 },
        {
          opacity: 1,
          y: 0,
          duration: 0.76,
          ease: 'power3.out',
          scrollTrigger: { trigger: el, start: 'top 90%', once: true },
          clearProps: 'opacity,transform',
        },
      );
    });

    // 3. Staggered metric/card groups
    const staggerGroups = [...document.querySelectorAll<HTMLElement>('[data-reveal-stagger]')];
    staggerGroups.forEach((grp) => {
      const items = [...grp.children].filter((c): c is HTMLElement => c instanceof HTMLElement);
      if (!items.length || grp.getBoundingClientRect().top < window.innerHeight * 0.92) return;
      gsap.fromTo(
        items,
        { opacity: 0, y: 22 },
        {
          opacity: 1,
          y: 0,
          duration: 0.68,
          stagger: 0.08,
          ease: 'power3.out',
          scrollTrigger: { trigger: grp, start: 'top 90%', once: true },
          clearProps: 'opacity,transform',
        },
      );
    });

    // 4. Subtle scrubbed depth parallax on artwork & backdrops
    const parallaxEls = [...document.querySelectorAll<HTMLElement>('[data-parallax-scroll]')];
    parallaxEls.forEach((el) => {
      const isPlate = el.classList.contains('pg-hero__plates') || el.classList.contains('close-bg');
      gsap.fromTo(
        el,
        isPlate ? { yPercent: -3 } : { y: 14 },
        {
          ...(isPlate ? { yPercent: 3 } : { y: -14 }),
          ease: 'none',
          scrollTrigger: {
            trigger: el.closest('section') ?? el,
            start: 'top bottom',
            end: 'bottom top',
            scrub: 0.7,
          },
        },
      );
    });

    ScrollTrigger.refresh();
  });

  window.addEventListener('vuoom:layout', () => ScrollTrigger.refresh());
  window.addEventListener('load', () => ScrollTrigger.refresh(), { once: true });
}
