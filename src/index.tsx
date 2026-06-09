/* @refresh reload */
import { render } from "solid-js/web";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import Selector from "./Selector";
import Recorder from "./Recorder";

// Route by the Tauri window LABEL (set when each window is created in Rust). This avoids
// URL-fragment/query encoding issues — the overlay windows all load plain index.html.
let label = "main";
try {
  label = getCurrentWindow().label;
} catch {
  // Not inside a Tauri window (e.g. browser dev) — default to the editor.
}
document.documentElement.dataset.surface = label;

const root = document.getElementById("root") as HTMLElement;
const View = label === "selector" ? Selector : label === "recorder" ? Recorder : App;

render(() => <View />, root);
