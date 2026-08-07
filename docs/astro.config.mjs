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
            { label: 'Playground Functions', slug: 'reference/playground-functions' },
          ],
        },
        {
          label: 'Standard Library',
          items: [
            { label: 'Overview', slug: 'reference/standard-library' },
            { label: 'io', slug: 'reference/std/io' },
            { label: 'conv', slug: 'reference/std/conv' },
            { label: 'str', slug: 'reference/std/str' },
            { label: 'math', slug: 'reference/std/math' },
            { label: 'time', slug: 'reference/std/time' },
            { label: 'iter', slug: 'reference/std/iter' },
            { label: 'algorithm', slug: 'reference/std/algorithm' },
            { label: 'collections', slug: 'reference/std/collections' },
            { label: 'fs', slug: 'reference/std/fs' },
            { label: 'env', slug: 'reference/std/env' },
            { label: 'proc', slug: 'reference/std/proc' },
            { label: 'net', slug: 'reference/std/net' },
            { label: 'thread', slug: 'reference/std/thread' },
            { label: 'term', slug: 'reference/std/term' },
          ],
        },
      ],
    }),
  ],
});
