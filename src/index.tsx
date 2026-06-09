/* @refresh reload */
import { render } from "solid-js/web";
import App from "./App";

// Single window, single surface. The record flow (region selector → countdown → stop bar)
// runs as an in-window overlay inside App — no separate webview windows to route.
const root = document.getElementById("root") as HTMLElement;
render(() => <App />, root);
