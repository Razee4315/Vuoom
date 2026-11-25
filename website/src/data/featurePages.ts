// Content for the five feature pages. Uses real footage & genuine product visuals only.

import type { QA } from './faq';

export interface FeaturePage {
  slug: 'auto-zoom' | 'gif-export' | 'captions' | 'editor' | 'audio-and-webcam';
  label: string;
  title: string;
  sub: string;
  media: { stage?: boolean; video?: 'demo' | 'github' | 'voice'; shot?: 'home' | 'flat' | 'zoom' };
  steps: string[];
  details: { h: string; p: string }[];
  keys?: { k: string[]; d: string }[];
  faq: QA[];
  related: FeaturePage['slug'][];
}

export const FEATURE_PAGES: FeaturePage[] = [
  {
    slug: 'auto-zoom',
    label: 'Auto-zoom',
    title: 'Auto-zoom that follows your clicks.',
    sub: 'The camera glides to what you click, then back out. Free on Windows and macOS.',
    media: { stage: true, video: 'github' },
    steps: [
      'Press Ctrl+Shift+Z (or Shift+Command+Z on Mac) while you record. The camera glides to your pointer.',
      'Press it again to pull back out.',
      'Or skip the shortcut and let Auto zooms plan every zoom from your clicks.',
    ],
    details: [
      { h: 'Never a hard cut', p: 'Zooms move on a critically damped spring, so they settle without bounce or overshoot.' },
      { h: 'Aim and feel', p: 'Each zoom can follow the pointer or lock to a spot, and feel Smooth, Snappy or Slow.' },
      { h: 'Motion blur', p: 'A touch of blur on every zoom and pan keeps a 30 fps GIF looking smooth.' },
      { h: 'Smooth pointer and click effects', p: 'Record without the real pointer and Vuoom redraws one with Ripple, Rings, Pulse or Burst feedback.' },
    ],
    keys: [
      { k: ['Ctrl', 'Shift', 'Z'], d: 'Zoom in or out at the pointer while recording' },
      { k: ['Z'], d: 'Add a zoom at the playhead in the editor' },
    ],
    faq: [
      { q: 'Does it zoom on every mouse move?', a: 'No. Zooms follow clicks and your shortcut, and hold while you keep working in one area, so the video stays calm.' },
      { q: 'Can I change a zoom after recording?', a: 'Yes. Every zoom is a block on the timeline you can move, stretch, re-aim or delete. Auto zooms can re-plan all of them at a new strength.' },
      { q: 'Is there a free auto-zoom screen recorder for Windows and Mac?', a: 'Vuoom is one: free, open source under Apache-2.0, native to Windows 10/11 and macOS 14+.' },
    ],
    related: ['editor', 'gif-export'],
  },
  {
    slug: 'gif-export',
    label: 'GIF and MP4 export',
    title: 'GIF or MP4, small on purpose.',
    sub: 'See the file size before you export. Paste it into GitHub, Slack or a changelog.',
    media: { video: 'demo' },
    steps: ['Press Ctrl+E (or Command+E on Mac).', 'Pick GIF or MP4 and a preset.', 'Press Copy and paste the file anywhere.'],
    details: [
      { h: 'A live size estimate', p: 'The export card shows the file size as you change width, frame rate or length.' },
      { h: 'Fit under N MB', p: 'Tell Vuoom the limit, such as 10 MB for GitHub, and it fits the GIF under it automatically.' },
      { h: 'MP4 on your GPU', p: 'H.264 up to 4K 60 fps with an AAC soundtrack, encoded by NVENC, Quick Sync, AMF or Apple VideoToolbox.' },
      { h: 'Captions included', p: 'Burn captions into the video, or save an .srt file timed to your export.' },
    ],
    keys: [{ k: ['Ctrl', 'E'], d: 'Open export' }],
    faq: [
      { q: 'How do I record a GIF for a GitHub README?', a: 'Record with Vuoom, trim, export as GIF with the fit-under-10-MB helper, then press Copy and paste into the README editor on GitHub.' },
      { q: 'Is there a watermark?', a: 'Never. Vuoom is free and open source.' },
      { q: 'Which is smaller, GIF or MP4?', a: 'MP4 is usually far smaller for the same length. GIF plays everywhere without a player, which is why READMEs and chats still use it.' },
    ],
    related: ['auto-zoom', 'editor'],
  },
  {
    slug: 'captions',
    label: 'Offline captions',
    title: 'Captions, made on your computer.',
    sub: 'Your narration becomes captions without uploading a thing.',
    media: { video: 'voice' },
    steps: ['Record with your microphone on.', 'Click Make captions. The speech model downloads once, 57 MB.', 'Fix any word, then export.'],
    details: [
      { h: 'Nothing leaves your computer', p: 'Speech is recognised by whisper.cpp on your own processor. After the first download it works offline.' },
      { h: 'Edit like text', p: 'Fix a word in the inspector. Drag a caption bar on the timeline to retime it.' },
      { h: 'Top or bottom, any size', p: 'Captions show in the preview and in every GIF and MP4.' },
      { h: '.srt for YouTube', p: 'Save a subtitle file timed to your export for players that support it.' },
    ],
    faq: [
      { q: 'Are the captions processed in the cloud?', a: 'No. They run via whisper.cpp locally on your machine. The model file downloads once from Hugging Face; your audio never leaves your device.' },
      { q: 'Which languages work?', a: 'The model is multilingual, supporting common spoken languages alongside English and Simplified Chinese.' },
      { q: 'Can I turn captions off after making them?', a: 'Yes. Hide them, or export an .srt file instead of burning them in.' },
    ],
    related: ['audio-and-webcam', 'gif-export'],
  },
  {
    slug: 'editor',
    label: 'Editor',
    title: 'Edit in seconds.',
    sub: 'Trim, cut, speed up and draw on the video. One timeline, no bloated suite.',
    media: { shot: 'zoom' },
    steps: ['Trim the ends with I and O.', 'Cut the fumbles with C, skim the waiting with X.', 'Add text, curved arrows, numbered steps or a spotlight, then export.'],
    details: [
      { h: 'Skim idle', p: 'Dead stretches play at 2 to 8 times speed, so waiting for a page never makes it into the video.' },
      { h: 'Annotations', p: 'Text, curved arrows, numbered steps, lines, boxes, ellipses, a pen and marker, highlights, spotlight, and blur masks.' },
      { h: 'Visual crop & frames', p: 'Drag a crop right on the video with aspect locks, or frame the recording in Subtle or Studio backdrops.' },
      { h: 'Undo everything', p: 'Every edit is undoable, and projects save in seconds as .vuoom files.' },
    ],
    keys: [
      { k: ['I'], d: 'Trim start at the playhead' },
      { k: ['O'], d: 'Trim end at the playhead' },
      { k: ['C'], d: 'Insert a cut' },
      { k: ['X'], d: 'Insert a speed-up' },
      { k: ['T'], d: 'Text tool' },
      { k: ['N'], d: 'Numbered step tool' },
      { k: ['L'], d: 'Spotlight' },
    ],
    faq: [
      { q: 'Can I blur passwords or emails?', a: 'Yes. Add a mask with the Hide tool (M) in solid black or pixelated style. It tracks the target even during zooms.' },
      { q: 'Does editing lower the quality?', a: 'No. Edits are instructions on top of the original frames; the video is rendered once when you export.' },
      { q: 'Can I save and reopen a project?', a: 'Yes. Ctrl+S saves a .vuoom project that reopens from the recents grid.' },
    ],
    related: ['auto-zoom', 'captions'],
  },
  {
    slug: 'audio-and-webcam',
    label: 'Audio and webcam',
    title: 'Your voice, your face, your screen.',
    sub: 'Narration, system sound and a webcam bubble, recorded together and kept in sync.',
    media: { shot: 'home' },
    steps: ['Turn on the microphone, system sound or webcam in the record bar.', 'Watch the level meters while you frame the shot.', 'Record. Everything lines up on the timeline.'],
    details: [
      { h: 'Two audio tracks', p: 'Microphone and computer sound each get a track with volume and mute. They stay in sync through cuts and speed-ups.' },
      { h: 'Studio-clean voice', p: 'One switch removes fan noise, hum and hiss. Another evens out a voice that drifts from the mic.' },
      { h: 'A webcam bubble', p: 'Circle, rounded square or 16:9, in any corner, any size, mirrored or not. Change it after recording.' },
      { h: 'Original always kept', p: 'Clean-up works on a copy, so the raw recording is one click away.' },
    ],
    faq: [
      { q: 'Can I record system audio?', a: 'Yes. Vuoom records system audio on its own track alongside your microphone through WASAPI on Windows and ScreenCaptureKit on macOS.' },
      { q: 'Does noise removal need the internet?', a: 'No. It uses RNNoise locally on your computer.' },
      { q: 'Can I move the webcam after recording?', a: 'Yes. Position, size, shape and mirroring are all editable afterwards.' },
    ],
    related: ['captions', 'editor'],
  },
];
