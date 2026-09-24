// Synthetic audio for the browser mock: a voice-like narration track and a system track
// with a few UI chimes, encoded as the same 16-bit PCM WAV the engine serves. The voice
// has a room-noise bed and phrases at uneven volume, so the clean-up options visibly change
// its waveform (the mock imitates their effect; the engine runs the real processing).
import type { AudioKind, AudioTrack } from "../types";

const RATE = 16_000;

function wav(samples: Float32Array, rate: number): ArrayBuffer {
  const buf = new ArrayBuffer(44 + samples.length * 2);
  const v = new DataView(buf);
  const str = (o: number, s: string) => {
    for (let i = 0; i < s.length; i++) v.setUint8(o + i, s.charCodeAt(i));
  };
  str(0, "RIFF");
  v.setUint32(4, 36 + samples.length * 2, true);
  str(8, "WAVE");
  str(12, "fmt ");
  v.setUint32(16, 16, true);
  v.setUint16(20, 1, true);
  v.setUint16(22, 1, true);
  v.setUint32(24, rate, true);
  v.setUint32(28, rate * 2, true);
  v.setUint16(32, 2, true);
  v.setUint16(34, 16, true);
  str(36, "data");
  v.setUint32(40, samples.length * 2, true);
  for (let i = 0; i < samples.length; i++) {
    v.setInt16(44 + i * 2, Math.round(Math.max(-1, Math.min(1, samples[i])) * 32767), true);
  }
  return buf;
}

/** Deterministic noise so screenshots are stable. */
function noise(seed: number) {
  let s = seed;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 2 ** 31 - 1;
  };
}

/** Phrases of syllables with pauses between them: reads as speech on a waveform. */
function voice(duration: number, clean: { denoise: boolean; level: boolean }): Float32Array {
  const out = new Float32Array(Math.ceil(duration * RATE));
  const rnd = noise(7);
  if (!clean.denoise) {
    // A fan and a little hiss under everything.
    const hiss = noise(11);
    for (let i = 0; i < out.length; i++) {
      out[i] = 0.018 * Math.sin((2 * Math.PI * 110 * i) / RATE) + 0.02 * hiss();
    }
  }
  const phrases: [number, number][] = [];
  for (let t = 0.4; t < duration - 0.5; ) {
    const len = 1.2 + ((phrases.length * 0.77) % 1.6);
    phrases.push([t, Math.min(duration - 0.2, t + len)]);
    t += len + 0.5 + ((phrases.length * 0.31) % 0.6);
  }
  for (const [n, [a, b]] of phrases.entries()) {
    // Every third phrase trails off, the next one is too close to the mic.
    const loudness = clean.level ? 1 : [1, 0.35, 1.6][n % 3];
    for (let i = Math.floor(a * RATE); i < Math.floor(b * RATE); i++) {
      const t = i / RATE;
      const syl = Math.max(0, Math.sin(2 * Math.PI * 4.3 * (t - a))) ** 1.5;
      const edge = Math.min(1, (t - a) / 0.08, (b - t) / 0.12);
      const f0 = 140 + 18 * Math.sin(2 * Math.PI * 0.7 * t);
      const tone =
        0.5 * Math.sin(2 * Math.PI * f0 * t) +
        0.25 * Math.sin(2 * Math.PI * 2 * f0 * t) +
        0.12 * Math.sin(2 * Math.PI * 3 * f0 * t);
      out[i] += 0.32 * loudness * syl * edge * (tone + 0.35 * rnd());
    }
  }
  return out;
}

/** Soft two-note chimes, like UI sounds from the recorded app. */
function chimes(duration: number): Float32Array {
  const out = new Float32Array(Math.ceil(duration * RATE));
  for (const at of [1.1, 2.4, 6.8, 10.2]) {
    if (at >= duration) continue;
    for (const [dt, f] of [
      [0, 880],
      [0.09, 1320],
    ]) {
      const s0 = Math.floor((at + dt) * RATE);
      for (let k = 0; k < RATE * 0.35 && s0 + k < out.length; k++) {
        const t = k / RATE;
        out[s0 + k] += 0.22 * Math.exp(-t * 11) * Math.sin(2 * Math.PI * f * t);
      }
    }
  }
  return out;
}

export function mockTrackWav(track: AudioTrack | undefined, kind: AudioKind, duration: number): ArrayBuffer {
  const clean = { denoise: !!track?.denoise, level: !!track?.level };
  return wav(kind === "mic" ? voice(duration, clean) : chimes(duration), RATE);
}

/** A plausible live level for the meters (speech-like bursts). */
export function mockLevel(kind: AudioKind): number {
  const t = performance.now() / 1000;
  if (kind === "system") return Math.random() < 0.04 ? 0.5 : 0.02;
  const syl = Math.max(0, Math.sin(2 * Math.PI * 4.3 * t)) * (Math.sin(2 * Math.PI * 0.35 * t) > -0.3 ? 1 : 0.05);
  return Math.min(1, 0.05 + 0.7 * syl * (0.7 + 0.3 * Math.random()));
}
