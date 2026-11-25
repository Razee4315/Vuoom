// Tactile Layered Cut-Paper & Sculpted Digital Graphics manifest for Vuoom (gpt-6.2-astra).
// Screen-recording concepts ONLY: pointer, zoom viewfinder, timeline tracks, voice waveforms, clip export cards, and clean wallpapers.

export const MASTER_STYLE =
  'Modern tactile 3D layered cut-paper and matte sculptural graphic illustration for a clean desktop software studio. ' +
  'Crisp precision-cut heavyweight matte cardstock and subtle frosted-acrylic layers with gentle physical thickness, clean soft editorial studio shadows, smooth geometric curves, and silky gradient depth. ' +
  'Soft overhead directional studio light. ' +
  'Palette: deep obsidian charcoal (#11100F), warm archival cream/ivory (#F5F1E8), Vuoom crimson red (#E5484D), warm coral-amber (#F29E6E), butter gold (#F2D57E), and soft sage-mint (#8AC7A7). ' +
  'Strictly NO vintage cameras, NO camera lenses, NO antique equipment, NO chemical bottles, NO microphones, NO film canisters, NO purple, violet, indigo, neon cyan, text, letters, numbers, words, logos, watermarks, or messy photoreal clutter.';

export const TRANSPARENT_RULE =
  'Isolated single clean dimensional graphic icon/object centered with generous 15% breathing margin on a genuinely transparent background (real alpha channel), ' +
  'no backdrop, no floor, no surface, no cast drop shadow touching the canvas edges, crisp razor-clean edges.';

