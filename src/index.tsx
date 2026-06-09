/* @refresh reload */
import { render } from "solid-js/web";
import App from "./App";
import Selector from "./Selector";
import Recorder from "./Recorder";

const root = document.getElementById("root") as HTMLElement;
const hash = window.location.hash.replace(/^#/, "");
// Tag the surface so CSS can make the overlay windows transparent (App.css paints an
// opaque background that would otherwise fill the selector/recorder windows).
document.documentElement.dataset.surface = hash || "main";

const View = hash === "selector" ? Selector : hash === "recorder" ? Recorder : App;

render(() => <View />, root);
