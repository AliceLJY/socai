import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://socai.io',
  output: 'static',
  vite: {
    server: {
      proxy: {
        '/api/server': {
          target: 'https://agent-api.socai.io',
          changeOrigin: true,
          rewrite: (path) => path.replace(/^\/api\/server/, ''),
        },
      },
    },
  },
  integrations: [
    starlight({
      title: 'socai docs',
      description:
        'Install socai, connect your signed-in Chrome, and run source-linked social media research.',
      favicon: '/favicon.svg',
      logo: {
        src: './src/assets/socai-mark.svg',
        alt: '',
      },
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/socai-io/socai',
        },
        {
          icon: 'discord',
          label: 'Discord',
          href: 'https://discord.gg/CpQdA7bwt8',
        },
      ],
      editLink: {
        baseUrl: 'https://github.com/socai-io/socai/edit/main/site/',
      },
      customCss: ['./src/styles/docs.css'],
      lastUpdated: true,
      disable404Route: true,
      sidebar: [
        {
          label: 'Start here',
          items: [
            { slug: 'docs' },
            { slug: 'docs/quickstart' },
            { slug: 'docs/installation' },
            { slug: 'docs/browser-connection' },
          ],
        },
        {
          label: 'Use socai',
          items: [
            { slug: 'docs/agent-workflows' },
            { slug: 'docs/cli' },
            { slug: 'docs/platforms' },
            { slug: 'docs/evidence' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { slug: 'docs/troubleshooting' },
            { slug: 'docs/development' },
          ],
        },
      ],
    }),
  ],
  markdown: {
    // The site is monochrome/light; Shiki's default github-dark theme fights
    // that and hard-codes a dark background on code blocks via inline styles.
    // Disable it so plain <pre><code> picks up our own .prose styling.
    syntaxHighlight: false,
  },
});
