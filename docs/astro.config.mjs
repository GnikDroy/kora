// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightThemeFlexoki from 'starlight-theme-flexoki';

const USERNAME = 'gnikdroy';
const REPO = 'kora';

export default defineConfig({
  site: `https://${USERNAME}.github.io`,
  base: `/${REPO}`,
  integrations: [
    starlight({
      title: 'Kora',
      description:
        'A small, statically typed language with generics, inference and garbage collection that compiles to native executables and JavaScript.',
      logo: { src: './src/assets/logo.svg', alt: 'Kora' },
      favicon: '/favicon.svg',
      plugins: [starlightThemeFlexoki()],
      customCss: ['./src/styles/custom.css'],
      social: [{ icon: 'github', label: 'GitHub', href: `https://github.com/${USERNAME}/${REPO}` }],
      sidebar: [
        {
          label: 'Guides',
          items: [
            { label: 'Getting Started', slug: 'guides/getting-started' },
            { label: 'Hello World', slug: 'guides/hello-world' },
            { label: 'Kora in 5 Minutes', slug: 'guides/kora-in-5-minutes' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'Architecture', slug: 'reference/architecture' },
            { label: 'Runtime Helpers', slug: 'reference/runtime-helpers' },
            { label: 'Playground Functions', slug: 'reference/playground-functions' },
          ],
        },
      ],
    }),
  ],
});
