// Comparison data. Claims about other products are limited to what their own sites and
// repositories state.

export type Cell = { v: 'yes' | 'no' | 'part' | 'text'; t: string };

const y = (t = 'Yes'): Cell => ({ v: 'yes', t });
const n = (t = 'No'): Cell => ({ v: 'no', t });
const p = (t: string): Cell => ({ v: 'part', t });
const x = (t: string): Cell => ({ v: 'text', t });

export const ROWS = [
  'Platforms',
  'Price',
  'Open source',
  'Auto-zoom into clicks',
  'Smooth redrawn pointer',
  'GIF export',
  'Captions',
  'Account needed',
] as const;

export interface Product {
  id: string;
  name: string;
  url: string;
  cells: Cell[];
}

export const PRODUCTS: Product[] = [
  {
    id: 'vuoom',
    name: 'Vuoom',
    url: '/',
    cells: [y('Windows 10/11 & macOS 14+'), x('Free'), y('Apache-2.0'), y(), y(), y(), y('Offline, on device'), n()],
  },
  {
    id: 'screen-studio',
    name: 'Screen Studio',
    url: 'https://screen.studio',
    cells: [p('macOS only'), x('Paid'), n(), y(), y(), y(), y('On device'), x('Paid licence')],
  },
  {
    id: 'cap',
    name: 'Cap',
    url: 'https://cap.so',
    cells: [y('Mac, Windows, Linux'), x('Free, paid Pro'), y('AGPL-3.0'), y(), y(), p('Check their site'), y('In the editor'), p('For share links')],
  },
  {
    id: 'obs',
    name: 'OBS Studio',
    url: 'https://obsproject.com',
    cells: [y('Windows, Mac, Linux'), x('Free'), y('GPL-2.0'), p('Plugins only'), n(), n('Record, then convert'), n(), n()],
  },
  {
    id: 'screentogif',
    name: 'ScreenToGif',
    url: 'https://www.screentogif.com',
    cells: [p('Windows only'), x('Free'), y('MS-PL'), n(), n(), y(), n(), n()],
  },
  {
    id: 'focusee',
    name: 'FocuSee',
    url: 'https://focusee.imobie.com',
    cells: [y('Windows and Mac'), x('Paid'), n(), y(), y(), y(), p('Check their site'), x('Paid licence')],
  },
];

export interface ComparePage {
  slug: string;
  path: '/compare/screen-studio/' | '/compare/cap/' | '/compare/obs/' | '/compare/screentogif/';
  them: string;
  h1: string;
  verdict: string;
  chooseThem: string[];
  chooseUs: string[];
  notes: { h: string; p: string }[];
  faq: { q: string; a: string }[];
}

