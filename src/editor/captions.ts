// Editor captions: the clip's timed captions, how they look, and making them from the
// narration. The first time, the engine downloads its speech model (about 57 MB), then it
// listens to the recording on this computer; both steps report progress and can be
// cancelled. Captions are plain timed text afterwards: every word can be fixed by hand.
import { createSignal } from "solid-js";
import { invoke, listen, save } from "../bridge";
import { friendlyError } from "../format";
import { prefs } from "../prefs";
import { createSyncSlot } from "../sync";
import type { Caption, CaptionStyle, CaptionsProgress, CaptionsStatus } from "../types";
import { toast } from "../ui";

export const DEFAULT_CAPTION_STYLE: CaptionStyle = { visible: true, size: 0.045, position: "bottom" };
export const CAPTION_MIN_SIZE = 0.025;
export const CAPTION_MAX_SIZE = 0.09;

/** Languages offered for new captions (the speech model knows many more). */
export const CAPTION_LANGUAGES: { value: string; label: string }[] = [
  { value: "auto", label: "Detect automatically" },
  { value: "en", label: "English" },
  { value: "es", label: "Spanish" },
  { value: "fr", label: "French" },
  { value: "de", label: "German" },
  { value: "it", label: "Italian" },
  { value: "pt", label: "Portuguese" },
  { value: "nl", label: "Dutch" },
  { value: "pl", label: "Polish" },
  { value: "tr", label: "Turkish" },
  { value: "ru", label: "Russian" },
  { value: "uk", label: "Ukrainian" },
  { value: "ar", label: "Arabic" },
  { value: "hi", label: "Hindi" },
  { value: "ur", label: "Urdu" },
  { value: "id", label: "Indonesian" },
  { value: "vi", label: "Vietnamese" },
  { value: "zh", label: "Chinese" },
  { value: "ja", label: "Japanese" },
  { value: "ko", label: "Korean" },
];

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
    return `Downloading the speech model… ${mb(j.done)} of ${mb(j.total)} MB`;
  }
  if (j.step === "listen") return `Listening to your recording… ${Math.round(j.done)}%`;
  return "Getting ready…";
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
      toast(`${made.length} captions added. Click one on the timeline to fix a word.`, "success");
    } catch (e) {
      const msg = friendlyError(e);
      if (!msg.includes("cancelled")) toast(msg, "error", 6000);
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
      toast(`${what} failed: ${friendlyError(e)}`, "error");
    }
    await edited();
  };

  // Typing lands locally at once; the engine gets the latest text.
  const textSync = createSyncSlot<{ id: number; text: string }>();
  const setText = (id: number, text: string) => {
    setList(list().map((c) => (c.id === id ? { ...c, text } : c)));
    textSync.push({ id, text }, (v) => run("Caption edit", () => invoke("set_caption_text", v)));
  };

  const setRange = async (id: number, start: number, end: number) => {
    const lo = Math.min(start, end);
    const hi = Math.max(start, end);
    setList(
      list()
        .map((c) => (c.id === id ? { ...c, range: { ...c.range, start: lo, end: hi } } : c))
        .sort((a, b) => a.range.start - b.range.start),
    );
    await run("Caption timing", () => invoke("set_caption_range", { id, start: lo, end: hi }));
  };

  const remove = async (id: number) => {
    setList(list().filter((c) => c.id !== id));
    await run("Delete caption", () => invoke("delete_caption", { id }));
  };

  const clear = async () => {
    setList([]);
    await run("Remove captions", () => invoke("clear_captions"));
  };

  /** Add an empty caption at `t` and return its id (null if it failed). */
  const add = async (t: number, text: string): Promise<number | null> => {
    try {
      const id = await invoke<number>("add_caption", { t, text });
      const cs = await invoke<{ captions?: Caption[] }>("clip_state");
      setList(cs.captions ?? []);
      await edited();
      return id;
    } catch (e) {
      toast(`Could not add a caption: ${friendlyError(e)}`, "error");
      return null;
    }
  };

  // Slider drags: the look changes locally this frame, the engine gets the latest value.
  const styleSync = createSyncSlot<CaptionStyle>();
  const updateStyle = (patch: Partial<CaptionStyle>) => {
    const next = { ...style(), ...patch };
    setStyle(next);
    styleSync.push(next, (v) => run("Caption style", () => invoke("set_caption_style", { style: v })));
  };

  /** Save the captions as an .srt subtitle file, timed to the exported video. */
  const saveSrt = async () => {
    const path = await save({
      defaultPath: "captions.srt",
      filters: [{ name: "Subtitles", extensions: ["srt"] }],
    });
    if (!path) return;
    try {
      await invoke("export_srt", { path });
      toast("Captions saved. Upload the .srt next to your video.", "success");
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
