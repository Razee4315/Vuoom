import { createSignal, onMount, For } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type Tool = "select" | "text" | "arrow" | "box" | "crop";

/** Mirrors vuoom_encode::GifSettings (serde). */
interface GifSettings {
  fps: number;
  width: number | null;
  quality: number;
  lossy: number | null;
}

const TOOLS: { id: Tool; label: string }[] = [
  { id: "select", label: "Select" },
  { id: "text", label: "Text" },
  { id: "arrow", label: "Arrow" },
  { id: "box", label: "Box" },
  { id: "crop", label: "Crop" },
];

function App() {
  const [tool, setTool] = createSignal<Tool>("select");
  const [status, setStatus] = createSignal("Ready");
  const [presets, setPresets] = createSignal<GifSettings[]>([]);

  onMount(async () => {
    try {
      const [readme, hq] = await invoke<[GifSettings, GifSettings]>("gif_presets");
      setPresets([readme, hq]);
      setStatus("Engine connected");
    } catch (e) {
      setStatus(`Backend error: ${String(e)}`);
    }
  });

  return (
    <div class="editor">
      <header class="topbar">
        <button class="btn record">● Record</button>
        <div class="title">Untitled — Vuoom</div>
        <button class="btn export">Export GIF</button>
      </header>

      <div class="workspace">
        <nav class="toolrail">
          <For each={TOOLS}>
            {(t) => (
              <button
                classList={{ tool: true, active: tool() === t.id }}
                onClick={() => setTool(t.id)}
                title={t.label}
              >
                {t.label}
              </button>
            )}
          </For>
        </nav>

        <main class="canvas">
          <div class="canvas-frame">
            <div class="canvas-placeholder">
              <p class="big">Preview</p>
              <small>Record a clip to begin — auto-zoom is applied automatically.</small>
            </div>
          </div>
        </main>

        <aside class="properties">
          <h2>Properties</h2>
          <p class="muted">Select an element on the canvas to edit it.</p>

          <h3>Export presets</h3>
          <For each={presets()}>
            {(p, i) => (
              <div class="preset">
                <strong>{i() === 0 ? "README" : "High quality"}</strong>
                <span>
                  {p.fps} fps · {p.width ?? "source"}px · q{p.quality}
                </span>
              </div>
            )}
          </For>
        </aside>
      </div>

      <footer class="timeline">
        <div class="timeline-track">
          <span class="muted">Timeline — trim · zoom blocks · text &amp; annotations</span>
        </div>
        <div class="statusbar">{status()}</div>
      </footer>
    </div>
  );
}

export default App;
