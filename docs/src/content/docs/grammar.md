---
title: Grammar
description: The formal EBNF grammar for kiru files.
---

import { Code } from 'astro:components';
import grammar from '../../../../assets/kiru.ebnf?raw';

This is the formal grammar behind the language taught from [Program
Structure](../program-structure/files-and-imports/) onward, kept as a single
source in
[`assets/kiru.ebnf`](https://github.com/infraflakes/kiru/blob/main/assets/kiru.ebnf).

<Code code={grammar} lang="text" title="kiru.ebnf" />
