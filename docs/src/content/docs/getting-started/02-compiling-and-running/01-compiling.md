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
$ ./app verify
```

## What the Compiler Checks

Compilation performs every check: syntax and escapes, modules and imports,
names and declaration order, call arity, kinds, body and return rules,
and thread rules. The list is catalogued in
[compile-time
checks](/language/09-entry-and-the-cli/03-compile-time-checks/).

A failure prints a positioned diagnostic and exits `1`:

```console
$ kc broken.kiru
broken.kiru:1:13: error: unknown name `nope`
fn main() { nope(); };
            ^^^^
```

If the program passes, the output is ready to run. The binary is
self-contained: it holds the compiler and the program, so it can be copied,
renamed, and shipped alone. There is no separate linking step and no
interpreter to install on the target machine.
