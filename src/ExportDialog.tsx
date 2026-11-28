import { batch, createEffect, createSignal, on, onCleanup, Show, untrack, type JSX } from "solid-js";
import { invoke, listen, revealItemInDir } from "./bridge";
import { t } from "./i18n";
import { pickSavePath } from "./saveDir";
import { Icon } from "./icons";
import { type ExportFormat, prefs } from "./prefs";
import { Field, IconButton, Slider, Switch, toast } from "./ui";
import { dialogA11y } from "./dialog";
import { fmtBytes, friendlyError, timeLeft } from "./format";
import { outputDuration } from "./geometry";
import { createSyncSlot } from "./sync";
import type { ClipState, SpeedRegion, Trim } from "./types";

/** The widest GIF or WebP on offer (a wider one is huge and few chat apps play it smoothly). */
const GIF_MAX_WIDTH = 1920;
const WEBP_MAX_WIDTH = 1920;
/** The largest frame H.264 encoders take: no side over 4096, and at most 4096 x 2304 pixels. */
const H264_MAX_SIDE = 4096;
const H264_MAX_PIXELS = 4096 * 2304;
/** The widest MP4 of a `w` x `h` picture: its own width, or what H.264 can hold of that shape. */
const mp4Width = (w: number, h: number): number =>
  Math.floor(w * Math.min(1, H264_MAX_SIDE / w, H264_MAX_SIDE / h, Math.sqrt(H264_MAX_PIXELS / (w * h))));
/** The width slider's step. */
const WIDTH_STEP = 20;

/**
 * The dialog's steps. `starting` holds while the OS save dialog is open.
 *
 *   configure ─Export─▶ starting ─path chosen─▶ exporting ─▶ done
 *   starting ─no path─▶ configure          exporting ─cancelled─▶ configure
 *   starting / exporting ─failed─▶ error ─Try again─▶ starting
 *   error ─Back to settings─▶ configure
 */
type Phase = "configure" | "starting" | "exporting" | "done" | "error";

