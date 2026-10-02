---
title: The Entry File
description: What makes a file the entry, and what happens to its main.
---

The entry is the file given to `kc`. The compiled binary runs that file's
own `main`, and that `main` must be declared in the root namespace. The
entry file cannot declare a module; keeping it in the root namespace is
what makes its `main` the entry. [The entry
function](/language/09-entry-and-the-cli/01-main/) lists the accepted
signatures.

## Main in Other Namespaces

A `main` in any other namespace is an ordinary function, reached with `::`
like anything else:

<span class="filename">Filename: tools.kiru</span>

```kiru
module tools;

fn main() {
  std::print("not the entry");
  return "";
};
```

An entry that imports this file can call `tools::main()`, and the call runs
it as a function. It has no special meaning, and a program may contain
several.

## Duplicate main in the Root

Root-namespace files merge, so two root files that both declare `main`
collide:

```console
$ kc main.kiru
main.kiru:2:4: error: `main` is declared more than once in this namespace
fn main() {};
   ^^^^
```

The rule is not special to `main`; every duplicate function name in a
namespace is an error, as [unique
names](/language/03-names-and-scope/02-unique-names/) describes.

## The Entry File Must Declare main

An entry file with no `main` is a compile error, as is a `main` with more
than one parameter:

```console
$ kc empty.kiru
empty.kiru:1:1: error: the entry file has no `main` function
fn note() {};
^
```

The parameter is optional. `fn main()` ignores the command line;
`fn main(rec args)` receives the args record. Both are valid, and a
program that reads no arguments declares the first. The entry `main` is
void: no caller binds its value, so `return;` and `return expr;` both end it
and a returned value is discarded.
