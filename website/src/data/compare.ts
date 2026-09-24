// Comparison data. Claims about other products are limited to what their own sites and
// repositories state, checked September 2026. Anything uncertain says so rather than guessing.

export type Cell = { v: 'yes' | 'no' | 'part' | 'text'; t: string };

const y = (t = 'Yes'): Cell => ({ v: 'yes', t });
const n = (t = 'No'): Cell => ({ v: 'no', t });
const p = (t: string): Cell => ({ v: 'part', t });
const x = (t: string): Cell => ({ v: 'text', t });

export const ROWS = [
  'Runs on Windows',
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
    cells: [y('Windows 10 and 11'), x('Free'), y('Apache-2.0'), y(), y(), y(), y('Offline, on device'), n()],
  },
  {
    id: 'screen-studio',
    name: 'Screen Studio',
    url: 'https://screen.studio',
    cells: [n('macOS only'), x('Paid'), n(), y(), y(), y(), y('On device'), x('Paid licence')],
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
    cells: [y('Windows'), x('Free'), y('MS-PL'), n(), n(), y(), n(), n()],
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
    h1: 'The Screen Studio alternative for Windows.',
    verdict:
      'Screen Studio set the standard for recordings that zoom into your clicks, and it only runs on a Mac. Vuoom brings that idea to Windows, free and open source.',
    chooseThem: [
      'You record on a Mac.',
      'You want to record an iPhone or iPad over a cable.',
      'You want a polished commercial product with paid support.',
    ],
    chooseUs: [
      'You record on Windows 10 or 11.',
      'You want it free, with no account and no subscription.',
      'You want the source code and a permissive licence.',
      'You mostly ship small GIFs for READMEs, changelogs and chat.',
    ],
    notes: [
      {
        h: 'The same idea, built natively for Windows',
        p: 'Vuoom captures with Windows Graphics Capture, renders zooms on the GPU with wgpu and encodes MP4 with Media Foundation, using NVENC, Quick Sync or AMF when your graphics card has them.',
      },
      {
        h: 'Zooms you can see and edit',
        p: 'Every zoom, automatic or manual, is a block on the timeline. Drag it, stretch it, change its strength or its feel. Nothing is a black box.',
      },
      {
        h: 'Moving from Screen Studio',
        p: 'The workflow will feel familiar: record, let the zooms happen, trim, export. Keyboard shortcuts differ; the cheat sheet is one keypress away with ?.',
      },
    ],
    faq: [
      {
        q: 'Is there a Screen Studio for Windows?',
        a: 'Screen Studio itself only runs on macOS. Vuoom is a free, open-source Windows app built around the same idea: a camera that zooms into your clicks.',
      },
      {
        q: 'Is Vuoom as good as Screen Studio?',
        a: 'Screen Studio is a mature commercial app with features Vuoom does not have, such as iPhone recording. Vuoom covers the core of it on Windows: auto-zoom, a smooth pointer, webcam, narration, captions and GIF or MP4 export, for free.',
      },
    ],
  },
  {
    slug: 'cap',
    path: '/compare/cap/',
    them: 'cap',
    h1: 'Vuoom vs Cap.',
    verdict:
      'Both are open-source screen recorders with auto-zoom. Cap is cross-platform and built around sharing links. Vuoom is Windows-only and built around small files you paste anywhere.',
    chooseThem: [
      'You need Mac, Windows and Linux with one tool.',
      'You want instant share links and a hosted library, like Loom.',
      'Your team wants a paid plan with cloud storage.',
    ],
    chooseUs: [
      'You are on Windows and want a native app with no account at all.',
      'You want a permissive licence (Apache-2.0) rather than AGPL.',
      'You want captions and voice clean-up that run entirely on your PC.',
      'You ship GIFs and MP4s as files rather than links.',
    ],
    notes: [
      {
        h: 'Licence',
        p: 'Cap\'s app is AGPL-3.0. Vuoom is Apache-2.0, which lets anyone reuse the code in their own projects, open or closed.',
      },
      {
        h: 'Sharing versus files',
        p: 'Cap leans on uploading and sharing a link. Vuoom never uploads anything; it hands you a file and a Copy button.',
      },
    ],
    faq: [
      {
        q: 'Are Cap and Vuoom related?',
        a: 'No. They are separate projects. Both use Rust and Tauri, and Vuoom\'s code is written independently.',
      },
    ],
  },
  {
    slug: 'obs',
    path: '/compare/obs/',
    them: 'obs',
    h1: 'OBS is for streams. Vuoom is for demos.',
    verdict:
      'OBS Studio is the best free tool for streaming and long recordings with scenes. For a 30-second product demo that zooms into clicks and ends up as a GIF, Vuoom gets you there without an editor.',
    chooseThem: [
      'You stream to Twitch or YouTube.',
      'You need scenes, sources, overlays and plugins.',
      'You record long sessions and edit them elsewhere.',
    ],
    chooseUs: [
      'You want zooms that follow your clicks without keyframing them by hand.',
      'You want to trim, cut, annotate and export in the same app.',
      'You want a GIF or small MP4 at the end, not a large MKV.',
    ],
    notes: [
      {
        h: 'Use both',
        p: 'Plenty of people keep OBS for streams and reach for Vuoom when they need a quick demo. They do not conflict.',
      },
    ],
    faq: [
      {
        q: 'Can OBS zoom into the mouse?',
        a: 'Not on its own. There are community plugins and scripts that follow the mouse. Vuoom does it natively, plans zooms from your clicks and lets you edit them on a timeline.',
      },
    ],
  },
  {
    slug: 'screentogif',
    path: '/compare/screentogif/',
    them: 'screentogif',
    h1: 'ScreenToGif, plus a camera.',
    verdict:
      'ScreenToGif is a much-loved free Windows tool with a frame-by-frame editor. Vuoom is also free and open source on Windows, and adds a camera that zooms into your clicks.',
    chooseThem: [
      'You want to edit a GIF frame by frame.',
      'You need its board and webcam recorders or its many GIF encoders.',
    ],
    chooseUs: [
      'You want the recording to zoom into what you click, automatically.',
      'You want narration, captions, a webcam bubble or MP4 as well as GIF.',
      'You prefer a timeline editor with cuts, speed-ups and annotations.',
    ],
    notes: [
      {
        h: 'Small GIFs, on purpose',
        p: 'Vuoom shows the file size before you export and can fit a GIF under a size you choose, which matters for GitHub\'s 10 MB limit.',
      },
    ],
    faq: [
      {
        q: 'What is the best free GIF screen recorder for Windows?',
        a: 'ScreenToGif is excellent for frame-level GIF editing. If you want the camera to zoom into your clicks automatically, and captions or MP4 too, try Vuoom. Both are free and open source.',
      },
    ],
  },
];
