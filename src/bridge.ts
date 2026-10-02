// Backend bridge. The app code imports invoke/listen/dialog helpers from here instead of
// from the Tauri packages directly; the bridge dispatches to the real Tauri runtime when
// present and to the browser mock otherwise. This keeps every screen developable, testable
// and screenshot-able in a plain browser without the native backend.
//
// The mock itself (mock/) is not imported here: index.tsx loads it on demand and hands it
// over with `installMock`, so the packaged app never downloads or parses it.

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import { save as tauriSave, open as tauriOpen, ask as tauriAsk } from "@tauri-apps/plugin-dialog";
import { check as tauriCheck, type Update } from "@tauri-apps/plugin-updater";
import { relaunch as tauriRelaunch } from "@tauri-apps/plugin-process";
import { openUrl as tauriOpenUrl, revealItemInDir as tauriReveal } from "@tauri-apps/plugin-opener";

export type { Update };

export const isMock =
  new URLSearchParams(window.location.search).has("mock") || !("__TAURI_INTERNALS__" in window);

export interface SaveOptions {
  defaultPath?: string;
  filters?: { name: string; extensions: string[] }[];
}
export interface OpenOptions {
  directory?: boolean;
  title?: string;
  multiple?: boolean;
  filters?: { name: string; extensions: string[] }[];
}

/** What the browser mock provides in place of the Tauri runtime (see mock/backend.ts). */
export interface MockBackend {
  handle(cmd: string, args: Record<string, unknown>): unknown;
  listen(event: string, cb: (payload: unknown) => void): () => void;
  save(opts?: SaveOptions): string;
  open(opts?: OpenOptions): string;
}

let mock: MockBackend | null = null;
/** Hand the bridge its mock backend. Called once, before the app renders (index.tsx). */
export function installMock(backend: MockBackend): void {
  mock = backend;
}
function theMock(): MockBackend {
  if (!mock) throw new Error("the mock backend was used before it was installed");
  return mock;
}

export function invoke<T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isMock) return tauriInvoke<T>(cmd, args);
  // Clone like real IPC would: the mock keeps and mutates its own objects, and handing
  // those references to the UI would defeat Solid's reference-based list diffing.
  return Promise.resolve()
    .then(() => theMock().handle(cmd, args ?? {}))
    .then((r) => structuredClone(r) as T);
}

export function listen<T>(event: string, cb: (payload: T) => void): Promise<() => void> {
  if (!isMock) return tauriListen<T>(event, (ev) => cb(ev.payload));
  return Promise.resolve(theMock().listen(event, cb as (payload: unknown) => void));
}

// Dialogs resolve instantly in mock mode so automated flows stay deterministic.
export function save(opts?: SaveOptions): Promise<string | null> {
  if (!isMock) return tauriSave(opts) as Promise<string | null>;
  return Promise.resolve(theMock().save(opts));
}
export function open(opts?: OpenOptions): Promise<string | null> {
  if (!isMock) return tauriOpen(opts) as Promise<string | null>;
  return Promise.resolve(theMock().open(opts));
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
/** Say this run is ending on purpose. An update's installer and a relaunch both end the
 *  process without the normal exit, and the next launch would take that for a crash. */
export async function markCleanExit(): Promise<void> {
  if (isMock) return;
  await tauriInvoke("mark_clean_exit").catch(() => undefined);
}
export async function relaunch(): Promise<void> {
  if (isMock) return;
  await markCleanExit();
  return tauriRelaunch();
}
export function openUrl(url: string): Promise<void> {
  if (!isMock) return tauriOpenUrl(url);
  window.open(url, "_blank", "noopener");
  return Promise.resolve();
}
export function revealItemInDir(path: string): Promise<void> {
  if (!isMock) return tauriReveal(path);
  return Promise.resolve();
}
