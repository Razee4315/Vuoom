// What platform the interface runs on, and the keyboard words that differ.
//
// The interface is written with Windows words: "Ctrl+S", "Alt". On a Mac the same shortcut
// is Command, so `mod(e)` reads Command there, and every key label the user sees goes
// through `keyCaps` / `keyText`, which spell it the Mac way (⌘S, ⇧⌘Z). Real Control on a
// Mac (only ever in a hotkey the user binds) is written "Control" to keep the two apart.
// `?os=mac` / `?os=windows` force a platform in the browser mock.

const forced = new URLSearchParams(window.location.search).get("os");

/** Whether the interface runs on macOS. */
export const isMac: boolean =
  forced === "mac" ||
  (forced !== "windows" && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent));

/** Whether the platform's main shortcut modifier is held: Command on a Mac, else Ctrl. */
export function mod(e: KeyboardEvent | MouseEvent | WheelEvent): boolean {
  return isMac ? e.metaKey : e.ctrlKey;
}

/** The Mac glyph for a modifier word, or `undefined` for a key. */
function macGlyph(word: string): string | undefined {
  switch (word.toLowerCase()) {
    case "ctrl":
    case "cmd":
    case "command":
    case "meta":
      return "⌘";
    case "control":
      return "⌃";
    case "alt":
    case "opt":
    case "option":
      return "⌥";
    case "shift":
      return "⇧";
    default:
      return undefined;
  }
}

/** Apple's order for modifiers. */
const MAC_ORDER = ["⌃", "⌥", "⇧", "⌘"];

/** The key names a Mac keyboard prints differently. */
function macKey(word: string): string {
  switch (word) {
    case "Enter":
      return "Return";
    case "Delete":
      return "⌦";
    case "Backspace":
      return "Delete";
    default:
      return word;
  }
}

/**
 * The key caps for a chord written like "Ctrl+Shift+Z": one entry per cap, in the platform's
 * own order and words. On Windows it is just the chord split up.
 */
export function keyCaps(chord: string): string[] {
  const parts = chord.split("+").filter((p) => p !== "");
  if (!isMac) return parts;
  const mods: string[] = [];
  const keys: string[] = [];
  for (const p of parts) {
    const g = macGlyph(p);
    if (g) mods.push(g);
    else keys.push(macKey(p));
  }
  mods.sort((a, b) => MAC_ORDER.indexOf(a) - MAC_ORDER.indexOf(b));
  return [...mods, ...keys];
}

/** A chord as one piece of text: "Ctrl+Shift+Z" on Windows, "⇧⌘Z" on a Mac. */
export function keyText(chord: string): string {
  if (!isMac) return chord;
  const caps = keyCaps(chord);
  const mods = caps.filter((c) => MAC_ORDER.includes(c)).join("");
  const rest = caps.filter((c) => !MAC_ORDER.includes(c)).join(" ");
  return rest.length > 1 && mods ? `${mods} ${rest}` : mods + rest;
}

/** A chord spelled out with modifiers, like "Ctrl+E", inside a sentence. */
const CHORD_IN_TEXT = /\b(?:(?:Ctrl|Cmd|Control|Shift|Alt|Opt)\+)+(?:[A-Z0-9]\b|F\d{1,2}\b|[\][,.=\-/]|scroll\b|wheel\b|滚轮)/g;

/** A sentence with every chord in it spelled the platform's way (a no-op on Windows). */
export function platformText(text: string): string {
  return isMac ? text.replace(CHORD_IN_TEXT, (chord) => keyText(chord)) : text;
}
