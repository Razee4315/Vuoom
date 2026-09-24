// The Vuoom icon set: one 24-unit grid, 1.75 stroke, round caps, currentColor. Every glyph
// in the interface comes from here so weights and metrics never drift between screens.
import type { JSX } from "solid-js";

const P: Record<string, string> = {
  undo: "M8.5 5L4 9.5 8.5 14M4 9.5h10a6 6 0 0 1 0 12h-3",
  redo: "M15.5 5L20 9.5 15.5 14M20 9.5H10a6 6 0 0 0 0 12h3",
  folder:
    "M3 8V6a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v2M3 8h17.2a1 1 0 0 1 .97 1.24l-2 8a1 1 0 0 1-.97.76H4a1 1 0 0 1-1-1z",
  save: "M5 3h11l5 5v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zM8 3v5h8V3M7 21v-7h10v7",
  export: "M12 3v12m0 0l-4.5-4.5M12 15l4.5-4.5M4 21h16",
  download: "M12 3v10m0 0l-4-4m4 4l4-4M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2",
  keyboard: "M2 6h20v12H2zM6 10h.01M10 10h.01M14 10h.01M18 10h.01M6 14h.01M18 14h.01M9.5 14h5",
  settings:
    "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1.08-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1.08 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9c.26.6.85 1 1.51 1H21a2 2 0 1 1 0 4h-.09c-.66 0-1.25.4-1.51 1z",
  search: "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM20 20l-4-4",
  close: "M6 6l12 12M18 6L6 18",
  check: "M4.5 12.5l5 5 10-11",
  plus: "M12 5v14M5 12h14",
  minus: "M5 12h14",
  chevronDown: "M6 9l6 6 6-6",
  chevronRight: "M9 6l6 6-6 6",
  chevronLeft: "M15 6l-6 6 6 6",
  chevronUp: "M6 15l6-6 6 6",
  play: "M8 5.6v12.8a.9.9 0 0 0 1.38.76l10.1-6.4a.9.9 0 0 0 0-1.52l-10.1-6.4A.9.9 0 0 0 8 5.6z",
  pause: "M6 5h4.4v14H6zM13.6 5H18v14h-4.4z",
  toStart: "M6 5h2.5v14H6zM20 5.8v12.4a.8.8 0 0 1-1.25.66L9.6 12.66a.8.8 0 0 1 0-1.32l9.15-6.2A.8.8 0 0 1 20 5.8z",
  toEnd: "M18 5h-2.5v14H18zM4 5.8v12.4a.8.8 0 0 0 1.25.66l9.15-6.2a.8.8 0 0 0 0-1.32L5.25 5.14A.8.8 0 0 0 4 5.8z",
  loop: "M17 2l4 4-4 4M3 11v-1a4 4 0 0 1 4-4h14M7 22l-4-4 4-4M21 13v1a4 4 0 0 1-4 4H3",
  zoomIn: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13zM15.5 15.5L21 21M10.5 7.5v6M7.5 10.5h6",
  mic: "M12 3a3 3 0 0 1 3 3v6a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3zM5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21M9 21h6",
  micOff:
    "M15 9.4V6a3 3 0 0 0-5.7-1.3M9 9v3a3 3 0 0 0 4.9 2.3M18.5 11a6.5 6.5 0 0 1-1 3.4M5.5 11a6.5 6.5 0 0 0 10.2 5.4M12 17.5V21M9 21h6M3 3l18 18",
  volume: "M4 9.5h3.5L12 5.5v13l-4.5-4H4zM15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11",
  volumeOff: "M4 9.5h3.5L12 5.5v13l-4.5-4H4zM16 9.5l5 5M21 9.5l-5 5",
  camera: "M4 7h9.5a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V9a2 2 0 0 1 2-2zM15.5 10.5l5.5-3.5v10l-5.5-3.5",
  cameraOff:
    "M9 7h4.5a2 2 0 0 1 2 2v2.5M15.5 15a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V9a2 2 0 0 1 1.3-1.9M15.5 10.5l5.5-3.5v10l-3-1.9M3 3l18 18",
  waves: "M3 12h1.5M6.5 9v6M10 5.5v13M13.5 8v8M17 10v4M20.5 12h.01",
  zoomOut: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13zM15.5 15.5L21 21M7.5 10.5h6",
  speed: "M5 5l7 7-7 7M13 5l7 7-7 7",
  cut: "M6 8.6a2.6 2.6 0 1 0 0-5.2 2.6 2.6 0 0 0 0 5.2zM6 20.6a2.6 2.6 0 1 0 0-5.2 2.6 2.6 0 0 0 0 5.2zM8.1 7.8L20 19M8.1 16.2L20 5",
  sparkle:
    "M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9zM19 15l.9 2.1L22 18l-2.1.9L19 21l-.9-2.1L16 18l2.1-.9z",
  clicks: "M12 14.4a2.4 2.4 0 1 0 0-4.8 2.4 2.4 0 0 0 0 4.8zM12 18.5a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13zM12 1.8v2.4M12 19.8v2.4M1.8 12h2.4M19.8 12h2.4",
  keys: "M2.5 6h19v12h-19zM6.5 10h0M10.3 10h0M14.1 10h0M17.7 10h0M7.5 14h9",
  frame: "M3 3h18v18H3zM3 15h18M8.5 10.6a1.6 1.6 0 1 0 0-3.2 1.6 1.6 0 0 0 0 3.2zM5 21l7-6 5 4 4-3",
  crop: "M6 2v14a2 2 0 0 0 2 2h14M18 22V8a2 2 0 0 0-2-2H2",
  layers: "M12 2l10 5-10 5L2 7zM2 12l10 5 10-5M2 17l10 5 10-5",
  panelLeft: "M3 4h18v16H3zM9 4v16",
  panelRight: "M3 4h18v16H3zM15 4v16",
  panelBottom: "M3 4h18v16H3zM3 15h18",
  monitor: "M2 4h20v13H2zM8 21h8M12 17v4",
  window: "M3 5h18v14H3zM3 9h18",
  region: "M4 8V4h4M16 4h4v4M20 16v4h-4M8 20H4v-4M9 9h6v6H9z",
  fullscreen: "M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5",
  history: "M3 12a9 9 0 1 0 3-6.7M3 4v4h4M12 7v5l3.5 2",
  trash: "M4 7h16M10 11v6M14 11v6M5 7l1 12a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2l1-12M9 7V4h6v3",
  copy: "M9 9h11v11H9zM5 15H4V4h11v1",
  duplicate: "M8 8h12v12H8zM4 16V4h12M14 11v6M11 14h6",
  bringForward: "M4 10h10v10H4zM10 4h10v10",
  sendBackward: "M10 4h10v10H10zM4 10h10v10H4z",
  eye: "M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
  eyeOff: "M3 3l18 18M10.6 5.1A10.8 10.8 0 0 1 12 5c6.4 0 10 7 10 7a17.6 17.6 0 0 1-3.2 4.2M6.6 6.6C3.8 8.4 2 12 2 12s3.6 7 10 7a10 10 0 0 0 5.4-1.6M9.9 9.9a3 3 0 0 0 4.2 4.2",
  magnet: "M6 3v8a6 6 0 0 0 12 0V3h-4v8a2 2 0 0 1-4 0V3zM6 7h4M14 7h4",
  info: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 11v5M12 7.5h.01",
  warning: "M12 3L2.5 20h19L12 3zm0 7v4m0 3.5h.01",
  command: "M9 6v12M15 6v12M6 9h12M6 15h12M6 9a3 3 0 1 1 3-3M18 9a3 3 0 1 0-3-3M6 15a3 3 0 1 0 3 3M18 15a3 3 0 1 1-3 3",
  sun: "M12 17a5 5 0 1 0 0-10 5 5 0 0 0 0 10zM12 1v2M12 21v2M4.2 4.2l1.4 1.4M18.4 18.4l1.4 1.4M1 12h2M21 12h2M4.2 19.8l1.4-1.4M18.4 5.6l1.4-1.4",
  palette: "M12 22a10 10 0 1 1 10-10c0 2.8-2.2 4-4 4h-2a2 2 0 0 0-1.5 3.3A1.6 1.6 0 0 1 12 22zM7.5 11h.01M10 7h.01M14 7h.01M16.5 11h.01",
  pin: "M12 17v5M9 3h6l-1 6 3 3v2H7v-2l3-3z",
  target: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 16a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM12 12h.01",
  cursor: "M5 3l6 15.5 2.3-6.2 6.2-2.3z",
  text: "M5 7V5h14v2M12 5v14M9 19h6",
  shape: "M4 6h16v12H4z",
  arrow: "M6 18L18 6M10.5 6H18v7.5",
  line: "M6 18L18 6",
  highlight: "M9 20l-4.4.9.9-4.4L15.4 5.6a1.6 1.6 0 0 1 2.3 0l.7.7a1.6 1.6 0 0 1 0 2.3zM4 22h10",
  mask: "M4 4h16v16H4zM5 19L19 5",
  lock: "M5 11h14v9H5zM8 11V7a4 4 0 0 1 8 0v4",
  unlock: "M5 11h14v9H5zM8 11V7a4 4 0 0 1 7.5-2",
  sliders: "M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6",
  wand: "M15 4V2M15 16v-2M8 9h2M20 9h2M17.8 11.8L19 13M17.8 6.2L19 5M3 21l9-9M12.2 6.2L11 5",
  timer: "M12 22a8 8 0 1 0 0-16 8 8 0 0 0 0 16zM12 10v4l2 2M9 2h6",
  film: "M3 3h18v18H3zM7 3v18M17 3v18M3 7.5h4M3 12h18M3 16.5h4M17 7.5h4M17 16.5h4",
  gif: "M3 5h18v14H3zM10 10H8a1 1 0 0 0-1 1v2a1 1 0 0 0 1 1h2v-2M13 10v4M16 14v-4h2.5M16 12h2",
  image: "M3 3h18v18H3zM8.5 10a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3zM21 15l-5-5L5 21",
  grid: "M3 3h7v7H3zM14 3h7v7h-7zM14 14h7v7h-7zM3 14h7v7H3z",
  list: "M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01",
  more: "M12 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2zM19 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2zM5 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2z",
  external: "M15 3h6v6M10 14L21 3M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6",
  reset: "M3 12a9 9 0 1 0 2.6-6.4L3 8M3 3v5h5",
  drive: "M22 12H2M5.5 5.1L2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.5-6.9A2 2 0 0 0 16.8 4H7.2a2 2 0 0 0-1.7 1.1zM6 16h.01M10 16h.01",
  bolt: "M13 2L3 14h9l-1 8 10-12h-9z",
  compact: "M4 9h16M4 15h16",
  grow: "M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7",
  shrink: "M4 14h6v6M20 10h-6V4M14 10l7-7M3 21l7-7",
  grip: "M9 6h.01M15 6h.01M9 12h.01M15 12h.01M9 18h.01M15 18h.01",
  expand: "M4 6h16M4 12h16M4 18h16",
};

export type IconName = keyof typeof P;

/** A stroked glyph from the Vuoom set. `fill` switches solid glyphs (play/pause) to fills. */
export function Icon(props: {
  name: IconName;
  size?: number;
  stroke?: number;
  fill?: boolean;
  class?: string;
}): JSX.Element {
  const size = () => props.size ?? 16;
  const solid = () => props.fill ?? ["play", "pause", "toStart", "toEnd"].includes(props.name);
  return (
    <svg
      class={props.class}
      width={size()}
      height={size()}
      viewBox="0 0 24 24"
      fill={solid() ? "currentColor" : "none"}
      stroke={solid() ? "none" : "currentColor"}
      stroke-width={props.stroke ?? 1.75}
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <path d={P[props.name]} />
    </svg>
  );
}
