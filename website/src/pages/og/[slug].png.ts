// One 1200×630 social card per page, rendered at build time with satori + resvg.
import type { APIRoute, GetStaticPaths } from 'astro';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import satori from 'satori';
import { Resvg } from '@resvg/resvg-js';
import sharp from 'sharp';
import { PAGES, ogSlug } from '../../data/pages';

const require = createRequire(import.meta.url);
const font = (p: string) => readFile(require.resolve(p));

let assets: Promise<{ display: Buffer; text: Buffer; shot: string }> | undefined;
function load() {
  assets ??= (async () => {
    const [display, text, shotBuf] = await Promise.all([
      font('@fontsource/bricolage-grotesque/files/bricolage-grotesque-latin-600-normal.woff'),
      font('@fontsource/geist/files/geist-latin-400-normal.woff'),
      sharp(join(process.cwd(), 'src/assets/shots/take-zoom.png'))
        .resize(900)
        .jpeg({ quality: 82 })
        .toBuffer(),
    ]);
    return { display, text, shot: `data:image/jpeg;base64,${shotBuf.toString('base64')}` };
  })();
  return assets;
}

export const getStaticPaths: GetStaticPaths = () =>
  Object.entries(PAGES).map(([path, meta]) => ({
    params: { slug: ogSlug(path) },
    props: { title: meta.og, kicker: meta.kicker },
  }));

type El = { type: string; props: Record<string, unknown> };
const h = (type: string, style: Record<string, unknown>, children?: unknown, extra: Record<string, unknown> = {}): El => ({
  type,
  props: { style, children, ...extra },
});

export const GET: APIRoute = async ({ props }) => {
  const { title, kicker } = props as { title: string; kicker: string };
  const { display, text, shot } = await load();
  const size = title.length > 34 ? 58 : 68;

  const tree = h('div', { width: 1200, height: 630, display: 'flex', background: '#f5f1e8', position: 'relative', fontFamily: 'Geist' }, [
    h('div', { position: 'absolute', left: 72, top: 64, display: 'flex', alignItems: 'center', gap: 14 }, [
      h('svg', { width: 44, height: 44 }, [
        h('path', {}, undefined, { d: 'M305 338 512 722 700 373', fill: 'none', stroke: '#242720', 'stroke-width': 104, 'stroke-linecap': 'round', 'stroke-linejoin': 'round' }),
        h('circle', {}, undefined, { cx: 719, cy: 338, r: 100, fill: '#c8412d' }),
      ], { viewBox: '0 0 1024 1024' }),
      h('div', { fontFamily: 'Bricolage', fontSize: 34, color: '#242720', letterSpacing: -0.6 }, 'Vuoom'),
    ]),
    h('div', { position: 'absolute', left: 72, top: 150, fontSize: 22, color: '#c8412d' }, kicker),
    h('div', { position: 'absolute', left: 72, top: 190, width: 600, fontFamily: 'Bricolage', fontSize: size, lineHeight: 1, letterSpacing: -2.2, color: '#242720' }, title),
    h('div', { position: 'absolute', left: 72, bottom: 64, fontSize: 24, color: '#575a50' }, 'Free, open-source screen recorder for Windows'),
    h('img', { position: 'absolute', left: 720, top: 120, width: 760, height: 428, borderRadius: 18, border: '1px solid rgba(242,240,235,0.12)', objectFit: 'cover' }, undefined, { src: shot, width: 760, height: 428 }),
  ]);

  const svg = await satori(tree as never, {
    width: 1200,
    height: 630,
    fonts: [
      { name: 'Bricolage', data: display, weight: 600, style: 'normal' },
      { name: 'Geist', data: text, weight: 400, style: 'normal' },
    ],
  });
  const png = new Resvg(svg, { fitTo: { mode: 'width', value: 1200 } }).render().asPng();
  return new Response(new Uint8Array(png), { headers: { 'Content-Type': 'image/png' } });
};
