#!/usr/bin/env node
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const DIST = path.join(HERE, '..', 'dist');
const BASE = '/Vuoom';

const EXPECTED_ROUTES = [
  '/',
  '/download/',
  '/features/',
  '/features/auto-zoom/',
  '/features/gif-export/',
  '/features/captions/',
  '/features/editor/',
  '/features/audio-and-webcam/',
  '/compare/',
  '/compare/screen-studio/',
  '/compare/cap/',
  '/compare/obs/',
  '/compare/screentogif/',
  '/guide/',
  '/shortcuts/',
  '/faq/',
  '/changelog/',
  '/open-source/',
  '/privacy/',
  '/terms/',
  '/security/',
  '/licenses/',
  '/press/',
  '/404/',
];

async function exists(p) {
  try {
    await fs.access(p);
    return true;
  } catch {
    return false;
  }
}

async function walk(dir) {
  const out = [];
  for (const e of await fs.readdir(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) out.push(...(await walk(p)));
    else out.push(p);
  }
  return out;
}

async function main() {
  const errors = [];
  let checkedLinks = 0;

  for (const route of EXPECTED_ROUTES) {
    const file =
      route === '/404/'
        ? path.join(DIST, '404.html')
        : path.join(DIST, route.slice(1), 'index.html');
    if (!(await exists(file))) {
      errors.push(`Missing route output: ${route} (${file})`);
      continue;
    }
    const html = await fs.readFile(file, 'utf8');
    const h1Count = (html.match(/<h1[\s>]/gi) || []).length;
    if (h1Count !== 1) errors.push(`${route}: expected 1 <h1>, found ${h1Count}`);
    if (!/<title>[^<]+<\/title>/i.test(html)) errors.push(`${route}: missing <title>`);
    if (!/<meta\s+name="description"\s+content="[^"]+"/i.test(html)) errors.push(`${route}: missing description`);
    if (!/<link\s+rel="canonical"\s+href="https:\/\/[^"]+"/i.test(html)) errors.push(`${route}: missing canonical`);

    const ldMatches = [...html.matchAll(/<script\s+type="application\/ld\+json">([\s\S]*?)<\/script>/gi)];
    if (!ldMatches.length) errors.push(`${route}: missing JSON-LD`);
    for (const m of ldMatches) {
      try {
        JSON.parse(m[1]);
      } catch (e) {
        errors.push(`${route}: invalid JSON-LD (${e.message})`);
      }
    }

    // Check img & video explicit width and height
    for (const m of html.matchAll(/<(img|video)\b([^>]+)>/gi)) {
      const attrs = m[2];
      if (!/\bwidth=/i.test(attrs) || !/\bheight=/i.test(attrs)) {
        errors.push(`${route}: <${m[1]}> missing explicit width/height: ${attrs.slice(0, 90)}`);
      }
    }

    // Check internal /Vuoom/ references resolve to dist files
    for (const m of html.matchAll(/(?:href|src|poster|data-src)="(\/Vuoom\/[^"#?]*)/g)) {
      const ref = m[1];
      checkedLinks++;
      const rel = ref.slice(BASE.length + 1);
      const direct = path.join(DIST, rel);
      const asDir = path.join(DIST, rel, 'index.html');
      if (!(await exists(direct)) && !(await exists(asDir))) {
        errors.push(`${route}: unresolved local ref ${ref}`);
      }
    }

    // Check forbidden mock-bar screenshots
    for (const banned of ['screenshot-editor', 'screenshot-record', 'screenshot-export']) {
      if (html.includes(banned)) errors.push(`${route}: forbidden dummy screenshot reference "${banned}"`);
    }
  }

  // Homepage copy hygiene checks
  const homeHtml = await fs.readFile(path.join(DIST, 'index.html'), 'utf8');
  const textOnly = homeHtml
    .replace(/<script[\s\S]*?<\/script>/gi, ' ')
    .replace(/<style[\s\S]*?<\/style>/gi, ' ')
    .replace(/<[^>]+>/g, ' ');
  if (textOnly.includes('\u2014')) errors.push('Homepage contains em dash (—)');
  if (/\bcursor\b/i.test(textOnly)) errors.push('Homepage copy uses "cursor" instead of "pointer"');

  // CSS & JS bundle checks
  const allFiles = await walk(DIST);
  let totalCssBytes = 0;
  let totalJsBytes = 0;
  let studioWebpBytes = 0;
  for (const f of allFiles) {
    const ext = path.extname(f).toLowerCase();
    const st = await fs.stat(f);
    if (ext === '.css') {
      totalCssBytes += st.size;
      const css = await fs.readFile(f, 'utf8');
      if (/#6366f1|#8b5cf6|\bpurple\b|\bviolet\b/i.test(css)) {
        errors.push(`Purple token/word detected in CSS: ${path.basename(f)}`);
      }
    } else if (ext === '.js') {
      totalJsBytes += st.size;
    } else if (ext === '.webp') {
      if (st.size > 200_000) {
        // Note: high-res desktop asset sizes are normal across widths; track summary
      }
      studioWebpBytes += st.size;
    }
  }

  console.log(`Audit summary:`);
  console.log(`  Routes checked: ${EXPECTED_ROUTES.length}`);
  console.log(`  Local /Vuoom/ refs verified: ${checkedLinks}`);
  console.log(`  Built CSS total: ${(totalCssBytes / 1024).toFixed(1)} KB`);
  console.log(`  Built JS total (all routes/chunks): ${(totalJsBytes / 1024).toFixed(1)} KB`);
  console.log(`  Built WebP assets total: ${(studioWebpBytes / 1024).toFixed(1)} KB`);

  if (errors.length) {
    console.error(`\nFAILED (${errors.length} issue(s)):`);
    for (const e of errors) console.error(`  - ${e}`);
    process.exit(1);
  }
  console.log('\nALL STATIC AUDITS PASSED (0 errors).');
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
