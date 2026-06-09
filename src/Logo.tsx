import type { JSX } from "solid-js";

/**
 * The Vuoom mark: a bold rounded "V" whose right tip is a recording dot.
 * Drawn inline so it inherits the theme color via `currentColor`.
 */
export function LogoMark(props: { size?: number }): JSX.Element {
  const size = () => props.size ?? 20;
  return (
    <svg
      width={size()}
      height={size()}
      viewBox="0 0 32 32"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M6.5 8 L16 25.5 L25.5 8"
        stroke="currentColor"
        stroke-width="5"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
      <circle cx="25.5" cy="8" r="4.6" fill="#e5484d" />
    </svg>
  );
}

/** Mark + wordmark, for the titlebar. */
export function LogoWordmark(): JSX.Element {
  return (
    <span class="brand" data-tauri-drag-region="">
      <LogoMark size={17} />
      <span class="brand-name">Vuoom</span>
    </span>
  );
}
