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
          label: '1. Getting Started',
          collapsed: true,
          items: [
            { label: '1.0 Overview', slug: 'getting-started/00-overview' },
            {
              label: '1.1 From a Release',
              slug: 'getting-started/01-installing-kiru/01-from-a-release',
            },
            { label: '1.2 From Source', slug: 'getting-started/01-installing-kiru/02-from-source' },
            {
              label: '1.3 Compiling',
              slug: 'getting-started/02-compiling-and-running/01-compiling',
            },
          ],
        },
        {
          label: '2. Common Concepts',
          collapsed: true,
          items: [
            { label: '2.0 Overview', slug: 'language/02-common-concepts/00-overview' },
            { label: '2.1 Text', slug: 'language/02-common-concepts/01-text' },
            { label: '2.2 Records', slug: 'language/02-common-concepts/02-records' },
            { label: '2.3 Field Access', slug: 'language/02-common-concepts/03-field-access' },
            {
              label: '2.4 Text Concatenation',
              slug: 'language/02-common-concepts/04-text-concatenation',
            },
            { label: '2.5 Functions', slug: 'language/02-common-concepts/05-functions' },
            { label: '2.6 Parameters', slug: 'language/02-common-concepts/06-parameters' },
            {
              label: '2.7 Return and Void',
              slug: 'language/02-common-concepts/07-return-and-recursion',
            },
            { label: '2.8 Calls', slug: 'language/02-common-concepts/08-calls' },
            { label: '2.9 Switch', slug: 'language/02-common-concepts/09-switch' },
          ],
        },
        {
          label: '3. Names, Scope, and Assignment',
          collapsed: true,
          items: [
            { label: '3.0 Overview', slug: 'language/03-names-and-scope/00-overview' },
            {
              label: '3.1 Declaration Order',
              slug: 'language/03-names-and-scope/01-declaration-order',
            },
            { label: '3.2 Unique Names', slug: 'language/03-names-and-scope/02-unique-names' },
            { label: '3.3 Assignment', slug: 'language/03-names-and-scope/03-assignment' },
            {
              label: '3.4 Bodies and Scopes',
              slug: 'language/03-names-and-scope/04-bodies-and-scopes',
            },
            { label: '3.5 Module Values', slug: 'language/03-names-and-scope/05-module-values' },
          ],
        },
        {
          label: '4. Types and Checking',
          collapsed: true,
          items: [
            { label: '4.0 Overview', slug: 'language/04-types/00-overview' },
            { label: '4.1 Types', slug: 'language/04-types/01-types' },
            { label: '4.2 Bindings', slug: 'language/04-types/02-bindings' },
            { label: '4.3 Type Checking', slug: 'language/04-types/03-type-checking' },
          ],
        },
        {
          label: '5. Modules and Namespaces',
          collapsed: true,
          items: [
            { label: '5.0 Overview', slug: 'language/05-modules-and-namespaces/00-overview' },
            { label: '5.1 Modules', slug: 'language/05-modules-and-namespaces/01-modules' },
            { label: '5.2 Imports', slug: 'language/05-modules-and-namespaces/02-imports' },
            {
              label: '5.3 The Entry File',
              slug: 'language/05-modules-and-namespaces/03-the-entry-file',
            },
          ],
        },
        {
          label: '6. Commands',
          collapsed: true,
          items: [
            { label: '6.0 Overview', slug: 'effects/06-commands/00-overview' },
            {
              label: '6.1 Building a Command',
              slug: 'effects/06-commands/01-building-a-command',
            },
            { label: '6.2 Builders', slug: 'effects/06-commands/02-builders' },
            { label: '6.3 Streaming Levels', slug: 'effects/06-commands/03-terminals' },
            {
              label: '6.4 Output and Errors',
              slug: 'effects/06-commands/04-streaming-and-capturing',
            },
            {
              label: '6.5 Exit Codes and Failure',
              slug: 'effects/06-commands/05-exit-codes-and-failure',
            },
            { label: '6.6 Process Groups', slug: 'effects/06-commands/06-process-groups' },
            {
              label: '6.7 Input, Output, and Timeouts',
              slug: 'effects/06-commands/07-input-output-and-timeouts',
            },
          ],
        },
        {
          label: '7. Concurrency',
          collapsed: true,
          items: [
            { label: '7.0 Overview', slug: 'effects/07-threads/00-overview' },
            { label: '7.1 Threads', slug: 'effects/07-threads/01-threads' },
          ],
        },
        {
          label: '8. Failure and Cleanup',
          collapsed: true,
          items: [
            { label: '8.0 Overview', slug: 'effects/08-failure-and-cleanup/00-overview' },
            { label: '8.1 Panic', slug: 'effects/08-failure-and-cleanup/01-panic' },
            { label: '8.2 Signals', slug: 'effects/08-failure-and-cleanup/02-signals' },
            { label: '8.3 Defer', slug: 'effects/08-failure-and-cleanup/03-defer' },
            {
              label: '8.4 Defers During Unwind',
              slug: 'effects/08-failure-and-cleanup/04-defers-during-unwind',
            },
          ],
        },
        {
          label: '9. Entry and the CLI',
          collapsed: true,
          items: [
            { label: '9.0 Overview', slug: 'language/09-entry-and-the-cli/00-overview' },
            { label: '9.1 The Entry Function', slug: 'language/09-entry-and-the-cli/01-main' },
            {
              label: '9.2 The Args Record',
              slug: 'language/09-entry-and-the-cli/02-the-args-record',
            },
            {
              label: '9.3 Compile-Time Checks',
              slug: 'language/09-entry-and-the-cli/03-compile-time-checks',
            },
          ],
        },
        {
          label: '10. The Standard Library',
          collapsed: true,
          items: [
            { label: '10.0 Overview', slug: 'stdlib/00-overview' },
            { label: '10.1 Runtime Builtins', slug: 'stdlib/01-runtime-builtins' },
            { label: '10.2 Written by Kiru', slug: 'stdlib/02-written-by-kiru' },
          ],
        },
        { label: 'Appendix A. Standard Library', slug: 'appendix/a-standard-library' },
        { label: 'Appendix B. Reserved Words', slug: 'appendix/b-reserved-words' },
        { label: 'Appendix C. Grammar', slug: 'appendix/c-grammar' },
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
