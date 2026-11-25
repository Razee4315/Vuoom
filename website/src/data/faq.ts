// Questions shared between the home page, the FAQ page and feature pages.
// Plain text so they serialize cleanly into FAQPage JSON-LD. Zero em dashes.

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
    q: 'Is Vuoom a Screen Studio alternative for Windows and Mac?',
    a: 'Yes. Screen Studio is paid and macOS only. Vuoom brings smooth auto-zoom, a redrawn pointer, timeline editing, offline captions and GIF/MP4 export to both Windows 10/11 and macOS 14+ Apple Silicon, free and open source.',
  },
  {
    q: 'Why does Windows SmartScreen or macOS Gatekeeper warn?',
    a: 'Builds are not code-signed with paid developer certificates yet. On Windows click More info, then Run anyway. On macOS right-click the app and choose Open the first time. Every installer is built in public by GitHub Actions from source.',
  },
  {
    q: 'Does Vuoom upload my recordings?',
    a: 'No. Recording, editing, voice clean-up, captions and export all run on your computer. Vuoom has no account, no cloud and no telemetry.',
  },
  {
    q: 'Which platforms are supported?',
    a: 'Windows 10 (1809+) and Windows 11 (64-bit) using Windows Graphics Capture and Media Foundation, plus macOS 14+ on Apple Silicon using ScreenCaptureKit, Metal and AVFoundation.',
  },
  {
    q: 'What can I export?',
    a: 'An optimised GIF or an H.264 MP4 up to 4K at 60 fps with an AAC soundtrack, plus an .srt caption file. A live size estimate and a fit-under-N-MB helper keep files small enough for GitHub and Slack.',
  },
];

export const FAQ_MORE: QA[] = [
  {
    q: 'How big is the download?',
    a: 'The Windows setup .exe is under 9 MB (.msi is about 11 MB), and the Apple Silicon .dmg is around 11 MB. Vuoom is built with Rust and Tauri, not Electron, so it does not bundle a second browser.',
  },
  {
    q: 'How do I zoom while recording?',
    a: 'Press Ctrl+Shift+Z on Windows (Shift+Command+Z on macOS) to glide the camera into your pointer, and press it again to pull back out. You can also let Auto zooms plan zooms from your clicks after stopping.',
  },
  {
    q: 'Can I edit the zooms afterwards?',
    a: 'Yes. Every zoom is a block on the timeline. Move it, resize it, change its strength, aim it at the pointer or lock it to a spot, and pick a feel: Smooth, Snappy or Slow.',
  },
  {
    q: 'How do captions work without the internet?',
    a: 'Vuoom uses whisper.cpp on your own computer. The first time you ask for captions it downloads a 57 MB speech model once from Hugging Face; after that everything runs offline.',
  },
  {
    q: 'How do updates work?',
    a: 'On launch Vuoom checks GitHub Releases for a newer signed update manifest. When one exists, an Update button appears in the top bar; nothing installs until you click it.',
  },
  {
    q: 'What happens if Vuoom crashes mid-recording?',
    a: 'Frames stream directly to disk as compressed LZ4 deltas while you record, so the take survives. Next time you open Vuoom, the home screen offers to recover it.',
  },
  {
    q: 'Can I record a single window or a part of the screen?',
    a: 'Yes. Pick a region (free or 16:9, 9:16, 1:1 or 4:5), a whole display, or a single app window, on any monitor, and pause or resume any time.',
  },
  {
    q: 'Can I contribute?',
    a: 'Please do. Bug reports, ideas and pull requests are welcome on GitHub. The frontend also ships with a browser mock engine so you can work on the UI without building Rust.',
  },
];
