---
title: Unique Names
description: One value name and one function name per namespace, and how to change a value.
---

A namespace keeps values and functions in two registries. A value name is declared once per namespace, a function name is declared once per namespace, and a function may share a name with a value. At the top level of a file, a second `txt` of the same name is rejected:

```console
$ kc main.kiru
main.kiru:2:5: error: `title` is declared more than once in this namespace
txt title = "b";
    ^^^^^
```

Within one body, a local cannot duplicate a name already local in that body, but a local may shadow a module-level name; `::name` reaches the module one. Each `case` arm has its own scope, so an arm's bindings do not collide with another arm's. [Declaration order](/language/03-names-and-scope/01-declaration-order/) covers visibility.

Change a value with assignment, not by declaring again:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  txt log = "";
  log = log + "first\n";
  log = log + "second\n";
  std::print(log);
};
```

```console
$ ./app
first
second
```

Assignment keeps the declaration and replaces the value; [assignment and read-only parameters](/language/02-common-concepts/08-assignment/) covers it.

Because `name` and `name(...)` disambiguate, a function and a value can share one name:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt thing = "value thing";
fn thing() -> txt { return("function thing"); };
fn main() {
  std::print(thing);
  std::print(thing());
};
```

Two functions with one name in a namespace are a duplicate:

```console
$ kc main.kiru
main.kiru:2:4: error: `build` is declared more than once in this namespace
fn build() {};
   ^^^^^
```

This includes `main`: two `main`s in the root namespace collide. [The entry file](/language/05-modules-and-namespaces/03-the-entry-file/) covers that case.
