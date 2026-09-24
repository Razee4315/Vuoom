// The choices for a new recording's frame rate, zoom and countdown. Home, the record HUD
// and Settings all offer these same sets, so a choice made in one always shows as selected
// in the others.
import { prefs } from "./prefs";

export interface Choice {
  value: number;
  label: string;
  tip?: string;
}

export const FPS_CHOICES: Choice[] = [
  { value: 24, label: "24 fps", tip: "Film-like motion, the smallest files" },
  { value: 30, label: "30 fps", tip: "Lean files, ideal for GIFs" },
  { value: 60, label: "60 fps", tip: "Silky motion for MP4" },
];

/** Zoom strength for Ctrl+Shift+Z while recording (1 = zoom off). */
export const ZOOM_CHOICES: Choice[] = [
  { value: 1, label: "Off" },
  { value: 1.5, label: "1.5×" },
  { value: 1.8, label: "1.8×" },
  { value: 2.5, label: "2.5×" },
  { value: 3, label: "3×" },
];

export const COUNTDOWN_CHOICES: Choice[] = [
  { value: 0, label: "None" },
  { value: 3, label: "3s" },
  { value: 5, label: "5s" },
  { value: 10, label: "10s" },
];

function nearest(choices: Choice[], v: number): number {
  return choices.reduce((best, c) => (Math.abs(c.value - v) < Math.abs(best - v) ? c.value : best), choices[0].value);
}

/** Move a stored value that an older build offered (say a 2× zoom) to the nearest choice
 *  still on offer, so every picker shows a selection. */
export function snapRecordPrefs(): void {
  for (const [pref, choices] of [
    [prefs.captureFps, FPS_CHOICES],
    [prefs.recordZoom, ZOOM_CHOICES],
    [prefs.countdown, COUNTDOWN_CHOICES],
  ] as const) {
    const v = pref();
    if (!choices.some((c) => c.value === v)) pref.set(nearest(choices, v));
  }
}
