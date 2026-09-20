---
title: Unique Names
description: One value name and one function name per namespace, and how to change a value.
---

A namespace keeps values and functions in two registries. A value name is
declared once, a function name is declared once, and a function may share a
name with a value. At the top level of a file, a second `txt` of the same
name is rejected:

```console
$ kc main.kiru
main.kiru:2:5: error: `title` is declared more than once in this namespace
txt title = "b";
    ^^^^^
```

The rule applies across scopes as well: within a body, a local cannot
reuse a name that is already visible, and each `case` arm has its own
scope, so an arm's bindings do not collide with another arm's.

## One Binding per Name

With one binding per name, a name has one declaration and one current
value. The name always refers to that declaration, so there is no shadowing
to reason about when reading a body.

## Changing a Value

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

Assignment keeps the declaration and replaces the value; it is described in
[assignment and read-only
parameters](/language/03-names-and-scope/03-assignment/).

## A Function May Share a Value's Name

Because `name` and `name(...)` disambiguate, a function and a value can use
one name:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt thing = "value thing";
fn thing() { return("function thing"); };
fn main() {
  std::print(thing);
  std::print(thing());
};
```

```console
$ ./app
value thing
function thing
```

The bare name reads the value; the call reaches the function.

## Redeclaring a Function

Two functions with one name in a namespace are a duplicate:

```console
$ kc main.kiru
main.kiru:2:4: error: `build` is declared more than once in this namespace
fn build() {};
   ^^^^^
```

This includes `main`: two `main`s in the root namespace collide, which is
why an entry file and a root helper cannot both declare one. [The entry
file](/language/05-modules-and-namespaces/03-the-entry-file/) covers that
case.
