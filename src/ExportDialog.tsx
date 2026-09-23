import { batch, createEffect, createSignal, onCleanup, Show, type JSX } from "solid-js";
import { invoke, listen, save, revealItemInDir } from "./bridge";
import { Icon } from "./icons";
import { prefs } from "./prefs";
import { Field, Slider, toast } from "./ui";
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
  const [format, setFormat] = createSignal<"gif" | "mp4">(prefs.exportFormat());
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

  // Each format remembers its last settings; otherwise it starts on its Balanced preset.
  const savedFor = (f: "gif" | "mp4") => (f === "gif" ? prefs.exportGif() : prefs.exportMp4());
  const restore = (f: "gif" | "mp4") => {
    const s = savedFor(f);
    if (!s) {
      applyPreset("readme");
      return;
    }
    setPreset(s.preset);
    setFps(s.fps);
    setWidth(s.width);
    setQuality(s.quality);
  };
  restore(format());
  createEffect(() => {
    const s = { preset: preset(), fps: fps(), width: width(), quality: quality() };
    (format() === "gif" ? prefs.exportGif : prefs.exportMp4).set(s);
  });

  // Switching format restores that format's own last settings.
  const switchFormat = (f: "gif" | "mp4") => {
    if (f === format()) return;
    // One batch, so the save effect never sees the new format with the old values.
    batch(() => {
      setFormat(f);
      restore(f);
    });
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

  const pct = () => Math.round(progress() * 100);
  const sizeText = () =>
    estimate() === null ? "Estimating…" : estimate()! < 0 ? "Unavailable" : `≈ ${fmtBytes(estimate()!)}`;
  const height = () => Math.round(width() / Math.max(props.aspect || 16 / 9, 0.2));

  return (
    <div class="modal-backdrop" onClick={() => !busy() && props.onClose()}>
      <div
        class="modal export"
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
          <header class="modal-head">
            <div>
              <h2 data-phase-title tabindex="-1">
                Export
              </h2>
              <p class="muted small">Zooms, annotations, speed-ups and cuts are baked into the file.</p>
            </div>
            <button type="button" class="ibtn" aria-label="Close" onClick={props.onClose}>
              <Icon name="close" size={14} />
            </button>
          </header>

          <div class="export-formats">
            <button
              type="button"
              class="export-format"
              classList={{ on: format() === "gif" }}
              aria-pressed={format() === "gif"}
              onClick={() => switchFormat("gif")}
            >
              <span class="export-format-icon">
                <Icon name="gif" size={20} />
              </span>
              <span>
                <strong>GIF</strong>
                <small>Loops anywhere: READMEs, docs, chat</small>
              </span>
            </button>
            <button
              type="button"
              class="export-format"
              classList={{ on: format() === "mp4" }}
              aria-pressed={format() === "mp4"}
              onClick={() => switchFormat("mp4")}
            >
              <span class="export-format-icon">
                <Icon name="film" size={20} />
              </span>
              <span>
                <strong>MP4 video</strong>
                <small>Smaller and smoother: Slack, X, YouTube</small>
              </span>
            </button>
          </div>

          <div class="export-presets">
            <button
              type="button"
              class="export-preset"
              classList={{ on: preset() === "readme" }}
              aria-pressed={preset() === "readme"}
              onClick={() => applyPreset("readme")}
            >
              Balanced
              <small>
                {presetValues("readme").fps} fps · {presetValues("readme").width}px
              </small>
            </button>
            <button
              type="button"
              class="export-preset"
              classList={{ on: preset() === "hq" }}
              aria-pressed={preset() === "hq"}
              onClick={() => applyPreset("hq")}
            >
              High quality
              <small>
                {presetValues("hq").fps} fps · {presetValues("hq").width}px
              </small>
            </button>
            <button
              type="button"
              class="export-preset"
              classList={{ on: preset() === "custom" }}
              aria-pressed={preset() === "custom"}
              onClick={() => applyPreset("custom")}
            >
              Custom
              <small>Tune every setting</small>
            </button>
          </div>

          <div class="export-controls">
            <Field label="Frame rate">
              <Slider
                value={fps()}
                min={8}
                max={format() === "mp4" ? 60 : 30}
                step={1}
                label="Frame rate"
                format={(v) => `${v} fps`}
                onInput={(v) => {
                  setFps(v);
                  setPreset("custom");
                }}
              />
            </Field>
            <Field label="Width">
              <Slider
                value={width()}
                min={400}
                max={1920}
                step={20}
                label="Max width"
                format={(v) => `${v}px`}
                onInput={(v) => {
                  setWidth(v);
                  setPreset("custom");
                }}
              />
            </Field>
            <Field label="Quality">
              <Slider
                value={quality()}
                min={40}
                max={100}
                step={1}
                label="Quality"
                format={(v) => `${v}`}
                onInput={(v) => {
                  setQuality(v);
                  setPreset("custom");
                }}
              />
            </Field>
            <Show when={format() === "gif"}>
              <div class="export-fit">
                <Icon name="target" size={14} />
                <span>Fit under</span>
                <input
                  class="input"
                  type="number"
                  min="0.2"
                  step="0.5"
                  placeholder="MB"
                  aria-label="Size budget in MB"
                  value={budgetMb()}
                  onInput={(e) => setBudgetMb(e.currentTarget.value)}
                />
                <span>MB</span>
                <button
                  type="button"
                  class="btn sm"
                  disabled={!Number(budgetMb()) || fitting()}
                  data-tip="Find the largest width and quality that stay under the budget"
                  onClick={() => void fitToBudget()}
                >
                  {fitting() ? "Fitting…" : "Fit"}
                </button>
              </div>
            </Show>
          </div>

          <div class="export-summary">
            <div>
              <small>Estimated size</small>
              <span class="export-size">{sizeText()}</span>
            </div>
            <div>
              <small>Output</small>
              <span>
                {width()} × {height()} · {outDur().toFixed(1)}s
              </span>
            </div>
          </div>

          <div class="modal-actions">
            <button type="button" class="btn ghost" onClick={props.onClose}>
              Cancel
            </button>
            <button type="button" class="btn primary" onClick={() => void doExport()}>
              <Icon name="export" size={14} /> Export {format().toUpperCase()}
            </button>
          </div>
        </Show>

        <Show when={phase() === "starting"}>
          <div class="export-progress">
            <h2 data-phase-title tabindex="-1">
              Choose where to save
            </h2>
            <p class="muted small">Pick a folder and a name for the {format().toUpperCase()}.</p>
          </div>
        </Show>

        <Show when={phase() === "exporting"}>
          <div class="export-progress">
            <div
              class="ring"
              style={{ "--p": pct() }}
              role="progressbar"
              aria-label="Export progress"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={pct()}
            >
              <span>{pct()}%</span>
            </div>
            <h2 data-phase-title tabindex="-1">
              Exporting {format().toUpperCase()}
            </h2>
            <p class="muted small">Rendering every frame with your zooms and annotations.</p>
            <button type="button" class="btn ghost" onClick={cancelExport}>
              Cancel export
            </button>
          </div>
        </Show>

        <Show when={phase() === "error"}>
          <h2 data-phase-title tabindex="-1">
            Export failed
          </h2>
          <p class="export-error">{errMsg()}</p>
          <div class="modal-actions">
            <button type="button" class="btn ghost" onClick={() => setPhase("configure")}>
              Back to settings
            </button>
            <button type="button" class="btn primary" onClick={() => void doExport()}>
              Try again
            </button>
          </div>
        </Show>

        <Show when={phase() === "done"}>
          <div class="export-progress">
            <div class="ring done">
              <span>
                <Icon name="check" size={34} stroke={2.4} />
              </span>
            </div>
            <h2 data-phase-title tabindex="-1">
              {format().toUpperCase()} ready
            </h2>
            <p class="export-path" data-tip={outPath()}>
              {outPath()}
            </p>
            <div class="done-actions">
              <button type="button" class="btn primary" onClick={() => void copyFile()}>
                <Icon name="copy" size={14} /> Copy file
              </button>
              <button type="button" class="btn" onClick={() => void copyPath()}>
                Copy path
              </button>
              <button type="button" class="btn" onClick={reveal}>
                <Icon name="folder" size={14} /> Show
              </button>
            </div>
            <p class="muted small">
              {copied() ||
                (format() === "gif"
                  ? "Copy, then paste the GIF into Slack, Discord or a GitHub comment."
                  : "Copy puts the MP4 on the clipboard as a file. If an app refuses to paste it, drag it in from the folder.")}
            </p>
          </div>
          <div class="modal-actions">
            <button type="button" class="btn" onClick={props.onClose}>
              Done
            </button>
          </div>
        </Show>
      </div>
    </div>
  );
}
