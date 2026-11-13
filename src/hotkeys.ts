// The two hotkeys that work while another app has the keyboard: zoom and stop. Both can be
// rebound (Settings > Shortcuts); every label that names them reads the current chord from
// here, and the engine is told whenever they change.
import { invoke } from "./bridge";
import { t } from "./i18n";
import { prefs } from "./prefs";
import { isMac } from "./platform";

export const ZOOM_DEFAULT = isMac ? "Cmd+Shift+Z" : "Ctrl+Shift+Z";
export const STOP_DEFAULT = isMac ? "Cmd+Shift+X" : "Ctrl+Shift+X";
/** Start recording: handled inside the app's own window, so it isn't rebindable here. */
export const RECORD_KEYS = "Ctrl+Shift+R";

/** The chord that zooms while recording, written like "Ctrl+Shift+Z". */
export const zoomKeys = (): string => prefs.hotkeyZoom();
/** The chord that stops the recording. */
export const stopKeys = (): string => prefs.hotkeyStop();

/** Tell the engine the current chords (it starts out on the defaults). */
export function pushHotkeys(): void {
  void invoke("set_hotkeys", { zoom: zoomKeys(), stop: stopKeys() }).catch(() => undefined);
}

/** The key part of a chord for a pressed key: a letter, a digit or F1 to F12. */
function keyName(code: string): string | null {
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1];
  const digit = /^Digit([0-9])$/.exec(code);
  if (digit) return digit[1];
  return /^F([1-9]|1[0-2])$/.test(code) ? code : null;
}

export type ChordAttempt =
  | { chord: string }
  /** Only modifiers are down so far: keep waiting. */
  | { pending: true }
  | { error: string };

/** The chord a key press spells, or why it can't be a hotkey. Mirrors the engine's rules
 *  (`vuoom_input::Chord::parse`): a letter or digit needs two of Ctrl, Shift and Alt, so a
 *  hotkey can't take over typing or a shortcut every app uses. */
export function chordFromEvent(e: KeyboardEvent): ChordAttempt {
  if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return { pending: true };
  const key = keyName(e.code);
  if (!key) return { error: t("Use a letter, a digit or F1 to F12") };
  // On a Mac, Command is the main modifier and real Control is written out as "Control".
  const mods = (
    isMac
      ? [e.ctrlKey && "Control", e.altKey && "Opt", e.shiftKey && "Shift", e.metaKey && "Cmd"]
      : [e.ctrlKey && "Ctrl", e.shiftKey && "Shift", e.altKey && "Alt"]
  ).filter((m): m is string => !!m);
  if (!key.startsWith("F") || key.length === 1) {
    if (mods.length < 2) {
      return {
        error: isMac
          ? t("Hold two of Command, Control, Option and Shift with it")
          : t("Hold two of Ctrl, Shift and Alt with it"),
      };
    }
  }
  return { chord: [...mods, key].join("+") };
}
