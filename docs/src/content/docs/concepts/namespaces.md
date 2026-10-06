---
title: Program Structure
description: Namespaces, imports, top-level values, and the entry.
---

As a program grows, you group its code into namespaces with `mod` blocks and
load it with imports. In this section we'll see how a file declares a
namespace, how to import another file, and where `main` lives.

## Namespaces

A `mod a::b { ... };` block declares an **inline namespace**. Every declaration
inside belongs to `a::b`:

```kiru
mod tasks::build {
  fn run(line<txt>) { std::command({}, line); };
};
```

`tasks::build::run` is reached by that path. A file may hold several `mod`
blocks, and **reopening a path merges** into the same namespace, so one
namespace can span blocks and files:

```kiru
mod tasks {
  fn clean() { std::print("clean"); };
};

mod tasks {
  fn build() { clean(); };   # `clean` is in the same namespace
};
```

Paths nest: `tasks::build` is inside `tasks`. An unqualified name is looked up
in the current namespace, then in each parent; a closer declaration wins.

`std` is reserved and always visible. A program cannot declare anything under
it.

## Imports

`import "path";` loads another file. The path is relative to the importing file
or absolute, and is always a fixed string; it is never expanded, so an import
cannot use a runtime value such as `$HOME`.

An import is an **item**: the imported file's items are spliced **at that
position**, so the order of the imports is the declaration order. An import is
**global-level only** — it is never written inside a `mod`.

```kiru
import "tasks.kiru";

fn main() {
  tasks::build::run("cargo build");
};
```

A file is loaded **once**: importing the same file from two places, or importing
in a cycle, is an error.

## Top-level values

A `let` at the top level is a top-level value. Top-level values are computed
once, when the program starts, in order, before `main`:

```kiru
let home<txt> = "/home/dev";

let backend<rec> = {
  name = "backend",
  dir = home + "/projects/backend",
};
```

A later reference reads the stored value; the initializer does not run again. A
top-level value can call a function declared above it, and cannot be assigned.
[Names and Scope](/concepts/names-and-scope/) covers the declare-before-use
rule.

## The entry

`main` is the **root namespace's** function — declared at the top level of some
file, not inside a `mod`. A `main` inside a `mod` is an ordinary function. The
compiler requires a root `main`, or the program does not build.
