// Backend bridge. The app code imports invoke/listen/dialog helpers from here instead of
// from the Tauri packages directly; the bridge dispatches to the real Tauri runtime when
// present and to the browser mock engine otherwise (see mock/engine.ts). This keeps every
// screen developable, testable and screenshot-able in a plain browser without the native
// backend, while the packaged app path stays untouched.

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import { save as tauriSave, open as tauriOpen, ask as tauriAsk } from "@tauri-apps/plugin-dialog";
import { check as tauriCheck, type Update } from "@tauri-apps/plugin-updater";
import { relaunch as tauriRelaunch } from "@tauri-apps/plugin-process";
import { revealItemInDir as tauriReveal } from "@tauri-apps/plugin-opener";
import { mockEngine, paintDesktop } from "./mock/engine";
import type { AnnotationSet, SpeedRegion, Trim, ZoomSeg, ZoomStyle } from "./types";

export type { Update };

const params = new URLSearchParams(window.location.search);
export const isMock = params.has("mock") || !("__TAURI_INTERNALS__" in window);

const DEMO_DIR = "C:\\Users\\demo\\Videos\\Demo Take.vuoom";
const VIDEOS = "C:\\Users\\demo\\Videos";

export function invoke<T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isMock) return tauriInvoke<T>(cmd, args);
  // Clone like real IPC would: the mock keeps and mutates its own objects, and handing
  // those references to the UI would defeat Solid's reference-based list diffing.
  return Promise.resolve(handleMock(cmd, args ?? {})).then((r) => structuredClone(r) as T);
}

export function listen<T>(event: string, cb: (payload: T) => void): Promise<() => void> {
  if (!isMock) return tauriListen<T>(event, (ev) => cb(ev.payload));
  if (event === "export-progress") {
    return Promise.resolve(mockEngine.onProgress(cb as (p: { done: number; total: number }) => void));
  }
  if (event === "stop-hotkey") {
    // No global hotkeys in a browser: wire Ctrl+Shift+X so the stop path stays testable.
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.shiftKey && e.code === "KeyX") cb(undefined as T);
    };
    window.addEventListener("keydown", onKey);
    return Promise.resolve(() => window.removeEventListener("keydown", onKey));
  }
  return Promise.resolve(() => undefined);
}

// Dialogs resolve instantly in mock mode so automated flows stay deterministic.
export function save(opts?: {
  defaultPath?: string;
  filters?: { name: string; extensions: string[] }[];
}): Promise<string | null> {
  if (!isMock) return tauriSave(opts) as Promise<string | null>;
  return Promise.resolve(`${VIDEOS}\\${opts?.defaultPath ?? "untitled.gif"}`);
}
export function open(opts?: { directory?: boolean; title?: string; multiple?: boolean }): Promise<string | null> {
  if (!isMock) return tauriOpen(opts) as Promise<string | null>;
  return Promise.resolve(DEMO_DIR);
}
export function ask(
  message: string,
  options?: { title?: string; kind?: "info" | "warning"; okLabel?: string; cancelLabel?: string },
): Promise<boolean> {
  if (!isMock) return tauriAsk(message, options);
  return Promise.resolve(true);
}
export function check(): Promise<Update | null> {
  if (!isMock) return tauriCheck();
  return Promise.resolve(null);
}
export function relaunch(): Promise<void> {
  if (!isMock) return tauriRelaunch();
  return Promise.resolve();
}
export function revealItemInDir(path: string): Promise<void> {
  if (!isMock) return tauriReveal(path);
  return Promise.resolve();
}

