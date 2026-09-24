// Questions shared between the home page, the FAQ page and feature pages.
// Answers are plain text so they can go into FAQPage JSON-LD unchanged.

export interface QA {
  q: string;
  a: string;
}

export const FAQ_GENERAL: QA[] = [
  {
    q: 'Is Vuoom really free?',
    a: 'Yes. Vuoom is free and open source under the Apache License 2.0. There is no paid tier, no trial, no watermark and no account.',
  },
  {
    q: 'Is Vuoom a Screen Studio alternative for Windows?',
    a: 'That is exactly why it exists. Screen Studio only runs on macOS. Vuoom brings the same idea, a camera that zooms into your clicks, to Windows 10 and 11 as a small native app.',
  },
  {
    q: 'Why does Windows SmartScreen warn when I install it?',
    a: 'The installers are not code-signed yet, and SmartScreen warns about any new unsigned app. Click More info, then Run anyway. The source code and the build workflow that produces every installer are public on GitHub.',
  },
  {
    q: 'Does Vuoom upload my recordings?',
    a: 'No. Recording, editing, voice clean-up, captions and export all run on your computer. Vuoom has no account, no cloud and no telemetry.',
  },
  {
    q: 'Does it work on Mac or Linux?',
    a: 'Not today. Vuoom is built on Windows Graphics Capture and Media Foundation, which is what keeps it small and fast. Windows 10 and 11 are supported.',
  },
  {
    q: 'What can I export?',
    a: 'An optimised GIF or an H.264 MP4 up to 60 fps with an AAC soundtrack, plus an .srt caption file. A live size estimate and a fit-under-N-MB helper keep files small enough for GitHub and Slack.',
  },
];

export const FAQ_MORE: QA[] = [
  {
    q: 'How big is the download?',
    a: 'The setup .exe is under 8 MB and the .msi is about 10 MB. Vuoom is built with Rust and Tauri, not Electron, so it does not ship a copy of a browser.',
  },
  {
    q: 'How do I zoom while recording?',
    a: 'Press Ctrl+Shift+Z to glide the camera into your pointer, and press it again to pull back out. You can also leave it to Auto zooms, which plans zooms from your clicks after you stop.',
  },
  {
    q: 'Can I edit the zooms afterwards?',
    a: 'Yes. Every zoom is a block on the timeline. Move it, resize it, change its strength, aim it at the pointer or lock it to a spot, and pick a feel: Smooth, Snappy or Slow.',
  },
  {
    q: 'How do captions work without the internet?',
    a: 'Vuoom uses whisper.cpp on your own processor. The first time you ask for captions it downloads a 57 MB speech model once from Hugging Face; after that everything runs offline.',
  },
  {
    q: 'How do updates work?',
    a: 'On launch Vuoom asks GitHub Releases whether a newer signed build exists. If one does, an Update button appears; nothing installs until you click it.',
  },
  {
    q: 'What happens if Vuoom crashes mid-recording?',
    a: 'Frames are written to disk as you record, so the take survives. Next time you open Vuoom, the home screen offers to recover it.',
  },
  {
    q: 'Can I record a single window or a part of the screen?',
    a: 'Yes. Pick a region (free or 16:9, 9:16, 1:1 or 4:5), a whole display, or a single app window, on any monitor.',
  },
  {
    q: 'Can I contribute?',
    a: 'Please do. Bug reports, ideas and pull requests are welcome on GitHub. The frontend can even run in a plain browser with a mock engine, so you do not need to build the Rust side to work on the interface.',
  },
];
