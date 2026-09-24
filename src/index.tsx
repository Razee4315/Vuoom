/* @refresh reload */
import "@fontsource-variable/inter";
import { render } from "solid-js/web";
import App from "./App";
import { snapRecordPrefs } from "./recordOptions";

// Single window, single surface. The record flow (region selector → countdown → stop bar)
// runs as an in-window overlay inside App, no separate webview windows to route.
snapRecordPrefs();
const root = document.getElementById("root") as HTMLElement;
try {
  render(() => <App />, root);
} catch (e) {
  // Surface a hard mount failure instead of dying behind the splash (also covers the
  // browser mock, where there is no devtools console open by default).
  console.error("Vuoom failed to mount:", e);
  root.innerHTML = `<pre style="color:#e5484d;padding:24px;white-space:pre-wrap;font:12px monospace">Vuoom failed to mount: ${String(
    (e as Error)?.stack ?? e,
  )}</pre>`;
}

// The launch splash (in index.html) stays up until App connects to the engine,
// App calls hideSplash() once the backend is ready (or has definitively failed).
