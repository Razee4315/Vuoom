// llms.txt: a plain summary for AI assistants and answer engines (llmstxt.org).
import type { APIRoute } from 'astro';
import { PAGES } from '../data/pages';
import { SITE, absolute } from '../lib/site';
import { FAQ_GENERAL, FAQ_MORE } from '../data/faq';

export const GET: APIRoute = () => {
  const pages = Object.entries(PAGES)
    .filter(([p]) => p !== '/404/')
    .map(([p, m]) => `- [${m.title}](${absolute(p)}): ${m.description}`)
    .join('\n');
  const faq = [...FAQ_GENERAL, ...FAQ_MORE].map((q) => `### ${q.q}\n${q.a}`).join('\n\n');
  const body = `# Vuoom

> ${SITE.description}

Vuoom is a free, open-source (Apache-2.0) screen recorder for Windows 10 and 11, often used as a Screen Studio alternative on Windows. While you record, the camera zooms into each click with smooth spring motion. It records microphone, system sound and a webcam bubble, removes noise, makes captions offline with whisper.cpp, and exports GIF or MP4. No account, no watermark, no cloud. Built with Rust, Tauri and wgpu.

- Download: ${absolute('/download/')}
- Source: ${SITE.repoUrl}
- Licence: Apache-2.0

## Pages

${pages}

## FAQ

${faq}
`;
  return new Response(body, { headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
};
