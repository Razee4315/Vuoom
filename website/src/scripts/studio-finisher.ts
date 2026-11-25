export function initStudioFinisher(root: HTMLElement) {
  const stage = root.querySelector<HTMLElement>('[data-sf-stage]');
  const frameBtns = [...root.querySelectorAll<HTMLButtonElement>('[data-sf-frame]')];
  const backdropBtns = [...root.querySelectorAll<HTMLButtonElement>('[data-sf-backdrop]')];

  if (!stage) return;

  function setFrame(val: string) {
    stage!.dataset.frame = val;
    frameBtns.forEach((b) => b.setAttribute('aria-checked', String(b.dataset.sfFrame === val)));
  }

  function setBackdrop(val: string) {
    stage!.dataset.backdrop = val;
    backdropBtns.forEach((b) => b.setAttribute('aria-checked', String(b.dataset.sfBackdrop === val)));
  }

  frameBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      const val = btn.dataset.sfFrame;
      if (val) setFrame(val);
    });
  });

  backdropBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      const val = btn.dataset.sfBackdrop;
      if (val) setBackdrop(val);
    });
  });
}
