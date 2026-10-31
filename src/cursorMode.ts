// The pointer options for new takes, shared by Settings, the record HUD and record start.
import { invoke } from "./bridge";
import { t } from "./i18n";
import { prefs } from "./prefs";
import type { CursorMode } from "./types";

export const CURSOR_MODES = (): {
  value: CursorMode;
  /** Short label (segmented controls). */
  label: string;
  /** Menu entry. */
  menu: string;
  /** Lowercase fragment for the HUD's options summary. */
  summary: string;
}[] => [
  { value: "smooth", label: t("Smooth"), menu: t("Smooth, re-drawn pointer"), summary: t("smooth cursor") },
  { value: "show", label: t("Recorded"), menu: t("The real pointer"), summary: t("cursor") },
  { value: "hide", label: t("Hidden"), menu: t("No pointer"), summary: t("no cursor") },
];

export const cursorSummary = (mode: CursorMode = prefs.cursorMode()) =>
  CURSOR_MODES().find((m) => m.value === mode)?.summary ?? t("cursor");

/** Tell the engine how the next take handles the pointer. The smooth mode hides the real
 *  pointer and draws a clean one from the recorded movements. */
export function pushCursorMode(mode: CursorMode = prefs.cursorMode()): void {
  void invoke("set_capture_cursor", { show: mode === "show", smooth: mode === "smooth" }).catch(
    () => undefined,
  );
}
