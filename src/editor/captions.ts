// Editor captions: the clip's timed captions, how they look, and making them from the
// narration. The first time, the engine downloads its speech model (about 57 MB), then it
// listens to the recording on this computer; both steps report progress and can be
// cancelled. Captions are plain timed text afterwards: every word can be fixed by hand.
import { createSignal } from "solid-js";
import { invoke, listen, save } from "../bridge";
import { friendlyError } from "../format";
import { locale, t } from "../i18n";
import { prefs } from "../prefs";
import { createSyncSlot } from "../sync";
import type { Caption, CaptionStyle, CaptionsProgress, CaptionsStatus } from "../types";
import { toast } from "../ui";

export const DEFAULT_CAPTION_STYLE: CaptionStyle = { visible: true, size: 0.045, position: "bottom" };
export const CAPTION_MIN_SIZE = 0.025;
export const CAPTION_MAX_SIZE = 0.09;

/** Languages offered for new captions (the speech model knows many more). Named by the
 *  platform, in the interface language. */
const LANGUAGE_CODES = ["en", "es", "fr", "de", "it", "pt", "nl", "pl", "tr", "ru", "uk", "ar", "hi", "ur", "id", "vi", "zh", "ja", "ko"];
export const CAPTION_LANGUAGES = (): { value: string; label: string }[] => {
  const names = new Intl.DisplayNames([locale()], { type: "language" });
  return [
    { value: "auto", label: t("Detect automatically") },
    ...LANGUAGE_CODES.map((value) => ({ value, label: names.of(value) ?? value })),
  ];
};

/** A captions job in progress: starting up, fetching the speech model, or listening. */
export interface CaptionsJob {
  step: "start" | "download" | "listen";
  done: number;
  total: number;
}

/** What a running job tells the user, e.g. "Downloading the speech model… 12 of 57 MB". */
export function jobLabel(j: CaptionsJob): string {
  if (j.step === "download") {
    const mb = (b: number) => Math.round(b / 1_000_000);
    return t("Downloading the speech model… {done} of {total} MB", { done: mb(j.done), total: mb(j.total) });
  }
  if (j.step === "listen") return t("Listening to your recording… {done}%", { done: Math.round(j.done) });
  return t("Getting ready…");
}

/** 0..1 progress of a job (0 while starting). */
export const jobFraction = (j: CaptionsJob) => (j.total > 0 ? Math.min(1, j.done / j.total) : 0);

export function createCaptions(opts: {
  hasClip: () => boolean;
  /** Called after any change, to mark the project edited and repaint the preview. */
  onEdit: () => Promise<void> | void;
}) {
  const [list, setList] = createSignal<Caption[]>([]);
  const [style, setStyle] = createSignal<CaptionStyle>(DEFAULT_CAPTION_STYLE);
  const [status, setStatus] = createSignal<CaptionsStatus | null>(null);
  const [job, setJob] = createSignal<CaptionsJob | null>(null);

  /** Take the engine's captions (clip load, undo and redo re-sync through here). */
  const adopt = (captions: Caption[] | undefined, st: CaptionStyle | undefined) => {
    setList(captions ?? []);
    setStyle(st ?? DEFAULT_CAPTION_STYLE);
  };

  const loadStatus = async () => {
    try {
      setStatus(await invoke<CaptionsStatus>("captions_status"));
    } catch {
      /* engine starting */
    }
  };

  const edited = async () => {
    await opts.onEdit();
  };

  /** Make captions from the narration, replacing any the clip has. */
  const generate = async () => {
    if (job() || !opts.hasClip()) return;
    setJob({ step: "start", done: 0, total: 0 });
    const unlisten = await listen<CaptionsProgress>("captions-progress", (p) => {
      if (job()) setJob({ step: p.step, done: p.done, total: p.total });
    });
    try {
      const made = await invoke<Caption[]>("generate_captions", { language: prefs.captionLanguage() });
      setList(made);
      await edited();
      toast(t("{count} captions added. Click one on the timeline to fix a word.", { count: made.length }), "success");
    } catch (e) {
      if (!String(e).includes("cancelled")) toast(friendlyError(e), "error", 6000);
    } finally {
      unlisten();
      setJob(null);
      void loadStatus();
    }
  };

  const cancel = () => {
    if (job()) void invoke("cancel_captions").catch(() => undefined);
  };

  const run = async (what: string, fn: () => Promise<unknown>) => {
    try {
      await fn();
    } catch (e) {
      toast(t("{what}: {why}", { what, why: friendlyError(e) }), "error");
    }
    await edited();
  };

  // Typing lands locally at once; the engine gets the latest text.
  const textSync = createSyncSlot<{ id: number; text: string }>();
  const setText = (id: number, text: string) => {
    setList(list().map((c) => (c.id === id ? { ...c, text } : c)));
    textSync.push({ id, text }, (v) => run(t("Caption edit failed"), () => invoke("set_caption_text", v)));
  };

  const setRange = async (id: number, start: number, end: number) => {
    const lo = Math.min(start, end);
    const hi = Math.max(start, end);
    setList(
      list()
        .map((c) => (c.id === id ? { ...c, range: { ...c.range, start: lo, end: hi } } : c))
        .sort((a, b) => a.range.start - b.range.start),
    );
    await run(t("Caption timing failed"), () => invoke("set_caption_range", { id, start: lo, end: hi }));
  };

  const remove = async (id: number) => {
    setList(list().filter((c) => c.id !== id));
    await run(t("Delete caption failed"), () => invoke("delete_caption", { id }));
  };

  const clear = async () => {
    setList([]);
    await run(t("Remove captions failed"), () => invoke("clear_captions"));
  };

  /** Add an empty caption at `t` and return its id (null if it failed). */
  const add = async (time: number, text: string): Promise<number | null> => {
    try {
      const id = await invoke<number>("add_caption", { t: time, text });
      const cs = await invoke<{ captions?: Caption[] }>("clip_state");
      setList(cs.captions ?? []);
      await edited();
      return id;
    } catch (e) {
      toast(t("Could not add a caption: {why}", { why: friendlyError(e) }), "error");
      return null;
    }
  };

  // Slider drags: the look changes locally this frame, the engine gets the latest value.
  const styleSync = createSyncSlot<CaptionStyle>();
  const updateStyle = (patch: Partial<CaptionStyle>) => {
    const next = { ...style(), ...patch };
    setStyle(next);
    styleSync.push(next, (v) => run(t("Caption style failed"), () => invoke("set_caption_style", { style: v })));
  };

  /** Save the captions as an .srt subtitle file, timed to the exported video. */
  const saveSrt = async () => {
    const path = await save({
      defaultPath: "captions.srt",
      filters: [{ name: t("Subtitles"), extensions: ["srt"] }],
    });
    if (!path) return;
    try {
      await invoke("export_srt", { path });
      toast(t("Captions saved. Upload the .srt next to your video."), "success");
    } catch (e) {
      toast(friendlyError(e), "error");
    }
  };

  /** The caption showing at source time `t`, if any (the later one where two overlap). */
  const at = (t: number) => {
    const all = list();
    for (let i = all.length - 1; i >= 0; i--) {
      const c = all[i];
      if (t >= c.range.start && t < c.range.end) return c;
    }
    return null;
  };

  return {
    list,
    style,
    status,
    job,
    adopt,
    loadStatus,
    generate,
    cancel,
    setText,
    setRange,
    remove,
    clear,
    add,
    updateStyle,
    saveSrt,
    at,
  };
}

export type Captions = ReturnType<typeof createCaptions>;
