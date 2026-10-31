/* @refresh reload */
import "@fontsource-variable/inter";
import { render } from "solid-js/web";
import App from "./App";
import { isMock } from "./bridge";
import { initLocale } from "./i18n";
import { snapRecordPrefs } from "./recordOptions";

// Single window, single surface. The record flow (region selector → countdown → stop bar)
// runs as an in-window overlay inside App, no separate webview windows to route.
snapRecordPrefs();
const root = document.getElementById("root") as HTMLElement;

// Surface a hard mount failure instead of dying behind the splash (also covers the
// browser mock, where there is no devtools console open by default).
const showMountFailure = (e: unknown) => {
  console.error("Vuoom failed to mount:", e);
  const pre = document.createElement("pre");
  pre.style.cssText = "color:#e5484d;padding:24px;white-space:pre-wrap;font:12px monospace";
  pre.textContent = `Vuoom failed to mount: ${String((e as Error)?.stack ?? e)}`;
  root.replaceChildren(pre);
  document.getElementById("splash")?.remove();
};

const mount = () => {
  try {
    render(() => <App />, root);
  } catch (e) {
    showMountFailure(e);
  }
};

// The dictionary loads before first render, so a Chinese interface never flashes English.
// In a plain browser the engine is a mock, fetched only then: the packaged app starts
// without it.
Promise.all([initLocale(), isMock ? import("./mock/install") : undefined]).then(mount, showMountFailure);

// The launch splash (in index.html) stays up until App connects to the engine,
// App calls hideSplash() once the backend is ready (or has definitively failed).
