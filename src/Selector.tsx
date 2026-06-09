import { createSignal, onMount, onCleanup, For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./Selector.css";

type Preset = { id: string; label: string; hint: string; ratio: number | null | "full" };

// ratio = width / height. null = free draw. "full" = whole screen, no draw.
const PRESETS: Preset[] = [
  { id: "full", label: "Full screen", hint: "Whole display", ratio: "full" },
  { id: "16:9", label: "16:9", hint: "YouTube · Reddit · X", ratio: 16 / 9 },
  { id: "9:16", label: "9:16", hint: "Reels · TikTok · Shorts", ratio: 9 / 16 },
  { id: "1:1", label: "1:1", hint: "Instagram · Facebook", ratio: 1 },
  { id: "4:5", label: "4:5", hint: "Instagram · Facebook", ratio: 4 / 5 },
  { id: "free", label: "Custom", hint: "Any size", ratio: null },
];

type Rect = { x: number; y: number; w: number; h: number };

export default function Selector() {
  const [preset, setPreset] = createSignal<Preset>(PRESETS[1]);
  const [sel, setSel] = createSignal<Rect | null>(null);
  let start: { x: number; y: number } | null = null;

  const cancel = () => void invoke("cancel_record_flow");

  const confirm = async () => {
    const p = preset();
    const r = sel();
    if (p.ratio === "full" || !r || r.w < 8 || r.h < 8) {
      await invoke("set_region", {}); // no fields → full screen
    } else {
      const dpr = window.devicePixelRatio || 1;
      await invoke("set_region", {
        x: Math.round(r.x * dpr),
        y: Math.round(r.y * dpr),
        w: Math.round(r.w * dpr),
        h: Math.round(r.h * dpr),
      });
    }
    await invoke("begin_countdown");
  };

  const onDown = (e: PointerEvent) => {
    if (preset().ratio === "full") return;
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    start = { x: e.clientX, y: e.clientY };
    setSel({ x: e.clientX, y: e.clientY, w: 0, h: 0 });
  };
  const onMove = (e: PointerEvent) => {
    if (!start) return;
    const ratio = preset().ratio;
    const x = Math.min(start.x, e.clientX);
    const y = Math.min(start.y, e.clientY);
    const w = Math.abs(e.clientX - start.x);
    const h = typeof ratio === "number" ? w / ratio : Math.abs(e.clientY - start.y);
    setSel({ x, y, w, h });
  };
  const onUp = () => {
    start = null;
  };

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") cancel();
    else if (e.key === "Enter") void confirm();
  };

  onMount(() => {
    window.addEventListener("keydown", onKey);
    onCleanup(() => window.removeEventListener("keydown", onKey));
  });

  const pickPreset = (p: Preset) => {
    setPreset(p);
    setSel(null);
    start = null;
  };

  const dims = () => {
    const r = sel();
    if (preset().ratio === "full" || !r) return "Full screen";
    const dpr = window.devicePixelRatio || 1;
    return `${Math.round(r.w * dpr)} × ${Math.round(r.h * dpr)} px`;
  };

  return (
    <div
      class="sel-root"
      classList={{ full: preset().ratio === "full" }}
      onPointerDown={onDown}
      onPointerMove={onMove}
      onPointerUp={onUp}
    >
      {/* Dim everything; the selection rect punches a bright hole via a huge box-shadow. */}
      <Show when={preset().ratio !== "full" && sel()}>
        {(r) => (
          <div
            class="sel-rect"
            style={{ left: `${r().x}px`, top: `${r().y}px`, width: `${r().w}px`, height: `${r().h}px` }}
          />
        )}
      </Show>
      <Show when={preset().ratio === "full"}>
        <div class="sel-fullhint">Recording the whole display</div>
      </Show>

      <div class="sel-bar" onPointerDown={(e) => e.stopPropagation()}>
        <div class="sel-presets">
          <For each={PRESETS}>
            {(p) => (
              <button
                classList={{ "sel-chip": true, active: preset().id === p.id }}
                title={p.hint}
                onClick={() => pickPreset(p)}
              >
                <strong>{p.label}</strong>
                <small>{p.hint}</small>
              </button>
            )}
          </For>
        </div>
        <div class="sel-actions">
          <span class="sel-dims">{dims()}</span>
          <button class="sel-btn ghost" onClick={cancel}>
            Cancel
          </button>
          <button class="sel-btn primary" onClick={() => void confirm()}>
            Start →
          </button>
        </div>
      </div>

      <Show when={preset().ratio !== "full" && !sel()}>
        <div class="sel-drawhint">Drag to mark the area · Esc to cancel</div>
      </Show>
    </div>
  );
}
