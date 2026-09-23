// Editor audio: the clip's recorded tracks, their waveforms, and preview playback.
//
// The playhead runs in source time: it jumps over cuts, races through sped-up spans and
// loops. Rather than scheduling all of that ahead of time, playback follows the playhead.
// While it moves at 1x through audible time, each track plays from wherever the playhead
// is; in a sped-up span the tracks stop (the export mutes those spans too); and whenever
// the playhead and the audio disagree by more than a blink (a seek, a cut jump, a loop,
// a rate change) the track restarts at the right spot.
import { createSignal } from "solid-js";
import { invoke } from "../bridge";
import type { AudioKind, AudioTrack } from "../types";

/** Waveform resolution: peak buckets per second of audio. */
export const PEAK_RATE = 100;
/** Playhead/audio disagreement that triggers a restart, in seconds. */
const RESYNC = 0.12;

export interface TrackData {
  buffer: AudioBuffer;
  /** Peak absolute level per 1/PEAK_RATE s, 0..1. */
  peaks: Float32Array;
}

interface Voice {
  gain: GainNode;
  src: AudioBufferSourceNode | null;
  /** Context time and buffer position (s) the current source started at. */
  startCtx: number;
  startPos: number;
  rate: number;
}

/** Peak level per bucket across all channels. */
export function computePeaks(buf: AudioBuffer): Float32Array {
  const out = new Float32Array(Math.max(1, Math.ceil(buf.duration * PEAK_RATE)));
  const per = buf.sampleRate / PEAK_RATE;
  for (let c = 0; c < buf.numberOfChannels; c++) {
    const ch = buf.getChannelData(c);
    for (let i = 0; i < ch.length; i++) {
      const b = Math.min(out.length - 1, Math.floor(i / per));
      const v = Math.abs(ch[i]);
      if (v > out[b]) out[b] = v;
    }
  }
  return out;
}

export const trackLabel = (k: AudioKind) => (k === "mic" ? "Microphone" : "System sound");
export const effectiveGain = (t: AudioTrack) => (t.muted ? 0 : Math.max(0, Math.min(4, t.gain)));

export function createAudio(opts: { onEdit: () => void }) {
  const [tracks, setTracks] = createSignal<AudioTrack[]>([]);
  const [data, setData] = createSignal<Partial<Record<AudioKind, TrackData>>>({});
  let ctx: AudioContext | null = null;
  const voices = new Map<AudioKind, Voice>();
  let gen = 0;

  const context = (): AudioContext | null => {
    if (ctx) return ctx;
    try {
      ctx = new AudioContext();
    } catch {
      ctx = null; // no audio output available: the editor stays silent
    }
    return ctx;
  };

  const stopVoice = (v: Voice) => {
    if (!v.src) return;
    try {
      v.src.stop();
    } catch {
      /* already stopped */
    }
    v.src.disconnect();
    v.src = null;
  };
  const stop = () => {
    for (const v of voices.values()) stopVoice(v);
  };

  /** Load a freshly opened clip's tracks (decoding each WAV once). */
  const load = async (list: AudioTrack[]) => {
    const g = ++gen;
    stop();
    setTracks(list);
    setData({});
    const c = context();
    if (!c) return;
    const loaded: Partial<Record<AudioKind, TrackData>> = {};
    for (const t of list) {
      try {
        const bytes = await invoke<ArrayBuffer>("audio_track", { kind: t.kind });
        const buffer = await c.decodeAudioData(bytes);
        if (g !== gen) return;
        loaded[t.kind] = { buffer, peaks: computePeaks(buffer) };
        setData({ ...loaded });
      } catch {
        /* unreadable track: its lane stays empty and it plays nothing */
      }
    }
  };

  const unload = () => {
    gen++;
    stop();
    setTracks([]);
    setData({});
  };

  /** Adopt the engine's track settings (after undo/redo or a clip-state refresh). */
  const adopt = (list: AudioTrack[]) => {
    const same = list.length === tracks().length && list.every((t, i) => t.kind === tracks()[i]?.kind);
    if (!same) {
      void load(list);
      return;
    }
    setTracks(list);
    for (const t of list) {
      const v = voices.get(t.kind);
      if (v) v.gain.gain.value = effectiveGain(t);
    }
  };

  /**
   * Keep playback in step with the playhead. `t` is the playhead (source seconds), `speed`
   * the clip's speed factor there and `rate` the preview rate.
   */
  const sync = (playing: boolean, t: number, speed: number, rate: number) => {
    if (!playing || Math.abs(speed - 1) > 1e-6 || tracks().length === 0) {
      stop();
      return;
    }
    const c = context();
    if (!c) return;
    // A suspended context's clock stands still; scheduling against it would restart every
    // track each frame. Ask it to run and pick up on a later tick.
    if (c.state !== "running") {
      void c.resume();
      return;
    }
    for (const tr of tracks()) {
      const d = data()[tr.kind];
      if (!d) continue;
      let v = voices.get(tr.kind);
      if (!v) {
        const gain = c.createGain();
        gain.connect(c.destination);
        v = { gain, src: null, startCtx: 0, startPos: 0, rate: 1 };
        voices.set(tr.kind, v);
      }
      v.gain.gain.value = effectiveGain(tr);
      const pos = t - tr.offset;
      const heard = v.src ? v.startPos + (c.currentTime - v.startCtx) * v.rate : Number.NaN;
      if (v.src && v.rate === rate && Math.abs(heard - pos) <= RESYNC) continue;
      stopVoice(v);
      if (pos >= d.buffer.duration) continue;
      const src = c.createBufferSource();
      src.buffer = d.buffer;
      src.playbackRate.value = rate;
      src.connect(v.gain);
      // Before the track's first sample: start it late instead of mid-buffer.
      if (pos < 0) src.start(c.currentTime - pos / rate, 0);
      else src.start(c.currentTime, pos);
      v.src = src;
      v.startCtx = c.currentTime;
      v.startPos = pos;
      v.rate = rate;
    }
  };

  const update = async (kind: AudioKind, patch: Partial<Pick<AudioTrack, "gain" | "muted">>) => {
    const cur = tracks().find((t) => t.kind === kind);
    if (!cur) return;
    const next = { ...cur, ...patch };
    setTracks(tracks().map((t) => (t.kind === kind ? next : t)));
    const v = voices.get(kind);
    if (v) v.gain.gain.value = effectiveGain(next);
    opts.onEdit();
    try {
      await invoke("set_audio_track", { kind, gain: next.gain, muted: next.muted });
    } catch {
      /* engine unavailable: the local change still plays */
    }
  };

  return {
    tracks,
    data,
    hasAudio: () => tracks().length > 0,
    load,
    unload,
    adopt,
    sync,
    stop,
    /** Wake the audio output from a user gesture (the Play click), per autoplay rules. */
    prime: () => {
      if (tracks().length > 0) void context()?.resume();
    },
    setGain: (kind: AudioKind, gain: number) => update(kind, { gain }),
    toggleMute: (kind: AudioKind) => {
      const t = tracks().find((x) => x.kind === kind);
      if (t) void update(kind, { muted: !t.muted });
    },
  };
}

export type EditorAudio = ReturnType<typeof createAudio>;
