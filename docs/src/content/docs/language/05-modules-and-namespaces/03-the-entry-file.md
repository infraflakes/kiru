---
title: The Entry File
description: What makes a file the entry, and what happens to its main.
---

The entry is the file given to `kc`; the compiled binary runs that file's
`main`. It must be in the root namespace, and it cannot declare a `module`;
keeping it in the root namespace is what makes its `main` the entry. [The
entry function](/language/09-entry-and-the-cli/01-main/) covers `main`, the
signatures it accepts, and what happens to its return value.