export function ExportDialog(props: {
  name: string;
  duration: number;
  /** Real width / height of the clip, so MP4 size math is honest for every ratio. */
  aspect: number;
  trim: Trim | null;
  speed: SpeedRegion[];
  cuts: Trim[];
  /** The clip has at least one unmuted audio track. */
  hasAudio: boolean;
  /** The clip has captions, which can also be saved as an .srt beside the export. */
  hasCaptions?: boolean;
  onClose: () => void;
  onStatus: (s: string) => void;
  onExported: () => void;
}): JSX.Element {
  const [format, setFormat] = createSignal<ExportFormat>(prefs.exportFormat());
  const [preset, setPreset] = createSignal<"readme" | "hq" | "custom">("readme");
  const [fps, setFps] = createSignal(15);
  const [width, setWidth] = createSignal(1000);
  const [quality, setQuality] = createSignal(80);
  const [phase, setPhase] = createSignal<Phase>("configure");
  const [progress, setProgress] = createSignal(0);
  // For the time-left line: when the export began, and a clock that ticks only while running.
  const [startedAt, setStartedAt] = createSignal(0);
  const [now, setNow] = createSignal(0);
  createEffect(() => {
    if (phase() !== "exporting") return;
    setNow(Date.now());
    let timer: number | undefined;
    const arm = () => {
      clearTimeout(timer);
      if (!document.hidden) {
        timer = window.setTimeout(() => {
          setNow(Date.now());
          arm();
        }, 1000);
      }
    };
    const onVisibility = () => {
      if (document.hidden) {
        clearTimeout(timer);
        timer = undefined;
      } else {
        setNow(Date.now());
        arm();
      }
    };
    arm();
    document.addEventListener("visibilitychange", onVisibility);
    onCleanup(() => {
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", onVisibility);
    });
  });
  const [estimate, setEstimate] = createSignal<number | null>(null); // -1 = unavailable
  const [outPath, setOutPath] = createSignal("");
  const [copied, setCopied] = createSignal("");
  const [errMsg, setErrMsg] = createSignal("");
  const [budgetMb, setBudgetMb] = createSignal("");
  const [fitting, setFitting] = createSignal(false);
  const [withAudio, setWithAudio] = createSignal(true);
  const [srtSaved, setSrtSaved] = createSignal("");
  const audioOn = () => props.hasAudio && withAudio();

  // The take's full picture width and the frame rate it was recorded at, from the engine:
  // the most an export can hold. Until they arrive (and on engines that don't say), a
  // 1080p take at 30 fps is assumed.
  const [full, setFull] = createSignal({ width: 1920, height: 1080, fps: 30 });
  void invoke<ClipState>("clip_state")
    .then((cs) => {
      if (cs.out_width && cs.out_width > 0) {
        setFull({ width: cs.out_width, height: cs.out_height ?? 0, fps: cs.source_fps ?? 30 });
      }
    })
    .catch(() => undefined);
  /** The widest this format exports the take: its own width, up to the format's limit. */
  const widest = () => {
    const { width: w, height: h } = full();
    if (format() === "gif") return Math.min(w, GIF_MAX_WIDTH);
    if (format() === "webp") return Math.min(w, WEBP_MAX_WIDTH);
    return h > 0 ? mp4Width(w, h) : Math.min(w, H264_MAX_SIDE);
  };
  /** The slider's top stop: the widest, rounded up to a whole step so it can be reached. */
  const sliderMax = () => Math.max(400, Math.ceil(widest() / WIDTH_STEP) * WIDTH_STEP);
  /** The width the export really gets: never wider than the take. */
  const outWidth = () => Math.min(width(), widest());

  let dialogEl: HTMLDivElement | undefined;

  const outDur = () => outputDuration(props.duration, props.trim, props.speed, props.cuts);

  // MP4 size follows directly from the bitrate (mirrors src-tauri mp4::bitrate) and uses
  // the REAL clip aspect, not an assumed 16:9.
  const mp4Estimate = () => {
    const q = Math.min(100, Math.max(40, quality()));
    const bpp = 0.04 + ((q - 40) / 60) * 0.16;
    const h = Math.round(outWidth() / Math.max(props.aspect || 16 / 9, 0.2));
    const bits = outWidth() * h * fps() * bpp;
    // Plus the AAC soundtrack (192 kbps) when it's included.
    const audioBytes = audioOn() ? 24_000 * outDur() : 0;
    return Math.min(Math.max(bits, 1_000_000), 50_000_000) * (outDur() / 8) + audioBytes;
  };

  // Live size estimate: GIF and WebP sample-and-extrapolate (debounced + latest-wins
  // serialized via createSyncSlot); MP4 is closed-form. A failed probe shows "Estimate
  // unavailable" instead of a fake 0 B.
  let estimateTimer: number | undefined;
  let estimateGen = 0;
  const estimateSlot = createSyncSlot<{
    gen: number;
    cmd: "estimate_gif" | "estimate_webp";
    args: Record<string, unknown>;
  }>();
  createEffect(() => {
    const f = format();
    const w = outWidth();
    const q = quality();
    const r = fps();
    const dither = prefs.gifDither();
    const lossless = prefs.webpLossless();
    setEstimate(null);
    clearTimeout(estimateTimer);
    const gen = ++estimateGen;
    if (f === "mp4") {
      setEstimate(mp4Estimate());
      return;
    }
    estimateTimer = window.setTimeout(() => {
      const cmd = f === "webp" ? "estimate_webp" : "estimate_gif";
      const args = f === "webp" ? { fps: r, width: w, quality: q, lossless } : { fps: r, width: w, quality: q, dither };
      estimateSlot.push({ gen, cmd, args }, async (req, superseded) => {
        if (superseded() || req.gen !== estimateGen) return;
        try {
          const b = await invoke<number>(req.cmd, req.args);
          if (req.gen === estimateGen) setEstimate(Math.max(1, Math.round(b)));
        } catch {
          if (req.gen === estimateGen) setEstimate(-1);
        }
      });
    }, 350);
  });
  onCleanup(() => clearTimeout(estimateTimer));

  // Presets are FORMAT-AWARE: the same named preset means different fps/widths for GIF,
  // WebP and MP4, so the sliders never contradict the selected preset chip.
  const presetValues = (p: "readme" | "hq"): { fps: number; width: number; quality: number } => {
    const f = format();
    if (f === "mp4") {
      return p === "readme"
        ? { fps: 30, width: Math.min(widest(), 1920), quality: 85 }
        : { fps: full().fps >= 50 ? 60 : 30, width: widest(), quality: 95 };
    }
    if (f === "webp") {
      return p === "readme"
        ? { fps: 20, width: Math.min(widest(), 1280), quality: 80 }
        : { fps: 30, width: Math.min(widest(), 1920), quality: 92 };
    }
    return p === "readme"
      ? { fps: 15, width: Math.min(widest(), 1000), quality: 80 }
      : { fps: 20, width: Math.min(widest(), 1280), quality: 95 };
  };

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
  const prefSlot = (f: ExportFormat) => (f === "gif" ? prefs.exportGif : f === "webp" ? prefs.exportWebp : prefs.exportMp4);
  const restore = (f: ExportFormat) => {
    const s = prefSlot(f)();
    if (!s) {
      applyPreset("readme");
      return;
    }
    // A named preset means what it means for THIS take; only Custom keeps its numbers.
    if (s.preset !== "custom") {
      applyPreset(s.preset);
      return;
    }
    setPreset(s.preset);
    setFps(s.fps);
    setWidth(s.width);
    setQuality(s.quality);
  };
  restore(format());
  // The take's size arrived: a named preset follows it.
  createEffect(
    on(
      full,
      () => {
        const p = untrack(preset);
        if (p !== "custom") applyPreset(p);
      },
      { defer: true },
    ),
  );
  createEffect(() => {
    const s = { preset: preset(), fps: fps(), width: width(), quality: quality() };
    prefSlot(format()).set(s);
  });

  // Switching format restores that format's own last settings.
  const switchFormat = (f: ExportFormat) => {
    if (f === format()) return;
    // One batch, so the save effect never sees the new format with the old values.
    batch(() => {
      setFormat(f);
      restore(f);
    });
  };

  // E2: fit GIF or WebP into an explicit byte budget by probing the engine's estimator for
  // the largest width that fits (then relaxing quality if nothing fits). Abortable so
  // input changes, format swaps, export start, or dialog unmount never emit stale updates.
  let fitAbort: AbortController | null = null;
  const cancelFit = () => {
    if (fitAbort) {
      fitAbort.abort();
      fitAbort = null;
    }
    setFitting(false);
  };
  onCleanup(cancelFit);
  createEffect(
    on([format, fps, width, quality, () => prefs.gifDither(), () => prefs.webpLossless()], () => {
      if (fitAbort) cancelFit();
    }, { defer: true }),
  );

  const fitToBudget = async () => {
    const mb = Number(budgetMb());
    const f = format();
    if (!Number.isFinite(mb) || mb <= 0 || (f !== "gif" && f !== "webp")) return;
    cancelFit();
    const ctrl = new AbortController();
    fitAbort = ctrl;
    const snapFps = fps();
    const snapWidth = outWidth();
    const snapQuality = quality();
    const snapLossless = prefs.webpLossless();
    const snapDither = prefs.gifDither();
    setFitting(true);
    try {
      const budget = mb * 1024 * 1024;
      const widths = [snapWidth, 1280, 1000, 900, 800, 700, 600, 500, 400]
        .filter((w, i, arr) => w <= Math.max(snapWidth, 400) && arr.indexOf(w) === i)
        .sort((a, b) => b - a);
      const qualities = [snapQuality, 75, 65, 55, 45];
      const cmd = f === "webp" ? "estimate_webp" : "estimate_gif";
      for (const q of qualities) {
        for (const w of widths) {
          if (ctrl.signal.aborted) return;
          const est = await invoke<number>(
            cmd,
            f === "webp"
              ? { fps: snapFps, width: w, quality: q, lossless: snapLossless }
              : { fps: snapFps, width: w, quality: q, dither: snapDither },
          ).catch(() => Number.POSITIVE_INFINITY);
          if (ctrl.signal.aborted) return;
          if (est > 0 && est <= budget) {
            fitAbort = null;
            batch(() => {
              setWidth(w);
              setQuality(q);
              setPreset("custom");
              setFitting(false);
            });
            toast(t("Fitted to about {est} at {w}px", { est: fmtBytes(est), w }), "success");
            return;
          }
        }
      }
      if (!ctrl.signal.aborted) {
        toast(t("Could not reach that size. Try a lower frame rate."), "error");
      }
    } finally {
      if (fitAbort === ctrl) {
        fitAbort = null;
        setFitting(false);
      }
    }
  };

  const formatLabel = (f: ExportFormat = format()) => (f === "gif" ? "GIF" : f === "webp" ? "WebP" : "MP4");

  const doExport = async () => {
    // Starts from the settings or from a failure's Try again; a second click finds the
    // phase already moved on to "starting".
    if (phase() !== "configure" && phase() !== "error") return;
    cancelFit();
    const f = format();
    const label = formatLabel(f);
    const safe = props.name.replace(/[^\w.-]+/g, "-").replace(/^-+|-+$/g, "") || "vuoom";
    setPhase("starting"); // dialog locks while the OS save dialog is open
    try {
      const path = await pickSavePath(
        safe,
        f,
        f === "gif"
          ? { name: "GIF", extensions: ["gif"] }
          : f === "webp"
            ? { name: "WebP", extensions: ["webp"] }
            : { name: t("MP4 video"), extensions: ["mp4"] },
      );
      if (!path) {
        setPhase("configure");
        return;
      }
      setStartedAt(Date.now());
      setNow(Date.now());
      setPhase("exporting");
      setProgress(0);
      props.onStatus(t("Exporting {format}...", { format: label }));
      // The listener is registered INSIDE the guarded flow and always cleaned up.
      const unlisten = await listen<{ done: number; total: number }>("export-progress", (p) => {
        setProgress(p.total > 0 ? p.done / p.total : 0);
      });
      try {
        const cmd = f === "gif" ? "export_gif" : f === "webp" ? "export_webp" : "export_mp4";
        const extra =
          f === "mp4"
            ? { audio: audioOn() }
            : f === "webp"
              ? { lossless: prefs.webpLossless() }
              : { dither: prefs.gifDither() };
        await invoke(cmd, {
          path,
          fps: fps(),
          width: outWidth(),
          quality: quality(),
          ...extra,
        });
        setOutPath(path);
        setPhase("done");
        props.onExported();
        props.onStatus(t("Exported {path}", { path }));
        toast(t("{format} exported", { format: label }), "success");
      } catch (e) {
        const msg = String(e);
        props.onStatus(
          msg.includes("export cancelled") ? t("Export cancelled") : t("Export failed: {why}", { why: friendlyError(e) }),
        );
        if (msg.includes("export cancelled")) {
          setPhase("configure");
        } else {
          setErrMsg(friendlyError(e));
          setPhase("error"); // inline failure state with Retry, never a silent reset
          toast(t("Export failed: {why}", { why: friendlyError(e) }), "error");
        }
      } finally {
        unlisten();
      }
    } catch (e) {
      setErrMsg(friendlyError(e));
      setPhase("error");
    }
  };

  // Abort an in-flight export. The backend loop bails at its next frame check, deletes the
  // partial file, and the invoke rejects with "export cancelled" (handled in doExport).
  const cancelExport = () => void invoke("cancel_export").catch(() => undefined);

  const copyFile = async () => {
    try {
      await invoke("copy_export_to_clipboard", { path: outPath() });
      // Honest per-format copy: GIFs paste as animations almost everywhere; WebP pastes in
      // modern browsers/chat apps; MP4s are copied as a file.
      setCopied(
        format() === "gif"
          ? t("Copied! Paste it into Slack, Discord, or a GitHub comment.")
          : format() === "webp"
            ? t("Copied! Paste it into Discord, Slack, or a browser, or drag it in from Show in folder.")
            : t("Copied as a file. If pasting doesn't work, drag it in from Show in folder."),
      );
      toast(t("Copied to clipboard"), "success");
    } catch (e) {
      setCopied(t("Copy failed: {why}", { why: friendlyError(e) }));
      toast(t("Copy failed: {why}", { why: friendlyError(e) }), "error");
    }
  };
  const copyPath = async () => {
    try {
      await navigator.clipboard.writeText(outPath());
      setCopied(t("Path copied."));
    } catch {
      setCopied(t("Could not copy the path."));
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
  /** Save the captions as `<export name>.srt` beside the export: players pick that up. */
  const saveSrt = async () => {
    const path = `${outPath().replace(/\.[^.\\/]+$/, "")}.srt`;
    try {
      await invoke("export_srt", { path });
      setSrtSaved(path.split(/[\\/]/).pop() ?? path);
    } catch (e) {
      toast(friendlyError(e), "error");
    }
  };
  const sizeText = () =>
    estimate() === null ? t("Estimating…") : estimate()! < 0 ? t("Unavailable") : `≈ ${fmtBytes(estimate()!)}`;
  const height = () => Math.round(outWidth() / Math.max(props.aspect || 16 / 9, 0.2));

  return (
    <div class="modal-backdrop" onClick={() => !busy() && props.onClose()}>
      <div
        class="modal export"
        ref={(el) => {
          dialogEl = el;
          dialogA11y(el, t("Export"), () =>
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
                {t("Export")}
              </h2>
              <p class="muted small">{t("Zooms, annotations, speed-ups and cuts are baked into the file.")}</p>
            </div>
            <IconButton icon="close" size={14} tip={t("Close")} onClick={props.onClose} />
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
                <small>{t("Animated image")}</small>
              </span>
            </button>
            <button
              type="button"
              class="export-format"
              classList={{ on: format() === "webp" }}
              aria-pressed={format() === "webp"}
              onClick={() => switchFormat("webp")}
            >
              <span class="export-format-icon">
                <Icon name="webp" size={20} />
              </span>
              <span>
                <strong>WebP</strong>
                <small>{t("Compact animated image")}</small>
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
                <strong>{t("MP4 video")}</strong>
                <small>{t("Video with audio")}</small>
              </span>
            </button>
          </div>

          <div class="export-presets">
            {(() => {
              const bal = () => presetValues("readme");
              const hq = () => presetValues("hq");
              const sameDims = () => bal().fps === hq().fps && bal().width === hq().width;
              return (
                <>
                  <button
                    type="button"
                    class="export-preset"
                    classList={{ on: preset() === "readme" }}
                    aria-pressed={preset() === "readme"}
                    onClick={() => applyPreset("readme")}
                  >
                    {t("Balanced")}
                    <small>
                      {sameDims() ? t("Quality {q}", { q: bal().quality }) : `${bal().fps} fps · ${bal().width}px`}
                    </small>
                  </button>
                  <button
                    type="button"
                    class="export-preset"
                    classList={{ on: preset() === "hq" }}
                    aria-pressed={preset() === "hq"}
                    onClick={() => applyPreset("hq")}
                  >
                    {t("High quality")}
                    <small>
                      {sameDims() ? t("Quality {q}", { q: hq().quality }) : `${hq().fps} fps · ${hq().width}px`}
                    </small>
                  </button>
                  <button
                    type="button"
                    class="export-preset"
                    classList={{ on: preset() === "custom" }}
                    aria-pressed={preset() === "custom"}
                    onClick={() => applyPreset("custom")}
                  >
                    {t("Custom")}
                    <small>{t("Tune every setting")}</small>
                  </button>
                </>
              );
            })()}
          </div>

          <div class="export-controls">
            <Field label={t("Frame rate")}>
              <Slider
                value={fps()}
                min={8}
                max={format() === "gif" ? 30 : 60}
                step={1}
                label={t("Frame rate")}
                format={(v) => `${v} fps`}
                onInput={(v) => {
                  setFps(v);
                  setPreset("custom");
                }}
              />
            </Field>
            <Field label={t("Width")}>
              <Slider
                value={width() >= widest() ? sliderMax() : width()}
                min={400}
                max={sliderMax()}
                step={WIDTH_STEP}
                label={t("Max width")}
                format={(v) => (v >= widest() ? t("{width}px (full size)", { width: widest() }) : `${v}px`)}
                onInput={(v) => {
                  setWidth(v);
                  setPreset("custom");
                }}
              />
            </Field>
            <Field label={t("Quality")}>
              <Slider
                value={quality()}
                min={40}
                max={100}
                step={1}
                label={t("Quality")}
                format={(v) => `${v}`}
                onInput={(v) => {
                  setQuality(v);
                  setPreset("custom");
                }}
              />
            </Field>
            <Show when={props.hasAudio}>
              <Show
                when={format() === "mp4"}
                fallback={
                  <p class="note export-audio-note">
                    <Icon name="volumeOff" size={13} />{" "}
                    {format() === "webp"
                      ? t("WebP animations are silent. Pick MP4 to keep the sound.")
                      : t("GIFs are silent. Pick MP4 to keep the sound.")}
                  </p>
                }
              >
                <Field label={t("Include audio")} hint={t("Narration and system sound, mixed as set in the Audio panel")}>
                  <Switch checked={withAudio()} label={t("Include audio")} onChange={setWithAudio} />
                </Field>
              </Show>
            </Show>
            <Show when={format() === "gif"}>
              <Field
                label={t("Smooth gradients")}
                hint={t("Dithers backdrops, shadows and the webcam so they don't show bands. Off makes a slightly smaller file")}
              >
                <Switch
                  checked={prefs.gifDither()}
                  label={t("Smooth gradients")}
                  onChange={(v) => prefs.gifDither.set(v)}
                />
              </Field>
            </Show>
            <Show when={format() === "webp"}>
              <Field
                label={t("Lossless")}
                hint={t("Pixel-exact text and UI with no compression artifacts. Larger than lossy WebP, still smaller than GIF")}
              >
                <Switch
                  checked={prefs.webpLossless()}
                  label={t("Lossless")}
                  onChange={(v) => prefs.webpLossless.set(v)}
                />
              </Field>
            </Show>
            <Show when={format() === "gif" || format() === "webp"}>
              <div class="export-fit">
                <span class="export-fit-label">
                  <Icon name="target" size={14} />
                  {t("Fit under")}
                </span>
                <div class="export-fit-input">
                  <input
                    class="input"
                    type="number"
                    min="0.2"
                    step="0.5"
                    placeholder="MB"
                    aria-label={t("Size budget in MB")}
                    value={budgetMb()}
                    onInput={(e) => setBudgetMb(e.currentTarget.value)}
                  />
                  <span class="export-fit-unit">MB</span>
                </div>
                <button
                  type="button"
                  class="btn sm export-fit-btn"
                  disabled={!Number(budgetMb()) || fitting()}
                  data-tip={t("Find the largest width and quality that stay under the budget")}
                  onClick={() => void fitToBudget()}
                >
                  {fitting() ? t("Fitting…") : t("Fit")}
                </button>
              </div>
            </Show>
          </div>

          <div class="export-summary">
            <div>
              <small>{t("Estimated size")}</small>
              <span class="export-size" role="status" aria-live="polite">
                {sizeText()}
              </span>
            </div>
            <div>
              <small>{t("Output")}</small>
              <span class="export-output">
                {outWidth()} × {height()} · {fps()} fps · {outDur().toFixed(1)}s
              </span>
            </div>
          </div>

          <div class="modal-actions">
            <button type="button" class="btn ghost" onClick={props.onClose}>
              {t("Cancel")}
            </button>
            <button type="button" class="btn primary" onClick={() => void doExport()}>
              <Icon name="export" size={14} /> {t("Export {format}", { format: formatLabel() })}
            </button>
          </div>
        </Show>

        <Show when={phase() === "starting"}>
          <div class="export-progress" role="status" aria-live="polite">
            <span class="spinner" aria-hidden="true" />
            <h2 data-phase-title tabindex="-1">
              {t("Choosing save location…")}
            </h2>
            <p class="muted small">{t("Pick a folder and a name for the {format}.", { format: formatLabel() })}</p>
          </div>
        </Show>

        <Show when={phase() === "exporting"}>
          <div class="export-progress">
            <div
              class="ring"
              style={{ "--p": pct() }}
              role="progressbar"
              aria-label={t("Export progress")}
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={pct()}
            >
              <span>{pct()}%</span>
            </div>
            <h2 data-phase-title tabindex="-1">
              {t("Exporting {format}", { format: formatLabel() })}
            </h2>
            <p class="muted small" role="status" aria-live="polite">
              {timeLeft(progress(), startedAt(), now()) || t("Rendering every frame with your zooms and annotations.")}
            </p>
            <button type="button" class="btn ghost" onClick={cancelExport}>
              {t("Cancel export")}
            </button>
          </div>
        </Show>

        <Show when={phase() === "error"}>
          <h2 data-phase-title tabindex="-1">
            {t("Export failed")}
          </h2>
          <p class="export-error" role="alert">
            {errMsg()}
          </p>
          <div class="modal-actions">
            <button type="button" class="btn ghost" onClick={() => setPhase("configure")}>
              {t("Back to settings")}
            </button>
            <button type="button" class="btn primary" onClick={() => void doExport()}>
              {t("Try again")}
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
              {t("{format} ready", { format: formatLabel() })}
            </h2>
            <p class="export-path" data-tip={outPath()}>
              {outPath()}
            </p>
            <div class="done-actions">
              <button type="button" class="btn primary" onClick={() => void copyFile()}>
                <Icon name="copy" size={14} /> {t("Copy file")}
              </button>
              <button type="button" class="btn" onClick={() => void copyPath()}>
                {t("Copy path")}
              </button>
              <button type="button" class="btn" onClick={reveal}>
                <Icon name="folder" size={14} /> {t("Show")}
              </button>
            </div>
            <Show when={props.hasCaptions}>
              <button type="button" class="btn ghost" disabled={!!srtSaved()} onClick={() => void saveSrt()}>
                <Icon name={srtSaved() ? "check" : "captions"} size={14} />{" "}
                {srtSaved() ? t("Saved {path}", { path: srtSaved() }) : t("Save captions next to it (.srt)")}
              </button>
            </Show>
            <p class="muted small" role="status" aria-live="polite">
              {copied() ||
                (format() === "gif"
                  ? t("Copy, then paste the GIF into Slack, Discord or a GitHub comment.")
                  : format() === "webp"
                    ? t("Copy, then paste the WebP into Discord, Slack or a browser.")
                    : t("Copy puts the MP4 on the clipboard as a file. If an app refuses to paste it, drag it in from the folder."))}
            </p>
          </div>
          <div class="modal-actions">
            <button type="button" class="btn" onClick={props.onClose}>
              {t("Done")}
            </button>
          </div>
        </Show>
      </div>
    </div>
  );
}
