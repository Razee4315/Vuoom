// Theme definitions. Black & white first, plus a few neutral options. No purple.

export interface Theme {
  id: string;
  name: string;
}

export const THEMES: Theme[] = [
  { id: "mono-dark", name: "Mono Dark" },
  { id: "mono-light", name: "Mono Light" },
  { id: "graphite", name: "Graphite" },
  { id: "paper", name: "Paper" },
  { id: "midnight", name: "Midnight" },
];

const STORAGE_KEY = "vuoom-theme";
const DEFAULT_THEME = "mono-dark";

/** Apply a theme by id and persist the choice. */
export function applyTheme(id: string): void {
  document.documentElement.dataset.theme = id;
  try {
    localStorage.setItem(STORAGE_KEY, id);
  } catch {
    // storage unavailable — ignore
  }
}

/** The persisted theme, or the default. */
export function initialTheme(): string {
  try {
    return localStorage.getItem(STORAGE_KEY) ?? DEFAULT_THEME;
  } catch {
    return DEFAULT_THEME;
  }
}
