---
title: Imports
description: Loading other files, path rules, load-once, and cycles.
---

`import` loads another file at the position of the import:

<span class="filename">Filename: main.kiru</span>

```kiru
import "tasks.kiru";
```

The path is relative to the importing file, or absolute. The path is a
fixed string literal; it is never expanded, so an import cannot use a
run-time value such as `$HOME`.

An import makes the loaded file's namespace reachable with `::`; it does
not introduce unqualified names into the importing file. After
`import "tasks.kiru"` with a `module tasks;` in that file, a call is written
`tasks::clean()`, and `clean()` alone is unknown:

<span class="filename">Filename: main.kiru</span>

```kiru
import "tasks.kiru";

fn main() {
  tasks::clean();
};
```

A file may reach its own namespace, the implicit `std` tree, and the
namespaces of the files it imports directly. Importing a file that imports
another does not make the second namespace reachable. Here `main.kiru`
imports `a.kiru`, which imports `tasks.kiru`; `main.kiru` still cannot name
the `tasks` namespace:

```console
$ kc main.kiru
main.kiru:4:3: error: namespace `tasks` is not imported
  tasks::clean();
  ^^^^^^^^^^^^
```

## Load Once

A path loads once per program. Importing the same file from two places
loads it the first time and reuses the result, so a file can be imported
freely without duplicating declarations. Files that declare the same
namespace merge into it, and two declarations of one name across those
files are duplicate-name compile errors; [unique
names](/language/03-names-and-scope/02-unique-names/) explains the rule.

A missing file and an import cycle are load errors:

```console
$ kc main.kiru
main.kiru:1:1: error: cannot find import gone.kiru
import "gone.kiru";
^^^^^^^^^^^^^^^^^^^
```

A cycle is reported at the import that closes the loop. Because imports are
depth-first, the error names the file that imported back into the load.

## Imports Come First

Imports must appear before declarations in a file. A declaration followed
by an import is a parse error:

```console
$ kc main.kiru
main.kiru:2:1: error: imports must come before declarations
import "gone.kiru";
^^^^^^
```

The order matches the load order: a file's imports are processed before its
declarations, so imported names are visible everywhere in the importing
file.