// ── mock command handlers ──────────────────────────────────────────────────────
function handleMock(cmd: string, a: Record<string, unknown>): unknown {
  const m = mockEngine;
  switch (cmd) {
    case "preview_port":
      return { port: 0, token: "mock" };
    case "engine_health":
      return { gpu: true };
    case "get_pref":
      return localStorage.getItem(`vuoom-mock-pref-${a.key as string}`);
    case "set_pref":
      localStorage.setItem(`vuoom-mock-pref-${a.key as string}`, a.value as string);
      return null;
    case "check_recovery":
      return m.hasClip ? null : 8.4;
    case "recovery_storage":
      return { bytes: 412_589_056, sessions: 2 };
    case "clear_recovery_storage":
      return 412_589_056;
    case "recover_session":
      return m.recoverSession();
    case "open_project_bundle":
      return m.openProject();
    case "save_project_bundle":
      return null;
    case "seek":
      m.playhead = Math.max(0, Math.min(m.duration || 0, a.t as number));
      m.sceneVersion++; // playhead moved: preview clients repaint
      return null;
    case "clip_state":
      return m.clipState();
    case "list_annotations":
      // Deep clone: Solid signals skip same-reference updates, and the mock mutates
      // its arrays in place (the real backend always serializes fresh objects).
      return structuredClone(m.anns) as AnnotationSet;
    case "add_text":
      return m.addText(a as unknown as { text: string; x: number; y: number; t: number });
    case "add_arrow":
      return m.addArrow(a as unknown as { fx: number; fy: number; tx: number; ty: number; t: number });
    case "add_box":
      return m.addBox({ ...(a as unknown as { x: number; y: number; w: number; h: number; t: number }) });
    case "add_ellipse":
      return m.addBox({
        ...(a as unknown as { x: number; y: number; w: number; h: number; t: number }),
        ellipse: true,
      });
    case "add_highlighter":
      return m.addBox({
        ...(a as unknown as { x: number; y: number; w: number; h: number; t: number }),
        highlight: true,
      });
    case "add_mask":
      return m.addBox({
        ...(a as unknown as { x: number; y: number; w: number; h: number; t: number }),
        mask: true,
      });
    case "set_crop":
      m.setCrop(
        a.x !== undefined
          ? { x: a.x as number, y: a.y as number, w: a.w as number, h: a.h as number }
          : null,
      );
      return m.clipState();
    case "plan_zoom_auto":
      return m.planZoomAuto(a.amount as number);
    case "list_displays":
      // `&onedisplay` skips the display picker (screenshots of the region HUD).
      if (params.has("onedisplay"))
        return [{ name: "\\.DISPLAY1", index: 1, x: 0, y: 0, w: 1920, h: 1080, primary: true }];
      return [
        { name: "\\.DISPLAY1", index: 1, x: 0, y: 0, w: 1920, h: 1080, primary: true },
        { name: "\\.DISPLAY2", index: 2, x: 1920, y: 0, w: 2560, h: 1440, primary: false },
      ];
    case "list_windows":
      return [
        { hwnd: 1001, title: "Demo Editor - Visual Studio Code", w: 1280, h: 800 },
        { hwnd: 1002, title: "Vuoom Docs - Google Chrome", w: 1440, h: 900 },
        { hwnd: 1003, title: "Terminal - pwsh", w: 1100, h: 700 },
      ];
    case "update_text":
      return mockUpdateText(m, a);
    case "update_box":
      m.updateBox(a.id as number, {
        x: a.x as number,
        y: a.y as number,
        w: a.w as number,
        h: a.h as number,
      });
      return null;
    case "update_arrow":
      m.updateArrow(a.id as number, a.fx as number, a.fy as number, a.tx as number, a.ty as number);
      return null;
    case "set_annotation_color":
      m.setAnnColor(a.id as number, a.r as number, a.g as number, a.b as number);
      return null;
    case "set_annotation_opacity":
      m.setAnnOpacity(a.id as number, a.a as number);
      return null;
    case "set_annotation_style":
      m.setAnnStyle(a.id as number, {
        thickness: a.thickness as number | undefined,
        filled: a.filled as boolean | undefined,
      });
      return null;
    case "set_highlight_shape":
      m.setHighlightShape(a.id as number, a.ellipse as boolean);
      return null;
    case "set_arrow_style":
      m.setArrowStyle(a.id as number, a.style as "arrow" | "line" | "double");
      return null;
    case "update_annotation_range":
      m.updateAnnRange(a.id as number, a.start as number, a.end as number);
      return null;
    case "persist_edits":
      return null;
    case "set_annotation_fades":
      m.setAnnFades(a.id as number, a.fadeIn as number, a.fadeOut as number);
      return null;
    case "duplicate_annotation":
      return m.duplicateAnn(a.id as number);
    case "paste_annotations":
      return m.pasteAnns(a.items as unknown as Parameters<typeof m.pasteAnns>[0], a.at as number);
    case "reorder_annotation":
      m.reorderAnn(a.id as number, a.dir as "forward" | "backward" | "front" | "back");
      return null;
    case "delete_annotation":
      m.deleteAnn(a.id as number, a.tag as string | undefined);
      return null;
    case "undo":
      return m.undo();
    case "redo":
      return m.redo();
    case "add_zoom":
      return m.addZoom(a.t as number) as ZoomSeg[];
    case "update_zoom":
      return m.updateZoom(a.index as number, a.start as number, a.end as number, a.amount as number);
    case "set_zoom_focus":
      return m.setZoomFocus(
        a.index as number,
        a.x !== undefined ? { x: a.x as number, y: a.y as number } : undefined,
      );
    case "set_zoom_style":
      return m.setZoomStyle(a.index as number, a.style as ZoomStyle);
    case "delete_zoom":
      return m.deleteZoom(a.index as number);
    case "set_capture_fps":
      return null;
    case "set_zoom_amount":
      m.zoomAmount = a.amount as number;
      return null;
    case "add_speed":
      return m.addSpeed(a.start as number, a.end as number, a.factor as number) as SpeedRegion[];
    case "update_speed":
      return m.updateSpeed(a.index as number, a.start as number, a.end as number, a.factor as number);
    case "delete_speed":
      return m.deleteSpeed(a.index as number);
    case "auto_speed":
      return m.autoSpeed(a.factor as number) as SpeedRegion[];
    case "clear_speed":
      m.clearSpeed();
      return null;
    case "add_cut":
      return m.addCut(a.start as number, a.end as number) as Trim[];
    case "update_cut":
      return m.updateCut(a.index as number, a.start as number, a.end as number);
    case "delete_cut":
      return m.deleteCut(a.index as number);
    case "set_trim":
      m.setTrim(a.start as number, a.end as number);
      return null;
    case "set_show_clicks":
      m.showClicks = a.on as boolean;
      return null;
    case "set_show_keys":
      m.showKeys = a.on as boolean;
      return null;
    case "set_frame_preset":
      m.setFramePreset(a.preset as string);
      return null;
    case "set_background_preset":
      m.setBackgroundPreset(a.name as string);
      return null;
    case "set_frame_style":
      m.setFrameStyle(a.padding as number, a.cornerRadius as number, a.shadow as number);
      return null;
    case "set_background_custom":
      m.setBackgroundCustom(
        a.from as [number, number, number],
        (a.to as [number, number, number] | undefined) ?? null,
        a.angle as number,
      );
      return null;
    case "estimate_gif":
      return m.estimateGif(a.fps as number, a.width as number, a.quality as number);
    case "export_gif":
    case "export_mp4":
      return m.runExport(a.path as string, a.fps as number, a.width as number, a.quality as number);
    case "cancel_export":
      m.cancelExport();
      return null;
    case "copy_export_to_clipboard":
      return null;
    case "enter_overlay":
      return m.enterOverlay();
    case "set_region":
      return null;
    case "show_region_border":
    case "hide_region_border":
    case "enter_stopbar":
      return null;
    case "start_recording":
      m.startRecording();
      return null;
    case "set_record_paused":
      m.setRecordPaused(a.paused as boolean);
      return null;
    case "finish_recording":
      return m.finishRecording();
    case "cancel_record_flow":
      m.live = false;
      return null;
    case "screenshot": {
      const c = document.createElement("canvas");
      c.width = 960;
      c.height = 540;
      paintDesktop(c.getContext("2d")!, 960, 540, m.playhead);
      return c.toDataURL("image/png");
    }
    default:
      console.warn(`[mock] unknown command: ${cmd}`, a);
      throw new Error(`mock: unknown command ${cmd}`);
  }
}

function mockUpdateText(m: typeof mockEngine, a: Record<string, unknown>) {
  const id = a.id as number;
  const patch: Record<string, unknown> = {};
  if (a.text !== undefined) patch.text = a.text;
  if (a.x !== undefined) {
    patch.pos = [a.x as number, a.y as number];
  }
  if (a.fontSize !== undefined) patch.font_size = a.fontSize;
  if (a.bold !== undefined) patch.bold = a.bold;
  if (a.italic !== undefined) patch.italic = a.italic;
  if (a.background !== undefined) patch.background = a.background;
  if (a.font !== undefined) patch.font = a.font;
  m.updateText(id, patch);
  return null;
}
