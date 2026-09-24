// The feature index. Every claim here ships in v2.0.0 (README "Features").
// `page` links to the dedicated SEO page where one exists.

export interface Feature {
  n: string;
  title: string;
  line: string;
  detail: string;
  page?: string;
  shot?: 'editor' | 'home' | 'record' | 'export' | 'editor-light';
  video?: 'demo' | 'github' | 'voice';
}

export const FEATURES: Feature[] = [
  {
    n: '01',
    title: 'Auto-zoom on click',
    line: 'The camera glides into the action, never a hard cut.',
    detail:
      'Press Ctrl+Shift+Z while recording, or let Auto zooms plan every zoom from your clicks at the strength you pick. Each zoom has an aim and a feel, Smooth, Snappy or Slow, with a touch of motion blur.',
    page: '/features/auto-zoom/',
    video: 'github',
  },
  {
    n: '02',
    title: 'A smooth pointer',
    line: 'Record without the real cursor and get a clean one back.',
    detail:
      'Vuoom redraws the pointer from your movements: jitter smoothed out, a press on every click, sized to stay legible in a small GIF, and fading away while it rests.',
    page: '/features/auto-zoom/',
    shot: 'editor',
  },
  {
    n: '03',
    title: 'Narration and system sound',
    line: 'Microphone, computer audio, or both, each on its own track.',
    detail:
      'Live level meters while you frame the shot, waveforms on the timeline, volume and mute per track, and audio that stays in sync through every cut and speed-up.',
    page: '/features/audio-and-webcam/',
    shot: 'record',
  },
  {
    n: '04',
    title: 'You, in the corner',
    line: 'A webcam bubble you can move, resize and reshape later.',
    detail:
      'Circle, rounded square or 16:9, in any corner, mirrored or not. The record HUD previews it live, right where it will sit.',
    page: '/features/audio-and-webcam/',
    shot: 'record',
  },
  {
    n: '05',
    title: 'Studio-clean voice',
    line: 'One switch removes fan noise, hum and hiss.',
    detail:
      'RNNoise cleans the room, a leveller evens out a voice that drifts toward and away from the mic, and peaks stay under clipping. Both work on a copy, so the original is one click away.',
    page: '/features/audio-and-webcam/',
    shot: 'editor',
  },
  {
    n: '06',
    title: 'Captions, made offline',
    line: 'Your narration becomes captions on your own PC.',
    detail:
      'whisper.cpp runs locally; the speech model downloads once and nothing is uploaded. Fix any word, drag to retime, burn them into the video or save an .srt file.',
    page: '/features/captions/',
    video: 'voice',
  },
  {
    n: '07',
    title: 'Draw on the video',
    line: 'Text, arrows, a pen, a marker, a spotlight, and blur.',
    detail:
      'Every annotation gets its own timeline bar, colour, opacity, fades and stacking order. Redaction masks hide what should not be seen.',
    page: '/features/editor/',
    shot: 'editor',
  },
  {
    n: '08',
    title: 'GIF or MP4, small',
    line: 'A live size estimate and a fit-under-N-MB helper.',
    detail:
      'Optimised GIF, or H.264 MP4 up to 60 fps encoded on your GPU when available. One click copies the file, ready to paste into GitHub, Slack or a changelog.',
    page: '/features/gif-export/',
    shot: 'export',
  },
  {
    n: '09',
    title: 'Nothing lost in a crash',
    line: 'Frames stream to disk while you record.',
    detail:
      'Recordings are stored as compressed deltas, typically 20 times smaller than raw pixels, so length is bounded by your drive and the last take can be recovered after a crash.',
    page: '/guide/#projects',
    shot: 'home',
  },
];
