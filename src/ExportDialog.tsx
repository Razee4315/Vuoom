import { createEffect, createSignal, onCleanup, Show, type JSX } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { dialogA11y } from "./dialog";
import { fmtBytes, friendlyError } from "./format";
import { outputDuration } from "./geometry";
import type { SpeedRegion, Trim } from "./types";

export function ExportDialog(props: {
  name: string;
  duration: number;
  trim: Trim | null;
  speed: SpeedRegion[];
  cuts: Trim[];
  onClose: () => void;
  onStatus: (s: string) => void;
  onExported: () => void;
}): JSX.Element {
  const [format, setFormat] = createSignal<"gif" | "mp4">("gif");
  const [preset, setPreset] = createSignal<"readme" | "hq" | "custom">("readme");
  const [fps, setFps] = createSignal(15);
  const [width, setWidth] = createSignal(1000);
  const [quality, setQuality] = createSignal(80);
  const [phase, setPhase] = createSignal<"configure" | "exporting" | "done">("configure");
  const [progress, setProgress] = createSignal(0);
  const [estimate, setEstimate] = createSignal<number | null>(null);
  const [outPath, setOutPath] = createSignal("");
  const [copied, setCopied] = createSignal("");

  const outDur = () => outputDuration(props.duration, props.trim, props.speed, props.cuts);

  // MP4 size follows directly from the bitrate (mirrors src-tauri mp4::bitrate).
  const mp4Estimate = () => {
    const q = Math.min(100, Math.max(40, quality()));
    const bpp = 0.04 + ((q - 40) / 60) * 0.16;
    const h = Math.round((width() * 9) / 16); // rough; real height depends on the source
    const bits = width() * h * fps() * bpp;
    return Math.min(Math.max(bits, 1_000_000), 50_000_000) * (outDur() / 8);
  };

  // Live size estimate: GIF samples-and-extrapolates (debounced); MP4 is closed-form.
  let estimateTimer: number | undefined;
  let estimateGen = 0;
  createEffect(() => {
    const args = { fps: fps(), width: width(), quality: quality() };
    setEstimate(null);
    clearTimeout(estimateTimer);
    const gen = ++estimateGen;
    if (format() === "mp4") {
      setEstimate(mp4Estimate());
      return;
    }
    estimateTimer = window.setTimeout(() => {
      invoke<number>("estimate_gif", args)
        .then((b) => {
          if (gen === estimateGen) setEstimate(b);
        })
        .catch(() => {
          if (gen === estimateGen) setEstimate(0);
        });
    }, 350);
  });
  onCleanup(() => clearTimeout(estimateTimer));

  const applyPreset = (p: "readme" | "hq" | "custom") => {
    setPreset(p);
    if (p === "readme") {
      setFps(15);
      setWidth(1000);
      setQuality(80);
    } else if (p === "hq") {
      setFps(20);
      setWidth(1280);
      setQuality(95);
    }
  };

  const doExport = async () => {
    const f = format();
    const safe = props.name.replace(/[^\w.-]+/g, "-").replace(/^-+|-+$/g, "") || "vuoom";
    const path = await save({
      defaultPath: `${safe}.${f}`,
      filters: [
        f === "gif"
          ? { name: "GIF", extensions: ["gif"] }
          : { name: "MP4 video", extensions: ["mp4"] },
      ],
    });
    if (!path) return;
    setPhase("exporting");
    setProgress(0);
    props.onStatus(`Exporting ${f.toUpperCase()}…`);
    const unlisten = await listen<{ done: number; total: number }>("export-progress", (ev) => {
      setProgress(ev.payload.total > 0 ? ev.payload.done / ev.payload.total : 0);
    });
    try {
      await invoke(f === "gif" ? "export_gif" : "export_mp4", {
        path,
        fps: fps(),
        width: width(),
        quality: quality(),
      });
      setOutPath(path);
      setPhase("done");
      props.onExported();
      props.onStatus(`Exported ${path}`);
    } catch (e) {
      setPhase("configure");
      // The backend uses the bare "export cancelled" sentinel for a user-initiated abort
      // (Cancel button / window-close) so we can show calm copy rather than a scary failure.
      const msg = String(e);
      props.onStatus(
        msg.includes("export cancelled") ? "Export cancelled" : `Export failed: ${friendlyError(e)}`,
      );
    } finally {
      unlisten();
    }
  };

  // Abort an in-flight export. The backend loop bails at its next frame check, deletes the
  // partial file, and `doExport`'s invoke rejects with "export cancelled" (handled above).
  const cancelExport = () => void invoke("cancel_export").catch(() => undefined);

  const copyFile = async () => {
    try {
      await invoke("copy_export_to_clipboard", { path: outPath() });
      // Honest per-format copy: GIFs paste as animations almost everywhere; MP4s are
      // copied as a *file*, which many chat apps won't accept from the clipboard.
      setCopied(
        format() === "gif"
          ? "Copied! Paste it into Slack, Discord, or a GitHub comment."
          : "Copied as a file. If pasting doesn't work, drag it in from Show in folder.",
      );
    } catch (e) {
      setCopied(`Copy failed: ${String(e)}`);
    }
  };
  const copyPath = async () => {
    try {
      await navigator.clipboard.writeText(outPath());
      setCopied("Path copied.");
    } catch {
      setCopied("Could not copy the path.");
    }
  };
  const reveal = () => void revealItemInDir(outPath()).catch(() => undefined);

  return (
    <div class="modal-backdrop" onClick={() => phase() !== "exporting" && props.onClose()}>
      <div
        class="modal"
        ref={(el) =>
          dialogA11y(el, "Export", () =>
            // Esc closes the dialog when idle, but during an export it becomes the Cancel
            // escape hatch instead of a no-op (the backdrop stays click-blocked).
            phase() === "exporting" ? cancelExport() : props.onClose(),
          )
        }
        onClick={(e) => e.stopPropagation()}
      >
        <Show when={phase() === "configure"}>
          <h2>Export</h2>
          <div class="format-row">
            <button
              classList={{ chip: true, active: format() === "gif" }}
              onClick={() => setFormat("gif")}
            >
              GIF<small>loops anywhere · README / chat</small>
            </button>
            <button
              classList={{ chip: true, active: format() === "mp4" }}
              onClick={() => {
                setFormat("mp4");
                if (fps() < 24) setFps(30);
              }}
            >
              MP4 video<small>smaller · smoother · Slack / X / YouTube</small>
            </button>
          </div>
          <div class="preset-row">
            <button classList={{ chip: true, active: preset() === "readme" }} onClick={() => applyPreset("readme")}>
              README<small>small · 15fps · 1000px</small>
            </button>
            <button classList={{ chip: true, active: preset() === "hq" }} onClick={() => applyPreset("hq")}>
              High quality<small>crisp · 20fps · 1280px</small>
            </button>
            <button classList={{ chip: true, active: preset() === "custom" }} onClick={() => applyPreset("custom")}>
              Custom<small>tune it yourself</small>
            </button>
          </div>

          <label class="field">
            <span>Frame rate · {fps()} fps</span>
            <input type="range" min="8" max={format() === "mp4" ? 60 : 30} step="1" value={fps()} onInput={(e) => { setFps(Number(e.currentTarget.value)); setPreset("custom"); }} />
          </label>
          <label class="field">
            <span>Max width · {width()} px</span>
            <input type="range" min="400" max="1920" step="20" value={width()} onInput={(e) => { setWidth(Number(e.currentTarget.value)); setPreset("custom"); }} />
          </label>
          <label class="field">
            <span>Quality · {quality()}</span>
            <input type="range" min="40" max="100" step="1" value={quality()} onInput={(e) => { setQuality(Number(e.currentTarget.value)); setPreset("custom"); }} />
          </label>

          <div class="export-meta">
            <span>
              {outDur().toFixed(1)}s of {format().toUpperCase()}
            </span>
            <span class="export-size">
              {estimate() === null ? "estimating size…" : `≈ ${fmtBytes(estimate()!)}`}
            </span>
          </div>

          <div class="modal-actions">
            <button class="btn" onClick={props.onClose}>
              Cancel
            </button>
            <button class="btn export" onClick={() => void doExport()}>
              Choose location & export
            </button>
          </div>
        </Show>

        <Show when={phase() === "exporting"}>
          <h2>Exporting…</h2>
          <div class="progress">
            <div class="progress-fill" style={{ width: `${Math.round(progress() * 100)}%` }} />
          </div>
          <p class="muted small">
            Compositing {Math.round(progress() * 100)}% — annotations, zoom, speed-up and cuts
            are baked into the final {format().toUpperCase()}.
          </p>
          <div class="modal-actions">
            <button class="btn ghost" onClick={cancelExport}>
              Cancel export
            </button>
          </div>
        </Show>

        <Show when={phase() === "done"}>
          <h2>{format().toUpperCase()} exported</h2>
          <p class="export-path" title={outPath()}>
            {outPath()}
          </p>
          <div class="done-actions">
            <button class="btn export" onClick={() => void copyFile()}>
              Copy {format().toUpperCase()}
            </button>
            <button class="btn" onClick={() => void copyPath()}>
              Copy path
            </button>
            <button class="btn" onClick={reveal}>
              Show in folder
            </button>
          </div>
          <p class="muted small">
            {copied() ||
              (format() === "gif"
                ? "Paste the copied GIF anywhere that accepts files."
                : "Copy puts the MP4 on the clipboard as a file — drag from the folder if an app won't paste it.")}
          </p>
          <div class="modal-actions">
            <button class="btn" onClick={props.onClose}>
              Done
            </button>
          </div>
        </Show>
      </div>
    </div>
  );
}