export const COMPARE_PAGES: ComparePage[] = [
  {
    slug: 'screen-studio',
    path: '/compare/screen-studio/',
    them: 'screen-studio',
    h1: 'Free Screen Studio alternative for Windows and Mac.',
    verdict:
      'Screen Studio set the bar for recordings that zoom into clicks on macOS. Vuoom brings that smooth spring camera to both Windows and Apple Silicon Macs, completely free and open source.',
    chooseThem: [
      'You want to record an iPhone or iPad screen over a cable.',
      'You want a polished commercial app with paid support.',
    ],
    chooseUs: [
      'You record on Windows 10/11 or macOS 14+ Apple Silicon.',
      'You want it free, with no account and no subscription.',
      'You want the source code and a permissive Apache-2.0 licence.',
      'You ship small GIFs for READMEs, changelogs and chat.',
    ],
    notes: [
      {
        h: 'Native performance on both platforms',
        p: 'Vuoom captures with Windows Graphics Capture on PC and ScreenCaptureKit on Mac, composites zooms on the GPU with wgpu, and encodes MP4 using hardware acceleration.',
      },
      {
        h: 'Zooms you can see and edit',
        p: 'Every zoom, automatic or manual, is a block on the timeline. Drag it, stretch it, change its strength or its feel. Nothing is a black box.',
      },
      {
        h: 'Moving from Screen Studio',
        p: 'The workflow feels immediately familiar: record, let the camera glide into clicks, trim, export. Press ? any time inside Vuoom for shortcuts.',
      },
    ],
    faq: [
      {
        q: 'Is there a Screen Studio for Windows?',
        a: 'Screen Studio itself only runs on macOS. Vuoom is a free, open-source app built around the same smooth spring camera idea, native to both Windows 10/11 and macOS 14+.',
      },
      {
        q: 'Is Vuoom as good as Screen Studio?',
        a: 'Screen Studio is a mature commercial app with extras such as iPhone capture. Vuoom covers the core demo studio for free: auto-zoom, smooth pointer, click effects, webcam, narration cleanup, offline captions and GIF/MP4 export.',
      },
    ],
  },
  {
    slug: 'cap',
    path: '/compare/cap/',
    them: 'cap',
    h1: 'Vuoom vs Cap.',
    verdict:
      'Both are open-source screen recorders with auto-zoom. Cap focuses on hosted share links like Loom. Vuoom focuses on local files you paste straight into docs, GitHub and chat.',
    chooseThem: [
      'You need Linux support today.',
      'You want instant cloud share links and a hosted video library.',
      'Your team wants a hosted Pro workspace.',
    ],
    chooseUs: [
      'You want a tiny native app with zero account and zero cloud uploads.',
      'You prefer a permissive Apache-2.0 licence rather than AGPL.',
      'You want offline captions and RNNoise voice clean-up built in.',
      'You ship small GIFs and MP4s as standalone files.',
    ],
    notes: [
      {
        h: 'Licence',
        p: 'Cap uses AGPL-3.0 for its app. Vuoom uses Apache-2.0, which allows reuse in open or commercial workflows cleanly.',
      },
      {
        h: 'Cloud links versus standalone files',
        p: 'Vuoom never uploads your screen. It gives you an estimated file size, fits GIFs under limits like 10 MB, and copies the file to your clipboard.',
      },
    ],
    faq: [
      {
        q: 'Are Cap and Vuoom related?',
        a: 'No. They are independent projects. Both use Rust and Tauri, and Vuoom is written independently from scratch.',
      },
    ],
  },
  {
    slug: 'obs',
    path: '/compare/obs/',
    them: 'obs',
    h1: 'OBS is for streams. Vuoom is for demos.',
    verdict:
      'OBS Studio is unmatched for live streaming and multi-scene broadcasts. For a focused 30-second product demo with click zooms and a clean GIF or MP4 export, Vuoom takes seconds.',
    chooseThem: [
      'You stream live to Twitch or YouTube.',
      'You build complex multi-source scenes and audio routing.',
      'You record hour-long sessions and cut them in Premiere or DaVinci Resolve.',
    ],
    chooseUs: [
      'You want smooth spring zooms that follow your clicks automatically.',
      'You want to trim, skim idle gaps, annotate and export in one app.',
      'You want a crisp GIF or compact MP4 right away.',
    ],
    notes: [
      {
        h: 'Use both',
        p: 'Many creators keep OBS for streams and open Vuoom whenever a feature needs a fast, readable demo.',
      },
    ],
    faq: [
      {
        q: 'Can OBS zoom into the pointer?',
        a: 'Not natively out of the box. Vuoom plans zooms around your clicks automatically, renders motion blur on pans, and lets you adjust every zoom block on a timeline.',
      },
    ],
  },
  {
    slug: 'screentogif',
    path: '/compare/screentogif/',
    them: 'screentogif',
    h1: 'ScreenToGif, plus a camera.',
    verdict:
      'ScreenToGif is a classic free Windows utility with a frame-by-frame editor. Vuoom is also free and open source, runs on both Windows and Mac, and adds auto-zoom and MP4.',
    chooseThem: [
      'You need frame-by-frame pixel editing of an existing GIF.',
      'You rely on its sketch board or multiple legacy GIF encoders.',
    ],
    chooseUs: [
      'You want your recording to zoom into clicks automatically.',
      'You want narration cleanup, offline captions, a webcam bubble and MP4 export.',
      'You record on macOS as well as Windows.',
    ],
    notes: [
      {
        h: 'Small GIFs, on purpose',
        p: 'Vuoom shows an honest size estimate before export and fits GIFs under limits such as GitHub 10 MB automatically.',
      },
    ],
    faq: [
      {
        q: 'What is the best free GIF screen recorder for Windows?',
        a: 'ScreenToGif is great for manual frame editing. If you want automatic camera zooms, smooth pointers, captions and MP4 too, try Vuoom.',
      },
    ],
  },
];
