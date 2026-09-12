import { createEffect, createSignal, onCleanup, Show, type JSX } from "solid-js";
import { invoke, listen, save, revealItemInDir } from "./bridge";
import { toast } from "./ui";
import { dialogA11y } from "./dialog";
import { fmtBytes, friendlyError } from "./format";
import { outputDuration } from "./geometry";
import type { SpeedRegion, Trim } from "./types";

type Phase = "configure" | "starting" | "exporting" | "done" | "error";

export function ExportDialog(props: {
  name: string;
  duration: number;
  /** Real width / height of the clip, so MP4 size math is honest for every ratio. */
  aspect: number;
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
  const [phase, setPhase] = createSignal<Phase>("configure");
  const [progress, setProgress] = createSignal(0);
  const [estimate, setEstimate] = createSignal<number | null>(null); // -1 = unavailable
  const [outPath, setOutPath] = createSignal("");
  const [copied, setCopied] = createSignal("");
  const [errMsg, setErrMsg] = createSignal("");
  const [budgetMb, setBudgetMb] = createSignal("");
  const [fitting, setFitting] = createSignal(false);

  let dialogEl: HTMLDivElement | undefined;
  let exportStarted = false;

  const outDur = () => outputDuration(props.duration, props.trim, props.speed, props.cuts);

  // MP4 size follows directly from the bitrate (mirrors src-tauri mp4::bitrate) and uses
  // the REAL clip aspect, not an assumed 16:9.
  const mp4Estimate = () => {
    const q = Math.min(100, Math.max(40, quality()));
    const bpp = 0.04 + ((q - 40) / 60) * 0.16;
    const h = Math.round(width() / Math.max(props.aspect || 16 / 9, 0.2));
    const bits = width() * h * fps() * bpp;
    return Math.min(Math.max(bits, 1_000_000), 50_000_000) * (outDur() / 8);
  };

  // Live size estimate: GIF samples-and-extrapolates (debounced); MP4 is closed-form.
  // A failed probe shows "Estimate unavailable" instead of a fake 0 B.
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
          if (gen === estimateGen) setEstimate(Math.max(1, Math.round(b)));
        })
        .catch(() => {
          if (gen === estimateGen) setEstimate(-1);
        });
    }, 350);
  });
  onCleanup(() => clearTimeout(estimateTimer));

  // Presets are FORMAT-AWARE: the same named preset means different fps for GIF and MP4,
  // so the sliders can never contradict the selected preset chip.
  const presetValues = (p: "readme" | "hq"): { fps: number; width: number; quality: number } =>
    format() === "mp4"
      ? p === "readme"
        ? { fps: 30, width: 1000, quality: 85 }
        : { fps: 30, width: 1280, quality: 92 }
      : p === "readme"
        ? { fps: 15, width: 1000, quality: 80 }
        : { fps: 20, width: 1280, quality: 95 };

  const applyPreset = (p: "readme" | "hq" | "custom") => {
    setPreset(p);
    if (p !== "custom") {
      const v = presetValues(p);
      setFps(v.fps);
      setWidth(v.width);
      setQuality(v.quality);
    }
  };

  // Switching format re-grounds the active preset so values stay consistent.
  const switchFormat = (f: "gif" | "mp4") => {
    setFormat(f);
    if (preset() !== "custom") applyPreset(preset());
  };

  // E2: fit the GIF into an explicit byte budget by probing the engine's estimator for the
  // largest width that fits (then relaxing quality if nothing fits). Frontend-only.
  const fitToBudget = async () => {
    const mb = Number(budgetMb());
    if (!Number.isFinite(mb) || mb <= 0 || format() !== "gif") return;
    setFitting(true);
    try {
      const budget = mb * 1024 * 1024;
      const widths = [width(), 1280, 1000, 900, 800, 700, 600, 500, 400]
        .filter((w, i, arr) => w <= Math.max(width(), 400) && arr.indexOf(w) === i)
        .sort((a, b) => b - a);
      const qualities = [quality(), 75, 65, 55, 45];
      for (const q of qualities) {
        for (const w of widths) {
          const est = await invoke<number>("estimate_gif", {
            fps: fps(),
            width: w,
            quality: q,
          }).catch(() => Number.POSITIVE_INFINITY);
          if (est > 0 && est <= budget) {
            setWidth(w);
            setQuality(q);
            setPreset("custom");
            setFitting(false);
            toast(`Fitted to about ${fmtBytes(est)} at ${w}px`, "success");
            return;
          }
        }
      }
      toast("Could not reach that size. Try a lower frame rate.", "error");
    } finally {
      setFitting(false);
    }
  };

  const doExport = async () => {
    if (exportStarted) return;
    exportStarted = true;
    const f = format();
    const safe = props.name.replace(/[^\w.-]+/g, "-").replace(/^-+|-+$/g, "") || "vuoom";
    setPhase("starting"); // dialog locks while the OS save dialog is open
    try {
      const path = await save({
        defaultPath: `${safe}.${f}`,
        filters: [
          f === "gif"
            ? { name: "GIF", extensions: ["gif"] }
            : { name: "MP4 video", extensions: ["mp4"] },
        ],
      });
      if (!path) {
        setPhase("configure");
        exportStarted = false;
        return;
      }
      setPhase("exporting");
      setProgress(0);
      props.onStatus(`Exporting ${f.toUpperCase()}...`);
      // The listener is registered INSIDE the guarded flow and always cleaned up.
      const unlisten = await listen<{ done: number; total: number }>("export-progress", (p) => {
        setProgress(p.total > 0 ? p.done / p.total : 0);
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
        toast(`${f.toUpperCase()} exported`, "success");
      } catch (e) {
        const msg = String(e);
        props.onStatus(
          msg.includes("export cancelled") ? "Export cancelled" : `Export failed: ${friendlyError(e)}`,
        );
        if (msg.includes("export cancelled")) {
          setPhase("configure");
        } else {
          setErrMsg(friendlyError(e));
          setPhase("error"); // inline failure state with Retry, never a silent reset
          toast(`Export failed: ${friendlyError(e)}`, "error");
        }
      } finally {
        unlisten();
      }
    } catch (e) {
      setErrMsg(friendlyError(e));
      setPhase("error");
    } finally {
      exportStarted = false;
    }
  };

  // Abort an in-flight export. The backend loop bails at its next frame check, deletes the
  // partial file, and the invoke rejects with "export cancelled" (handled in doExport).
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
      toast("Copied to clipboard", "success");
    } catch (e) {
      setCopied(`Copy failed: ${String(e)}`);
      toast(`Copy failed: ${String(e)}`, "error");
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

  // Focus ownership across phase swaps: the dialog content is replaced wholesale, so
  // move focus to the phase heading (otherwise focus falls out to the page body).
  createEffect(() => {
    phase();
    queueMicrotask(() => {
      dialogEl?.querySelector<HTMLElement>("[data-phase-title]")?.focus();
    });
  });

  const busy = () => phase() === "starting" || phase() === "exporting";

  return (
    <div class="modal-backdrop" onClick={() => !busy() && props.onClose()}>
      <div
        class="modal"
        ref={(el) => {
          dialogEl = el;
          dialogA11y(el, "Export", () =>
            // Esc closes when idle, cancels an in-flight export, and is ignored mid-save.
            phase() === "exporting" ? cancelExport() : busy() ? undefined : props.onClose(),
          );
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <Show when={phase() === "configure"}>
          <h2 data-phase-title tabindex="-1">Export</h2>
          <div class="format-row">
            <button
              type="button"
              classList={{ chip: true, active: format() === "gif" }}
              aria-pressed={format() === "gif"}
              onClick={() => switchFormat("gif")}
            >
              GIF<small>loops anywhere · README / chat</small>
            </button>
            <button
              type="button"
              classList={{ chip: true, active: format() === "mp4" }}
              aria-pressed={format() === "mp4"}
              onClick={() => switchFormat("mp4")}
            >
              MP4 video<small>smaller · smoother · Slack / X / YouTube</small>
            </button>
          </div>
          <div class="preset-row">
            <button type="button" classList={{ chip: true, active: preset() === "readme" }} aria-pressed={preset() === "readme"} onClick={() => applyPreset("readme")}>
              README<small>
                {presetValues("readme").fps}fps · {presetValues("readme").width}px
              </small>
            </button>
            <button type="button" classList={{ chip: true, active: preset() === "hq" }} aria-pressed={preset() === "hq"} onClick={() => applyPreset("hq")}>
              High quality<small>
                {presetValues("hq").fps}fps · {presetValues("hq").width}px
              </small>
            </button>
            <button type="button" classList={{ chip: true, active: preset() === "custom" }} aria-pressed={preset() === "custom"} onClick={() => applyPreset("custom")}>
              Custom<small>tune it yourself</small>
            </button>
          </div>

          <label class="field">
            <span class="field-label">
              Frame rate <span class="field-value">{fps()} fps</span>
            </span>
            <input type="range" min="8" max={format() === "mp4" ? 60 : 30} step="1" value={fps()} onInput={(e) => { setFps(Number(e.currentTarget.value)); setPreset("custom"); }} />
          </label>
          <label class="field">
            <span class="field-label">
              Max width <span class="field-value">{width()} px</span>
            </span>
            <input type="range" min="400" max="1920" step="20" value={width()} onInput={(e) => { setWidth(Number(e.currentTarget.value)); setPreset("custom"); }} />
          </label>
          <label class="field">
            <span class="field-label">
              Quality <span class="field-value">{quality()}</span>
            </span>
            <input type="range" min="40" max="100" step="1" value={quality()} onInput={(e) => { setQuality(Number(e.currentTarget.value)); setPreset("custom"); }} />
          </label>

          <Show when={format() === "gif"}>
            <div class="fit-row">
              <label class="fit-label">
                Fit under
                <input
                  class="fit-input"
                  type="number"
                  min="0.2"
                  step="0.5"
                  placeholder="MB"
                  value={budgetMb()}
                  onInput={(e) => setBudgetMb(e.currentTarget.value)}
                />
                MB
              </label>
              <button
                type="button"
                class="btn"
                disabled={!Number(budgetMb()) || fitting()}
                title="Probe encoder estimates and pick the largest settings under the budget"
                onClick={() => void fitToBudget()}
              >
                {fitting() ? "Fitting..." : "Fit"}
              </button>
            </div>
          </Show>

          <div class="export-meta">
            <span>
              {outDur().toFixed(1)}s of {format().toUpperCase()}
            </span>
            <span class="export-size">
              {estimate() === null
                ? "estimating size..."
                : estimate()! < 0
                  ? "Estimate unavailable"
                  : `≈ ${fmtBytes(estimate()!)}`}
            </span>
          </div>

          <div class="modal-actions">
            <button class="btn ghost" onClick={props.onClose}>
              Cancel
            </button>
            <button class="btn export" onClick={() => void doExport()}>
              Choose location & export
            </button>
          </div>
        </Show>

        <Show when={phase() === "starting"}>
          <h2 data-phase-title tabindex="-1">Choosing a location...</h2>
          <p class="muted small">Pick where the {format().toUpperCase()} should be written.</p>
        </Show>

        <Show when={phase() === "exporting"}>
          <h2 data-phase-title tabindex="-1">Exporting {format().toUpperCase()}</h2>
          <p class="export-pct" role="status">{Math.round(progress() * 100)}%</p>
          <div
            class="progress"
            role="progressbar"
            aria-label="Export progress"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(progress() * 100)}
          >
            <div class="progress-fill" style={{ width: `${Math.round(progress() * 100)}%` }} />
          </div>
          <p class="muted small">
            Annotations, zooms, speed-up and cuts are baked into the final file. This takes a
            moment.
          </p>
          <div class="modal-actions">
            <button class="btn ghost" onClick={cancelExport}>
              Cancel export
            </button>
          </div>
        </Show>

        <Show when={phase() === "error"}>
          <h2 data-phase-title tabindex="-1">Export failed</h2>
          <p class="export-error">{errMsg()}</p>
          <div class="modal-actions">
            <button class="btn ghost" onClick={() => setPhase("configure")}>
              Back to settings
            </button>
            <button class="btn export" onClick={() => void doExport()}>
              Retry export
            </button>
          </div>
        </Show>

        <Show when={phase() === "done"}>
          <h2 data-phase-title tabindex="-1">{format().toUpperCase()} exported</h2>
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
                : "Copy puts the MP4 on the clipboard as a file, so drag it in from the folder if an app won't paste it.")}
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
