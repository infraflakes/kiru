---
title: Program Structure
description: Namespaces, imports, and top-level values.
---

As a program grows, you group its code into namespaces and load it with
imports. In this section we'll see how a file declares its namespace, how to
import another file, and when top-level values are computed.

## Namespaces

Every file belongs to a namespace, a path of identifiers that keeps names
apart. A file that opens with `module` belongs to that path:

<span class="filename">Filename: build.kiru</span>

```kiru
module tasks::build;
```

`module` must be the first thing in the file and may appear once. Files that
declare the same path share one namespace, so a namespace can span files. A
file with no `module` belongs to the root namespace, which is where the entry
file lives.

Paths nest: `tasks::build` is inside `tasks`. An unqualified name is looked up
in the current namespace, then in each parent; a closer declaration wins.

`std` is reserved and always visible. A file cannot declare anything under it.

## Imports

`import "path";` loads another file. The path is relative to the importing file
or absolute, and is always a fixed string; it is never expanded, so an import
cannot use a runtime value such as `$HOME`. Imports must come before any
declaration.

An import makes the loaded file's namespace reachable by name. It adds no bare
names:

<span class="filename">Filename: main.kiru</span>

```kiru
import "tasks.kiru";

fn main() {
  tasks::clean();     # the namespace is named
};
```

A file can name its own namespace, `std`, and the namespaces of the files it
imports directly. Importing a file that imports another does not make the
second namespace reachable.

A path loads once per program, so a file can be imported freely. A missing file
and an import cycle are errors.

## Top-level values

A `txt`, `rec`, or `list` declared at the top level of a file is a top-level
value. Top-level values are computed once, when the program starts, in
declaration order, before `main`:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt home = "/home/dev";

rec backend = {
  name = "backend",
  dir = home + "/projects/backend",
};
```

A later reference reads the stored value; the initializer does not run again. A
top-level value can call a function declared above it, and cannot be assigned.
[Names and Scope](/concepts/names-and-scope/) covers the declare-before-use
rule.
