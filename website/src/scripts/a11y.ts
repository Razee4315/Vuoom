// Accessible roving tabindex and arrow-key navigation for radiogroups and tablists.
export function initKeyboardGroups(): void {
  const groups = [
    ...document.querySelectorAll<HTMLElement>('[role="radiogroup"]'),
    ...document.querySelectorAll<HTMLElement>('[role="tablist"]'),
  ];
  if (!groups.length) return;

  groups.forEach((group) => {
    const isTablist = group.getAttribute('role') === 'tablist';
    const itemSelector = isTablist ? '[role="tab"]' : '[role="radio"]';
    const activeAttr = isTablist ? 'aria-selected' : 'aria-checked';

    const syncTabIndex = () => {
      const items = [...group.querySelectorAll<HTMLButtonElement>(itemSelector)].filter(
        (el) => !el.disabled,
      );
      if (!items.length) return;
      const active = items.find((el) => el.getAttribute(activeAttr) === 'true') ?? items[0];
      items.forEach((el) => {
        el.tabIndex = el === active ? 0 : -1;
      });
    };

    syncTabIndex();

    const mo = new MutationObserver(syncTabIndex);
    mo.observe(group, {
      subtree: true,
      attributes: true,
      attributeFilter: [activeAttr, 'disabled'],
    });

    group.addEventListener('keydown', (e) => {
      const target = e.target as HTMLElement | null;
      if (!target || !target.matches(itemSelector)) return;
      const items = [...group.querySelectorAll<HTMLButtonElement>(itemSelector)].filter(
        (el) => !el.disabled,
      );
      const idx = items.indexOf(target as HTMLButtonElement);
      if (idx === -1) return;

      let nextIdx = -1;
      if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
        nextIdx = (idx + 1) % items.length;
      } else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
        nextIdx = (idx - 1 + items.length) % items.length;
      } else if (e.key === 'Home') {
        nextIdx = 0;
      } else if (e.key === 'End') {
        nextIdx = items.length - 1;
      } else {
        return;
      }

      e.preventDefault();
      const nextBtn = items[nextIdx];
      nextBtn.focus();
      nextBtn.click();
    });
  });
}
