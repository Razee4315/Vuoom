// Feature index referencing genuine recordings and real product stills (zero dummy mockups).

export interface Feature {
  n: string;
  title: string;
  line: string;
  detail: string;
  page?: string;
  shot?: 'home' | 'flat' | 'zoom';
  video?: 'demo' | 'github' | 'voice';
}

export const FEATURES: Feature[] = [
  {
    n: '01',
    title: 'Auto-zoom on click',
    line: 'The camera glides into the action, never a hard cut.',
    detail:
      'Press Ctrl+Shift+Z on Windows (Shift+Command+Z on macOS) while recording, or let Auto zooms plan zooms from your clicks. Each zoom has an aim and a feel, Smooth, Snappy or Slow, with subtle motion blur.',
    page: '/features/auto-zoom/',
    video: 'github',
  },
  {
    n: '02',
    title: 'A smooth pointer',
    line: 'Record without the real pointer and get a clean one back.',
    detail:
      'Vuoom redraws the pointer from your movements: jitter smoothed out, a press on every click, click ripples/rings/bursts, sized to stay legible in a small GIF and fading away while it rests.',
    page: '/features/auto-zoom/',
    shot: 'zoom',
  },
  {
    n: '03',
    title: 'Narration and system sound',
    line: 'Microphone, computer audio, or both, each on its own track.',
    detail:
      'Live level meters while you frame the shot, waveforms on the timeline, volume and mute per track, and audio that stays in sync through every cut and speed-up.',
    page: '/features/audio-and-webcam/',
    shot: 'home',
  },
  {
    n: '04',
    title: 'You, in the corner',
    line: 'A webcam bubble you can move, resize and reshape later.',
    detail:
      'Circle, rounded square or 16:9, in any corner, mirrored or not. The record HUD previews it live, right where it will sit.',
    page: '/features/audio-and-webcam/',
    video: 'demo',
  },
  {
    n: '05',
    title: 'Studio-clean voice',
    line: 'One switch removes fan noise, hum and hiss.',
    detail:
      'RNNoise cleans the room, a leveller evens out a voice that drifts toward and away from the mic, and peaks stay under clipping. Both work on a copy, so the original is one click away.',
    page: '/features/audio-and-webcam/',
    video: 'voice',
  },
  {
    n: '06',
    title: 'Captions, made offline',
    line: 'Your narration becomes captions on your own computer.',
    detail:
      'whisper.cpp runs locally; the speech model downloads once and nothing is uploaded. Fix any word, drag to retime, burn them into the video or save an .srt file.',
    page: '/features/captions/',
    video: 'voice',
  },
  {
    n: '07',
    title: 'Draw on the video',
    line: 'Text, curved arrows, numbered steps, a spotlight, and blur.',
    detail:
      'Every annotation gets its own timeline bar, colour, opacity, fades and stacking order. Pixelated or black hide masks keep secrets covered even while zoomed.',
    page: '/features/editor/',
    video: 'demo',
  },
  {
    n: '08',
    title: 'GIF or MP4, small',
    line: 'A live size estimate and a fit-under-N-MB helper.',
    detail:
      'Optimised GIF with gradient dithering, or H.264 MP4 up to 4K 60 fps encoded on your GPU when available. One click copies the file, ready to paste into GitHub, Slack or a changelog.',
    page: '/features/gif-export/',
    shot: 'zoom',
  },
  {
    n: '09',
    title: 'Nothing lost in a crash',
    line: 'Frames stream to disk while you record.',
    detail:
      'Recordings are stored as compressed LZ4 deltas, typically 20 times smaller than raw pixels, so length is bounded by your drive and the last take can be recovered after a crash.',
    page: '/guide/#projects',
    shot: 'home',
  },
];
