// @ts-check

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import starlight from '@astrojs/starlight';
import svelte from '@astrojs/svelte';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'astro/config';

// The site reads only `docs/`, but Vite is allowed to serve from the
// repository root so the dev server can follow symlinked tooling.
const repoRoot = fileURLToPath(new URL('..', import.meta.url));

// The Kiru TextMate grammar is read from disk next to this config so the
// fences highlight as `kiru` regardless of the working directory the build
// starts from.
//
// Astro's content cache keeps the highlighted markup of the pages it has
// already rendered, so a build after editing `kiru.tmLanguage.json` reuses the
// old colors unless the caches are dropped first. Clear `docs/.astro`,
// `docs/node_modules/.astro` and `docs/node_modules/.vite`, or run
// `bun run build --force`, before trusting a grammar change.
const kiruGrammar = JSON.parse(
  readFileSync(fileURLToPath(new URL('./kiru.tmLanguage.json', import.meta.url)), 'utf-8'),
);

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
      title: 'Kiru',
      description: 'A statically typed, compiled process orchestration language.',
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
          label: 'Getting Started',
          collapsed: false,
          items: [
            { label: 'Installation', slug: 'getting-started/installation' },
            { label: 'Hello, World!', slug: 'getting-started/hello-world' },
            { label: 'Compiling and Running', slug: 'getting-started/compiling-and-running' },
          ],
        },
        { label: 'A First Program', slug: 'getting-started/a-first-program' },
        {
          label: 'Common Concepts',
          collapsed: false,
          items: [
            { label: 'Text', slug: 'concepts/text' },
            { label: 'Records', slug: 'concepts/records' },
            { label: 'Types and Checking', slug: 'concepts/types-and-checking' },
            { label: 'Functions', slug: 'concepts/functions' },
            { label: 'Bindings and Mutability', slug: 'concepts/bindings-and-mutability' },
            { label: 'Switch', slug: 'concepts/switch' },
            { label: 'Lists and Loops', slug: 'concepts/lists-and-loops' },
            { label: 'Names and Scope', slug: 'concepts/names-and-scope' },
            { label: 'Namespaces and Imports', slug: 'concepts/namespaces' },
          ],
        },
        {
          label: 'Commands',
          collapsed: false,
          items: [
            { label: 'Running a Command', slug: 'commands/running-a-command' },
            { label: 'The Command Spec', slug: 'commands/the-command-spec' },
            { label: 'Process Groups', slug: 'commands/process-groups' },
          ],
        },
        { label: 'Threads', slug: 'concurrency/threads' },
        {
          label: 'Failure and Signals',
          collapsed: false,
          items: [
            { label: 'Panic', slug: 'failure/panic' },
            { label: 'Signals', slug: 'failure/signals' },
          ],
        },
        {
          label: 'The Standard Library',
          collapsed: false,
          items: [
            { label: 'std::process', slug: 'stdlib/process' },
            { label: 'std::io', slug: 'stdlib/io' },
            { label: 'std::fs', slug: 'stdlib/fs' },
            { label: 'std::text', slug: 'stdlib/text' },
            { label: 'std::lists', slug: 'stdlib/lists' },
            { label: 'std::env', slug: 'stdlib/env' },
            { label: 'std::path', slug: 'stdlib/path' },
            { label: 'std::time', slug: 'stdlib/time' },
          ],
        },
        { label: 'Reserved Words', slug: 'appendix/reserved-words' },
        { label: 'Grammar', slug: 'appendix/grammar' },
      ],
      tableOfContents: { minHeadingLevel: 2, maxHeadingLevel: 3 },
      // The Kiru grammar supplied above teaches Shiki the `kiru` language.
      expressiveCode: { shiki: { langs: [kiruGrammar] } },
    }),
    svelte(),
  ],
  vite: {
    plugins: [tailwindcss()],
    server: { fs: { allow: [repoRoot] } },
  },
});
