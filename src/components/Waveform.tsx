// One audio track's waveform across the timeline. Bars scale with the track's volume, and
// everything that won't be heard is drawn faint: cuts, the trimmed-off ends, sped-up spans
// (they play silent) and the whole track when it's muted.
import { createEffect, onCleanup, onMount } from "solid-js";
import { PEAK_RATE, effectiveGain, type TrackData } from "../editor/audio";
import type { AudioTrack, SpeedRegion, Trim } from "../types";

/** Widest backing canvas; deeper timeline zoom stretches it (browsers cap canvas size). */
const MAX_W = 8192;

export default function Waveform(props: {
  data: TrackData | undefined;
  track: AudioTrack;
  duration: number;
  trim: Trim | null;
  cuts: Trim[];
  speed: SpeedRegion[];
  /** Any value that changes with the theme, so colors are re-read. */
  theme: unknown;
}) {
  let canvas!: HTMLCanvasElement;
  let widthPx = 0;

  const draw = () => {
    const d = props.data;
    const ctx = canvas.getContext("2d");
    if (!ctx || widthPx <= 0) return;
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    const w = Math.max(1, Math.min(MAX_W, Math.round(widthPx * dpr)));
    const h = Math.max(1, Math.round(canvas.clientHeight * dpr));
    if (canvas.width !== w) canvas.width = w;
    if (canvas.height !== h) canvas.height = h;
    ctx.clearRect(0, 0, w, h);
    const dur = props.duration;
    if (!d || dur <= 0) return;

    const css = getComputedStyle(canvas);
    const live = css.getPropertyValue("--t-audio").trim() || "#2fbfa8";
    const faint = css.getPropertyValue("--line-strong").trim() || "#444";
    const gain = effectiveGain(props.track);
    const muted = gain === 0;
    const t0 = props.trim?.start ?? 0;
    const t1 = props.trim?.end ?? dur;
    const heard = (t: number) =>
      !muted &&
      t >= t0 &&
      t <= t1 &&
      !props.cuts.some((c) => t >= c.start && t < c.end) &&
      !props.speed.some((r) => t >= r.start && t < r.end && r.factor !== 1);

    const peaks = d.peaks;
    const mid = h / 2;
    const bar = Math.max(2, Math.round(2 * dpr));
    const step = bar + Math.max(1, Math.round(dpr));
    // Unmuted bars show the volume setting; muted ones keep their shape so the lane
    // still reads as speech.
    const scale = muted ? 1 : Math.min(gain, 2.5);
    let lastLive: boolean | null = null;
    for (let x = 0; x < w; x += step) {
      const ta = (x / w) * dur;
      const tb = ((x + step) / w) * dur;
      const a = Math.floor((ta - props.track.offset) * PEAK_RATE);
      const b = Math.ceil((tb - props.track.offset) * PEAK_RATE);
      let peak = 0;
      for (let i = Math.max(0, a); i < Math.min(peaks.length, b); i++) peak = Math.max(peak, peaks[i]);
      if (peak <= 0.002) continue;
      // Perceptual scale: quiet speech stays visible next to loud peaks.
      const amp = Math.min(1, Math.sqrt(Math.min(1, peak * scale)));
      const isLive = heard((ta + tb) / 2);
      if (isLive !== lastLive) {
        ctx.fillStyle = isLive ? live : faint;
        lastLive = isLive;
      }
      const bh = Math.max(bar, amp * (h - 4 * dpr));
      ctx.fillRect(x, mid - bh / 2, bar, bh);
    }
  };

  onMount(() => {
    const ro = new ResizeObserver(() => {
      widthPx = canvas.clientWidth;
      draw();
    });
    ro.observe(canvas);
    onCleanup(() => ro.disconnect());
  });

  createEffect(() => {
    // Track every input the drawing depends on.
    void [props.data, props.track, props.duration, props.trim, props.cuts, props.speed, props.theme];
    draw();
  });

  return <canvas class="tl-wave" ref={canvas} />;
}
