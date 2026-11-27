import type { JSX } from "solid-js";

type ArtProps = { class?: string };

const decorative = { "aria-hidden": "true", focusable: "false" } as const;

export function ContactSheetArt(props: ArtProps): JSX.Element {
  return (
    <svg
      class={props.class}
      width={144}
      height={80}
      viewBox="0 0 144 80"
      {...decorative}
    >
      <g fill="var(--panel)" stroke="var(--muted)" stroke-width="1.5" stroke-linejoin="round">
        <rect x="23" y="9" width="78" height="46" rx="5" />
        <rect x="32" y="17" width="78" height="46" rx="5" />
        <path d="M46 25h63l10 10v31a5 5 0 0 1-5 5H46a5 5 0 0 1-5-5V30a5 5 0 0 1 5-5Z" />
        <path d="M51 42v-7h9m48 19v7h-9" fill="none" stroke-linecap="round" />
      </g>
      <circle cx="104" cy="36" r="2.75" fill="#e5484d" />
    </svg>
  );
}

export function SelectionHandlesArt(props: ArtProps): JSX.Element {
  return (
    <svg
      class={props.class}
      width={80}
      height={56}
      viewBox="0 0 80 56"
      {...decorative}
      fill="var(--panel)"
      stroke="var(--muted)"
      stroke-width="1.5"
      stroke-linejoin="round"
    >
      <rect x="13" y="10" width="45" height="29" fill="none" />
      <rect x="10" y="7" width="6" height="6" />
      <rect x="55" y="36" width="6" height="6" />
      <path d="m35 24 15 10-7 1 4 9-4 2-4-10-4 5Z" stroke="var(--ink)" />
    </svg>
  );
}

export function StudioEmblem(props: ArtProps): JSX.Element {
  return (
    <svg
      class={props.class}
      width={64}
      height={64}
      viewBox="0 0 64 64"
      {...decorative}
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <rect x="4.5" y="4.5" width="55" height="55" rx="13" fill="var(--panel-2)" stroke="var(--line-strong)" />
      <path d="M18 6h28" stroke="var(--rim, var(--line-strong))" />
      <g transform="translate(11 10) scale(1.3125)">
        <path d="m6.5 8 9.5 17.5L25.5 8" fill="none" stroke="var(--line-strong)" stroke-width="5" />
        <circle cx="25.5" cy="8" r="4.6" fill="#e5484d" />
      </g>
    </svg>
  );
}

export function WindowOutlineArt(props: ArtProps): JSX.Element {
  return (
    <svg
      class={props.class}
      width={96}
      height={64}
      viewBox="0 0 96 64"
      {...decorative}
      fill="var(--panel)"
      stroke="var(--muted)"
      stroke-width="1.5"
      stroke-linecap="round"
    >
      <rect x="10" y="7" width="60" height="39" rx="5" />
      <path d="M10 17h60" />
      <rect x="25" y="18" width="61" height="39" rx="5" />
      <path d="M25 28h61" />
      <g fill="var(--muted)" stroke="none">
        <circle cx="32" cy="23" r="1.25" />
        <circle cx="37" cy="23" r="1.25" />
      </g>
    </svg>
  );
}
