// JSON-LD builders. Base.astro adds Organization, WebSite and BreadcrumbList on every page.
import { SITE, absolute } from './site';
import { latest } from './github';
import type { QA } from '../data/faq';

export async function softwareApp() {
  const r = await latest();
  return {
    '@type': 'SoftwareApplication',
    '@id': `${absolute('/')}#app`,
    name: 'Vuoom',
    alternateName: 'Vuoom screen recorder',
    description: SITE.description,
    applicationCategory: 'MultimediaApplication',
    applicationSubCategory: 'Screen recorder',
    operatingSystem: 'Windows 10, Windows 11, macOS 14+',
    softwareVersion: r.version,
    datePublished: r.date || undefined,
    downloadUrl: r.exe?.url ?? SITE.latestUrl,
    installUrl: absolute('/download/'),
    fileSize: r.exe ? `${Math.round(r.exe.size / 1024)} KB` : undefined,
    license: 'https://www.apache.org/licenses/LICENSE-2.0',
    isAccessibleForFree: true,
    url: absolute('/'),
    image: absolute('/og/home.png'),
    screenshot: absolute('/media/demo-poster.jpg'),
    codeRepository: SITE.repoUrl,
    author: { '@type': 'Person', name: SITE.author, url: SITE.authorUrl },
    publisher: { '@id': `${absolute('/')}#org` },
    offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
    featureList: [
      'Automatic zoom into clicks',
      'Smooth redrawn pointer and click effects',
      'Microphone and system audio recording',
      'Webcam overlay bubble',
      'RNNoise voice cleanup and loudness levelling',
      'Offline captions with whisper.cpp',
      'Annotations: text, curved arrows, numbered steps, pen, spotlight, blur',
      'GIF and GPU H.264 MP4 export',
      'Crash recovery',
    ],
  };
}

export function faqPage(items: QA[]) {
  return {
    '@type': 'FAQPage',
    mainEntity: items.map((it) => ({
      '@type': 'Question',
      name: it.q,
      acceptedAnswer: { '@type': 'Answer', text: it.a },
    })),
  };
}

export function howTo(name: string, steps: { name: string; text: string }[]) {
  return {
    '@type': 'HowTo',
    name,
    totalTime: 'PT2M',
    tool: [{ '@type': 'HowToTool', name: 'Vuoom' }],
    step: steps.map((s, i) => ({ '@type': 'HowToStep', position: i + 1, name: s.name, text: s.text })),
  };
}
