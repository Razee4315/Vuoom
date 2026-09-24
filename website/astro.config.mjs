// @ts-check
import { defineConfig } from 'astro/config';
import sitemap from '@astrojs/sitemap';

// The two values to change for a custom domain: `site` becomes the domain and `base`
// becomes '/'. Then add public/CNAME with the domain. Everything else follows.
export default defineConfig({
  site: 'https://razee4315.github.io',
  base: '/Vuoom',
  trailingSlash: 'always',
  output: 'static',
  build: { format: 'directory', inlineStylesheets: 'auto' },
  prefetch: { prefetchAll: false, defaultStrategy: 'hover' },
  integrations: [
    sitemap({
      filter: (page) => !page.includes('/404'),
      changefreq: 'weekly',
      lastmod: new Date(),
    }),
  ],
});
