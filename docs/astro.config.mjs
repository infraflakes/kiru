// @ts-check

import { fileURLToPath } from 'node:url';
import starlight from '@astrojs/starlight';
import svelte from '@astrojs/svelte';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'astro/config';

// The docs project lives in `docs/`, but the minimal example and the grammar
// are imported raw from `assets/`, so the dev server must be allowed to read
// the repository root as well.
const repoRoot = fileURLToPath(new URL('..', import.meta.url));

export default defineConfig({
  site: 'https://kiru.infraflakes.fyi',
  // The dev and preview servers accept requests from every interface, and the
  // tailnet hostnames are explicitly allowed past Vite's host check so the
  // site can be opened from a phone on the same network.
  server: {
    host: true,
    allowedHosts: ['serein', 'serein.saury-forel.ts.net'],
  },
  integrations: [
    starlight({
      title: 'kiru',
      description:
        'Kiru is statically validated monotyped DSL designed for process orchestration for multiple repositories, featuring an integrated CLI (kiru) that validates, compiles, and executes locally.',
      customCss: ['./src/styles/global.css'],
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/infraflakes/kiru',
        },
      ],
      sidebar: [
        { label: 'Introduction', link: '/' },
        {
          label: '1. Getting Started',
          items: [
            { label: '1.1 Installation', slug: 'getting-started/installation' },
            { label: '1.2 Hello, World!', slug: 'getting-started/hello-world' },
            { label: '1.3 CLI Basics', slug: 'getting-started/cli-basics' },
          ],
        },
        {
          label: '2. Program Structure',
          items: [
            { label: '2.1 Files and Imports', slug: 'program-structure/files-and-imports' },
            {
              label: '2.2 Declaration Order and Namespaces',
              slug: 'program-structure/declaration-order',
            },
          ],
        },
        {
          label: '3. Values and Templates',
          items: [
            { label: '3.1 Text and Whitespace', slug: 'values/text-and-whitespace' },
            { label: '3.2 Command Substitution', slug: 'values/command-substitution' },
            { label: '3.3 References', slug: 'values/references' },
          ],
        },
        {
          label: '4. Variables',
          items: [
            { label: '4.1 Declaring Variables', slug: 'variables/declaring-variables' },
            { label: '4.2 Command Values and Reuse', slug: 'variables/command-values' },
          ],
        },
        {
          label: '5. Functions',
          items: [
            { label: '5.1 Declaring and Calling', slug: 'functions/declaring-and-calling' },
            { label: '5.2 Parameters', slug: 'functions/parameters' },
            { label: '5.3 Scope and Validation', slug: 'functions/scope-and-validation' },
          ],
        },
        {
          label: '6. Runs',
          items: [
            { label: '6.1 Entry Points and Sequencing', slug: 'runs/entry-points' },
            { label: '6.2 Projects and Context', slug: 'runs/projects' },
            { label: '6.3 Concurrency', slug: 'runs/concurrency' },
            { label: '6.4 Failure and Cancellation', slug: 'runs/failure-and-cancellation' },
          ],
        },
        {
          label: '7. Primitives',
          items: [
            { label: '7.1 Running Commands', slug: 'primitives/running-commands' },
            { label: '7.2 Environment Blocks', slug: 'primitives/environment-blocks' },
            { label: '7.3 Switch and Case', slug: 'primitives/switch' },
          ],
        },
        {
          label: '8. Configuration',
          items: [
            { label: '8.1 Profiles', slug: 'configuration/profiles' },
            { label: '8.2 Projects and Direnv', slug: 'configuration/projects' },
            { label: '8.3 Syncing', slug: 'configuration/syncing' },
          ],
        },
        { label: '9. CLI', slug: 'cli' },
        { label: '10. Architecture', slug: 'architecture' },
        { label: '11. Grammar', slug: 'grammar' },
      ],
      tableOfContents: { minHeadingLevel: 2, maxHeadingLevel: 3 },
      // kiru's fences highlight as shell until a dedicated grammar exists.
      expressiveCode: { shiki: { langAlias: { kiru: 'sh' } } },
    }),
    svelte(),
  ],
  vite: {
    plugins: [tailwindcss()],
    server: { fs: { allow: [repoRoot] } },
  },
});
