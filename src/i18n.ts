// Interface language. The English text is the key: t("Export failed: {why}", { why }) looks
// the sentence up in the active dictionary, fills its placeholders, and falls back to the
// English when a key is missing. t reads the locale signal, so a call inside JSX follows a
// language change on its own. Only a call that runs once (a module-level constant) can't.
import { createSignal } from "solid-js";
import { prefs } from "./prefs";

export type Locale = "en" | "zh-CN";

export const isLocale = (v: unknown): v is Locale => v === "en" || v === "zh-CN";

const [locale, setActive] = createSignal<Locale>("en");
export { locale };

// Fetched on demand, so English users never pay for the dictionary.
let dict: Record<string, string> = {};
const load = async (l: Locale) => {
  if (l === "zh-CN") dict = (await import("./locales/zh-CN.json")).default;
};

async function apply(l: Locale): Promise<void> {
  // A dictionary that can't be fetched leaves the interface in English.
  await load(l).catch(() => undefined);
  setActive(l);
  document.documentElement.lang = l;
}

/** Switch the interface language and remember the choice. */
export async function setLocale(l: Locale): Promise<void> {
  prefs.language.set(l);
  await apply(l);
}

/** The `?lang=` override, the saved choice, or the system language. Run before first render. */
export function initLocale(): Promise<void> {
  const forced = new URLSearchParams(window.location.search).get("lang");
  const saved = prefs.language();
  if (isLocale(forced)) return apply(forced);
  if (isLocale(saved)) return apply(saved);
  return apply(navigator.language.startsWith("zh") ? "zh-CN" : "en");
}

export function t(text: string, vars?: Record<string, string | number>): string {
  const s = locale() === "en" ? text : (dict[text] ?? text);
  return vars ? s.replace(/\{(\w+)\}/g, (hole, name) => String(vars[name] ?? hole)) : s;
}
