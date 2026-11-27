// Theme preference, resolution, and live system/focus observers.
// Direction A: Tungsten Darkroom (dark) & Daylight Archival Studio (light), plus neutral studio options. Zero purple.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isMock } from "./bridge";
import { t } from "./i18n";

export type ResolvedThemeId = "tungsten" | "daylight" | "mono-dark" | "mono-light" | "graphite" | "midnight";
export type ThemePrefId = "system" | ResolvedThemeId;

export interface Theme {
  id: ThemePrefId;
  name: string;
}

const RESOLVED_IDS: readonly ResolvedThemeId[] = [
  "tungsten",
  "daylight",
  "mono-dark",
  "mono-light",
  "graphite",
  "midnight",
];

const THEME_BACKDROPS: Record<ResolvedThemeId, string> = {
  tungsten: "#110f0e",
  daylight: "#f4f0e8",
  "mono-dark": "#0b0b0b",
  "mono-light": "#f1f1f1",
  graphite: "#111518",
  midnight: "#090f17",
};

const STORAGE_KEY = "vuoom-theme";
const DEFAULT_THEME: ThemePrefId = "system";

export const THEMES = (): Theme[] => [
  { id: "system", name: t("System") },
  { id: "tungsten", name: t("Tungsten Darkroom") },
  { id: "daylight", name: t("Daylight Archival") },
  { id: "mono-dark", name: t("Mono Dark") },
  { id: "mono-light", name: t("Mono Light") },
  { id: "graphite", name: t("Graphite") },
  { id: "midnight", name: t("Midnight") },
];

/** Map legacy or unknown theme strings onto a valid ThemePrefId ("paper" -> "daylight"). */
export function normalizeThemePref(raw: string | null | undefined): ThemePrefId {
  if (!raw) return DEFAULT_THEME;
  if (raw === "paper") return "daylight";
  if (raw === "system") return "system";
  return (RESOLVED_IDS as readonly string[]).includes(raw) ? (raw as ResolvedThemeId) : DEFAULT_THEME;
}

/** Whether ?theme= explicitly selects a valid theme override in URL search params. */
export function hasThemeQueryOverride(): boolean {
  try {
    const raw = new URLSearchParams(window.location.search).get("theme");
    if (!raw) return false;
    return raw === "system" || raw === "paper" || (RESOLVED_IDS as readonly string[]).includes(raw);
  } catch {
    return false;
  }
}

/** Resolve a preference ("system" or concrete palette id) into the active palette id. */
export function resolveTheme(pref: ThemePrefId, prefersDark?: boolean): ResolvedThemeId {
  if (pref !== "system") return pref;
  const dark = prefersDark ?? (typeof window !== "undefined" && window.matchMedia?.("(prefers-color-scheme: dark)").matches !== false);
  return dark ? "tungsten" : "daylight";
}

function setDomTheme(resolved: ResolvedThemeId): void {
  const isLight = resolved === "daylight" || resolved === "mono-light";
  const root = document.documentElement;
  root.dataset.theme = resolved;
  root.style.colorScheme = isLight ? "light" : "dark";
  const meta = document.querySelector('meta[name="theme-color"]');
  if (meta) meta.setAttribute("content", THEME_BACKDROPS[resolved]);
}

/** Preview a theme preference on the live DOM without mutating localStorage. */
export function previewTheme(id: string): ResolvedThemeId {
  const pref = normalizeThemePref(id);
  const resolved = resolveTheme(pref);
  setDomTheme(resolved);
  return resolved;
}

/** Apply a theme preference, optionally persisting the user preference to localStorage. */
export function applyTheme(id: string, persist = true): ResolvedThemeId {
  const pref = normalizeThemePref(id);
  const resolved = resolveTheme(pref);
  setDomTheme(resolved);
  if (persist) {
    try {
      localStorage.setItem(STORAGE_KEY, pref);
    } catch {
      // storage unavailable, ignore
    }
  }
  return resolved;
}

/** Read the initial theme preference: ?theme= override, persisted choice, or System default. */
export function initialTheme(): ThemePrefId {
  try {
    if (hasThemeQueryOverride()) {
      return normalizeThemePref(new URLSearchParams(window.location.search).get("theme"));
    }
  } catch {
    /* no location.search */
  }
  try {
    return normalizeThemePref(localStorage.getItem(STORAGE_KEY));
  } catch {
    return DEFAULT_THEME;
  }
}

/** Subscribe once to OS dark/light preference changes while theme preference is "system". */
export function watchSystemTheme(onChange: (resolved: ResolvedThemeId) => void): () => void {
  if (typeof window === "undefined" || !window.matchMedia) return () => undefined;
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  const handler = (e: MediaQueryListEvent) => onChange(e.matches ? "tungsten" : "daylight");
  mq.addEventListener("change", handler);
  return () => mq.removeEventListener("change", handler);
}

/** Track native/browser window focus on :root[data-window-focused] for inactive window dimming. */
export function watchWindowFocus(): () => void {
  const root = document.documentElement;
  const setFocused = (focused: boolean) => {
    root.dataset.windowFocused = focused ? "true" : "false";
  };
  setFocused(typeof document === "undefined" ? true : document.hasFocus());

  let disposed = false;
  let unlisten: (() => void) | undefined;
  const attachBrowser = () => {
    const onFocus = () => setFocused(true);
    const onBlur = () => setFocused(false);
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    unlisten = () => {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
    };
  };

  if (!isMock) {
    const appWindow = getCurrentWindow();
    void (async () => {
      try {
        setFocused(await appWindow.isFocused());
        const un = await appWindow.onFocusChanged(({ payload }) => setFocused(payload));
        if (disposed) un();
        else unlisten = un;
      } catch {
        if (!disposed) attachBrowser();
      }
    })();
  } else {
    attachBrowser();
  }

  return () => {
    disposed = true;
    unlisten?.();
  };
}
