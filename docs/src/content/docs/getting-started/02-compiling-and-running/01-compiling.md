---
title: The Compile Command
description: The compiler command, its output path, and the checks it runs.
---

The compiler has three commands:

```console
$ kc <path/to/entry> [-o <output>]
$ kc help
$ kc version
```

`kc <path>` checks the entry file and everything it imports, then writes a
standalone executable at the entry path with the `.kiru` suffix removed:

```console
$ kc tools/release.kiru
$ ./tools/release
```

With `-o`, the output path is chosen explicitly:

```console
$ kc main.kiru -o app
$ ./app
```

A failure prints a positioned diagnostic and exits `1`; [compile-time
checks](/language/09-entry-and-the-cli/03-compile-time-checks/) lists what the
compiler proves.

The output is self-contained: it holds the compiler and the program, so it can
be copied, renamed, and shipped alone. There is no separate linking step and no
interpreter to install on the target machine.
