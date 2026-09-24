// Keycaps (M17) press when the matching shortcut fires, from the stage or the keyboard.
export interface Keycaps {
  press(): void;
}

const NAME: Record<string, string> = { control: 'ctrl', shift: 'shift' };

export function initKeycaps(): Keycaps {
  const press = () => {
    document.querySelectorAll<HTMLElement>('.hero__keycaps').forEach((set) => {
      set.querySelectorAll<HTMLElement>('.key').forEach((k, i) => {
        setTimeout(() => k.classList.add('is-down'), i * 40);
        setTimeout(() => k.classList.remove('is-down'), 260 + i * 40);
      });
    });
  };
  window.addEventListener('keydown', (e) => {
    const key = e.key.toLowerCase();
    const name = NAME[key] ?? key;
    document
      .querySelectorAll<HTMLElement>(`.key[data-key="${CSS.escape(name)}"]`)
      .forEach((k) => k.classList.add('is-down'));
  });
  window.addEventListener('keyup', () => {
    document
      .querySelectorAll<HTMLElement>('.key.is-down')
      .forEach((k) => k.classList.remove('is-down'));
  });
  return { press };
}
