// Header states (scrolled, compact) and the mobile menu sheet with a focus trap.
const MENU_ICON =
  '<span class="visually-hidden">Menu</span><svg width="22" height="22" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"><path d="M3.5 7h13M3.5 13h13"/></svg>';
const CLOSE_ICON =
  '<span class="visually-hidden">Close menu</span><svg width="22" height="22" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"><path d="m5 5 10 10M15 5 5 15"/></svg>';

export function initNav() {
  const nav = document.querySelector<HTMLElement>('[data-nav]');
  if (!nav) return;

  const onScroll = () => {
    const y = window.scrollY;
    nav.classList.toggle('is-scrolled', y > 8);

  };
  onScroll();
  window.addEventListener('scroll', onScroll, { passive: true });

  const btn = nav.querySelector<HTMLButtonElement>('[data-menu-toggle]');
  const sheet = nav.querySelector<HTMLElement>('[data-menu]');
  if (!btn || !sheet) return;
  const html = document.documentElement;

  const focusables = () =>
    [...sheet.querySelectorAll<HTMLElement>('a, button'), btn].filter(
      (el) => el.offsetParent !== null,
    );

  const setOpen = (open: boolean) => {
    btn.setAttribute('aria-expanded', String(open));
    sheet.hidden = !open;
    html.classList.toggle('menu-open', open);
    btn.innerHTML = open ? CLOSE_ICON : MENU_ICON;
    window.dispatchEvent(new CustomEvent(open ? 'vuoom:lock' : 'vuoom:unlock'));
    if (open) sheet.querySelector<HTMLElement>('a')?.focus();
    else btn.focus();
  };

  btn.addEventListener('click', () => setOpen(btn.getAttribute('aria-expanded') !== 'true'));
  document.addEventListener('keydown', (e) => {
    if (sheet.hidden) return;
    if (e.key === 'Escape') setOpen(false);
    if (e.key === 'Tab') {
      const list = focusables();
      const first = list[0];
      const lastEl = list[list.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        lastEl.focus();
      } else if (!e.shiftKey && document.activeElement === lastEl) {
        e.preventDefault();
        first.focus();
      }
    }
  });
  sheet.addEventListener('click', (e) => {
    if ((e.target as HTMLElement).closest('a')) setOpen(false);
  });
  window.matchMedia('(min-width: 961px)').addEventListener('change', (e) => {
    if (e.matches && !sheet.hidden) setOpen(false);
  });
}