export const MANIFEST = [
  {
    id: 'hero-bg-dark',
    shape: 'wide panoramic 21:9 landscape',
    transparent: false,
    editOf: null,
    targetWidth: 1920,
    slot: 'hero-bg-dark',
    alt: 'Layered dark obsidian cut-paper waves with a crisp crimson and warm coral edge rim.',
    prompt:
      'Wide panoramic 21:9 abstract layered cut-paper graphic background. Clean matte obsidian charcoal paper sheets stacked in smooth sweeping topographic curves across the right third of the frame, ' +
      'with crisp subtle stepped paper-edge shadows and a thin luminous Vuoom crimson-red and warm coral rim light catching one graceful fold. ' +
      'Keep the left 55% smooth, dark, quiet matte charcoal (#11100F) with zero clutter so white headline typography stays crystal clear.',
  },
  {
    id: 'hero-bg-light',
    shape: 'wide panoramic 21:9 landscape',
    transparent: false,
    editOf: 'hero-bg-dark',
    targetWidth: 1920,
    slot: 'hero-bg-light',
    alt: 'Layered warm ivory cut-paper waves with soft daylight shadows and a coral contour edge.',
    prompt:
      'Same sweeping layered cut-paper curve geometry, layer contours, perspective, and framing as the attached reference image. ' +
      'Transform the dark charcoal sheets into bright warm archival ivory and cream matte cardstock (#F5F1E8) with soft crisp daylight shadows between paper layers and a delicate warm coral-crimson edge along the right fold. ' +
      'Keep the left 55% clean, bright, smooth cream paper for dark ink typography.',
  },
  {
    id: 'graphic-pointer',
    shape: 'square 1:1',
    transparent: true,
    editOf: null,
    targetWidth: 560,
    slot: 'hero-fg-pointer',
    alt: 'Layered paper-cut desktop pointer arrow hovering over concentric crimson click-ripple rings.',
    prompt:
      'Square 1:1 tactile dimensional cut-paper graphic. A sleek, crisp modern desktop pointer arrow crafted from thick angled ivory-white matte cardstock with subtle dark charcoal layered bevel thickness, ' +
      'hovering slightly angled above two floating concentric crimson-red (#E5484D) and soft coral click-ripple rings. Pure clean graphic object, zero text.',
  },
  {
    id: 'graphic-zoom-rest',
    shape: 'square 1:1',
    transparent: true,
    editOf: null,
    targetWidth: 560,
    slot: 'hero-fg-zoom-rest',
    alt: 'Tactile layered paper-cut screen viewfinder crop brackets framing a frosted focus dot.',
    prompt:
      'Square 1:1 tactile dimensional cut-paper graphic. A clean modern screen-capture viewfinder frame made of four rounded matte dark charcoal and warm ivory corner crop brackets arranged cleanly in a balanced square, ' +
      'framing a translucent frosted-acrylic circular focus disc with a small crisp crimson-red (#E5484D) target dot in the center. Pure modern digital UI graphic, zero camera body, zero text.',
  },
  {
    id: 'graphic-zoom-active',
    shape: 'square 1:1',
    transparent: true,
    editOf: 'graphic-zoom-rest',
    targetWidth: 560,
    slot: 'hero-fg-zoom-active',
    alt: 'Tightened screen viewfinder crop brackets glowing with an active crimson focus ring.',
    prompt:
      'Same camera angle, square framing, materials, and clean transparent alpha background as the attached reference image. ' +
      'Show the active zoomed state: tighten the four rounded corner crop brackets slightly inward toward the center and illuminate the central crimson-red (#E5484D) focus dot into a glowing concentric crimson and warm coral target ring. ' +
      'Keep the alpha edges clean with no backdrop or floor.',
  },
  {
    id: 'wallpaper-crimson',
    shape: 'wide 16:9 landscape',
    transparent: false,
    editOf: null,
    targetWidth: 1400,
    slot: 'studio-backdrop-crimson',
    alt: 'Minimal abstract obsidian and crimson cut-paper wave desktop wallpaper.',
    prompt:
      'Wide 16:9 pure minimal abstract desktop wallpaper. Smooth sculptural layered matte obsidian charcoal and warm Vuoom crimson-coral cut-paper ribbons sweeping softly from corner to corner with velvety gradient light. ' +
      'Completely empty, flat-surface abstract wallpaper with zero physical objects, zero props, zero desks, and zero text.',
  },
  {
    id: 'wallpaper-paper',
    shape: 'wide 16:9 landscape',
    transparent: false,
    editOf: null,
    targetWidth: 1400,
    slot: 'studio-backdrop-paper',
    alt: 'Minimal abstract warm cream, coral, and butter-gold cut-paper wave desktop wallpaper.',
    prompt:
      'Wide 16:9 pure minimal abstract desktop wallpaper. Sweeping layered matte archival cream, soft peach-coral, warm butter-gold, and warm sand cut-paper arcs with gentle paper-edge shadows. ' +
      'Completely empty, uncluttered abstract background wallpaper with zero physical objects, zero props, and zero text.',
  },
  {
    id: 'wallpaper-sage',
    shape: 'wide 16:9 landscape',
    transparent: false,
    editOf: null,
    targetWidth: 1400,
    slot: 'studio-backdrop-sage',
    alt: 'Minimal abstract deep pine-slate and soft sage-mint cut-paper wave desktop wallpaper.',
    prompt:
      'Wide 16:9 pure minimal abstract desktop wallpaper. Smooth deep pine-slate charcoal and soft muted sage-mint layered cut-paper waves bathed in calm warm studio rim light. ' +
      'Completely empty, uncluttered abstract background wallpaper with zero physical objects, zero props, zero purple, and zero text.',
  },
  {
    id: 'graphic-timeline',
    shape: 'landscape 3:2',
    transparent: true,
    editOf: null,
    targetWidth: 680,
    slot: 'finisher-graphic-timeline',
    alt: 'Dimensional paper-cut timeline tracks in crimson, butter gold, and sage sliced by a vertical playhead.',
    prompt:
      'Landscape 3:2 tactile dimensional cut-paper graphic floating at a gentle clean isometric tilt. Three stacked horizontal matte cardstock timeline track lanes built from rounded pill-shaped blocks in Vuoom crimson red (#E5484D), warm butter gold (#F2D57E), and soft sage mint (#8AC7A7) resting on a slim dark charcoal base tray, ' +
      'cleanly crossed by a tall vertical crimson playhead needle with a crisp top diamond cap. Pure modern graphic, strictly zero text or numbers.',
  },
  {
    id: 'graphic-soundwave',
    shape: 'square 1:1',
    transparent: true,
    editOf: null,
    targetWidth: 560,
    slot: 'audio-graphic-wave',
    alt: 'Layered paper-cut voice equalizer bars beside clean blank rounded speech caption cards.',
    prompt:
      'Square 1:1 tactile dimensional cut-paper graphic. Seven crisp rounded vertical matte-cardstock audio equalizer bars rising in a balanced symmetric rhythm, colored in a smooth gradient from Vuoom crimson coral to soft sage mint, ' +
      'paired beside two clean overlapping blank ivory rounded speech-caption pill cards showing subtle layered paper thickness. Pure clean graphic, strictly zero text or letters.',
  },
  {
    id: 'graphic-export',
    shape: 'landscape 3:2',
    transparent: true,
    editOf: null,
    targetWidth: 640,
    slot: 'export-graphic-cards',
    alt: 'Dimensional paper-cut media clip cards with a sculpted crimson upward export arrow.',
    prompt:
      'Landscape 3:2 tactile dimensional cut-paper graphic. Two overlapping clean rounded-square matte cardstock media cards tilted gently: one dark charcoal card featuring a crisp embossed ivory play triangle emblem, and one warm butter-gold card featuring a clean circular loop ribbon icon, ' +
      'with a bold sculpted crimson-red upward share/export arrow rising dynamically between them. Strictly zero text, zero letters, zero words, zero "GIF" or "MP4" labels.',
  },
  {
    id: 'closing-bg-dark',
    shape: 'wide panoramic 21:9 landscape',
    transparent: false,
    editOf: null,
    targetWidth: 1920,
    slot: 'closing-bg-dark',
    alt: 'Minimal dark obsidian layered cut-paper ribbons with subtle crimson and warm cream rim light.',
    prompt:
      'Wide panoramic 21:9 abstract layered cut-paper background. Clean sweeping matte obsidian charcoal paper curves stacked harmoniously along the right side with subtle crimson-red and warm butter-cream rim highlights along the fold edges. ' +
      'Keep the left 60% calm, uncluttered matte charcoal obsidian (#11100F) so foreground text reads effortlessly.',
  },
  {
    id: 'closing-bg-light',
    shape: 'wide panoramic 21:9 landscape',
    transparent: false,
    editOf: 'closing-bg-dark',
    targetWidth: 1920,
    slot: 'closing-bg-light',
    alt: 'Warm ivory layered cut-paper ribbons with subtle coral and sage highlights.',
    prompt:
      'Same sweeping cut-paper ribbon geometry and framing as the attached reference image. Transform the dark obsidian sheets into warm archival ivory and soft stone cardstock (#F5F1E8) with subtle coral-crimson and soft sage rim accents along the right fold, ' +
      'leaving the left 60% clean and bright for dark typography.',
  },
];

export function buildPrompt(item) {
  const parts = [
    'IMPORTANT: Do NOT run any shell commands. Do NOT read, inspect, or copy files with shell tools. ONLY call your built-in image generation tool (image_gen) to generate this single image, then reply with the saved file path.',
    '',
    item.prompt,
    item.transparent ? TRANSPARENT_RULE : '',
    `Target framing & aspect: ${item.shape}.`,
    MASTER_STYLE,
  ];
  return parts.filter(Boolean).join('\n\n');
}
