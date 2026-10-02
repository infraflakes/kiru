---
title: Modules
description: Namespaces, the reserved std namespace, and how names resolve.
---

Every file belongs to a *namespace*, a path of identifiers that keeps names
apart without prefixes on every declaration. A file that opens with a
`module` declaration belongs to that path:

<span class="filename">Filename: build.kiru</span>

```kiru
module tasks::build;
```

The declaration must be the first thing in the file and may appear once.
Files that declare the same path share one namespace, so a namespace can be
split across files. A file without a `module` declaration belongs to the
*root namespace*, which is the namespace the entry file lives in. [Unique
names](/language/03-names-and-scope/02-unique-names/) explains the
duplicate-name rules.

## Two Registries

A namespace holds values and functions in two registries. A value name is
declared once and a function name is declared once, and a function may
share a name with a value because `name` and `name(...)` disambiguate. Two
values, or two functions, with one name in a namespace are an error.

## Nested Paths

Paths nest, so a namespace `tasks::build` lives inside `tasks`. An
unqualified name in `tasks::build` is looked up in that namespace, then in
its parent, and so on to the root; a name in `tasks::build` wins over a
name of the same spelling in `tasks`.

A file `tasks.kiru` that declares the parent namespace:

<span class="filename">Filename: tasks.kiru</span>

```kiru
module tasks;

fn root_name() {
  return "tasks";
};
```

A file `build.kiru` that declares the child namespace and calls the parent
name unqualified:

<span class="filename">Filename: build.kiru</span>

```kiru
module tasks::build;
import "tasks.kiru";

fn label() {
  return root_name();
};
```

An entry that imports both and calls the qualified name:

<span class="filename">Filename: main.kiru</span>

```kiru
import "tasks.kiru";
import "build.kiru";

fn main() {
  std::print(tasks::build::label());
};
```

Inside `tasks::build::label`, `root_name` resolves to
`tasks::root_name`, so the program prints `tasks`.

## The Reserved std Namespace

`std` is reserved and implicit. User code cannot declare anything under it:

```console
$ kc main.kiru
main.kiru:1:1: error: `std` is reserved and cannot be declared
module std;
^^^^^^^^^^^
```

The standard library lives there and is always visible: `std::command`
resolves in every file without an import. [Appendix
A](/appendix/a-standard-library/) lists the shipped names.

## How Resolution Works

A qualified name is looked up in the namespace it names:
`tasks::build::label` searches `tasks::build`, and a namespace is reachable
only when the file imports it directly or is declared in it. An unqualified
name climbs: the current namespace, then each parent, then the root.

A leading `::` names the root namespace explicitly, so `::name` and
`::build()` reach the root even when a local or an inner namespace shadows
the name.

Resolution happens at compile time, and only declarations above the
reference are visible. [Declaration
order](/language/03-names-and-scope/01-declaration-order/) covers that rule.
