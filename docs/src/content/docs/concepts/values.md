---
title: Values and Types
description: Bindings, the types, literals, operators, and what the compiler checks.
---

A binding gives a name to a value. Every value has a type, and the compiler
checks every expression against it before the program is built, so a wrong value
can never reach a running program.

## Bindings

`let name: Type = value;` declares a binding: the name, then the type after a
colon, then the value. The type is always written; a binding never infers one.

```kiru
use std::text;
fn main() {
  let name: txt = "backend";
  std::print(name);
};
```

A binding may be reassigned. `name = value;` stores a new value, and the value
must have the binding's type:

```kiru
use std::text;
fn main() {
  let state: txt = "idle";
  state = "running";
  std::print(state);    # running
};
```

`name.field = value;` assigns one field of a declared type's value. [Declared
Types](/concepts/declared-types/) covers fields. A vec changes through its
operations, not an index: `xs = xs.replace(i, v)` replaces an element and
`xs = xs.push(v)` appends. [Vecs](/concepts/vecs/) covers it.

A parameter is a local copy: the function may reassign it and assign its fields,
and the caller's value is unchanged:

```kiru
use std::text;
fn normalize(name: txt) -> txt {
  name = "@(name)!";
  return name;
};
fn main() { std::print(normalize("x")); };    # x!
```

A `let` at the top level is a module value: computed once when the program
starts, and never assignable. [Namespaces](/concepts/modules/) covers it.

## The types

The runtime provides three intrinsic types:

```text
txt       string data
bool      true or false
int       a whole number
```

They are not keywords and not declarations: they are the language's own types,
visible in every namespace, so writing one needs no `use`.

Everything else is a declared type: a `rec` or an `enum`, written `<Name>` or
applied to type arguments (`Vec<T>`). `Vec<T>`, the sequence type, is such a
declared `rec` in `std::dynamic`; so is `HashMap<K, V>`, and `Option<T>` is a
declared `enum` in `std`. A declared type is reached by its path or with a
`use`, exactly as any other name is.

Text is the interchange type at the OS boundary — paths, versions, and command
output are text — while `bool` and `int` are for decisions and counting inside a
program.

A declared type is a `rec` (a fixed set of typed fields) or an `enum` (one of a
fixed set of variants); [Declared Types](/concepts/declared-types/) covers both.
A call that returns no value produces no value at all: a function with no return
type has no return type to name, so such a call may only stand as a statement.
[Functions](/concepts/functions/) covers it.

## Literals

Each intrinsic type has its own literal form, and a declared type uses the
`{ … }` literal:

```kiru
use std;
use std::text;
use std::dynamic::Vec;
fn main() {
  let name: txt = "backend";                 # text
  let retries: int = 3;                      # an integer
  let verbose: bool = true;                  # a boolean
  let tags: Vec<txt> = Vec.new();            # a vec, built with its operations
  tags = tags.push("api");
  std::print("@(name) @(retries) @(verbose) @(tags.len())");
};
```

| Type | Annotation | Bound value |
| --- | --- | --- |
| text | `: txt` | a string literal, an interpolation, a field access, a name, or a call returning text |
| int | `: int` | an integer literal, an arithmetic expression, a name, or a call returning an integer |
| bool | `: bool` | `true`, `false`, a comparison, a logical expression, a name, or a call returning a boolean |
| vec | `: Vec<T>` | a `Vec.new()` call, a vec variable, or a call returning a vec |

A `{ … }` literal takes its type from the position it is checked against, so one
with no expected type is an error; it is the literal for a declared type, not an
intrinsic ([Declared Types](/concepts/declared-types/)). A vec has no literal:
it is built and changed through `new`, `push`, `replace`, `at`, and `len`
([Vecs](/concepts/vecs/)).

## Operators

```text
+ - * / %        arithmetic on integers, yielding an integer
< <= > >=        ordering of integers, yielding a boolean
== !=            equality of two values of one type, yielding a boolean
&& || !          boolean logic
-                integer negation (prefix)
```

`+` adds integers; it does not join text. Build text with interpolation instead:
`"@(left)@(right)"`. [Text](/concepts/text/) covers how interpolation inserts a
value.

Integers are 64-bit. Division and remainder by zero stop the run. `%` takes the
sign of its left operand, so `-7 % 3` is `-1` and `7 % -3` is `1`; division
truncates toward zero, so `7 / 2` is `3` and `-7 / 2` is `-3`.

`&&` and `||` short-circuit: the right operand runs only when the left does not
decide the result, so `false && f()` never calls `f` and `true || f()` never
calls `f`.

## Precedence

A tighter operator binds first, and `()` groups. From tightest to loosest:

```text
- !              prefix
* / %
+ -
< <= > >=
== !=
&&
||
```

Binary operators of equal precedence group left to right, so `a - b - c` is
`(a - b) - c`.

## What the compiler checks

The compiler checks every expression's type before the program is built. A
binding's type is written, never inferred; a *generic call* is the one place a
type is inferred, from its arguments ([Generics](/concepts/generics/) covers
that). Every other type comes from the declaration or the expression form:

- a string literal is text, an integer literal is int, and `true`/`false` is bool;
- a `{ … }` literal takes its type from its position, and a `Vec` value comes from its operations;
- `left + right` is int, and both sides must be int;
- a comparison yields bool, and its operands are checked per operator;
- `&&` and `||` require bool on both sides and yield bool;
- a field access is only for a declared field and has that field's type, and a `HashMap` lookup `m.get(key)` has the value type `V`; [Hash maps](/concepts/hash-maps/) covers it;
- `xs.at(i)` has the element type of `Vec<T>`, and the index must be `int`;
- a call has the type the callee declares; a call that returns nothing has no type and may only stand as a statement;
- a parameter's type is written after a colon at its declaration, and every call is checked against it;
- a function's return type is `-> txt`, `-> Vec<T>`, `-> bool`, `-> int`, a declared type, or absent.

Types are exact: a value fits a position only when the types are equal. There is
no subtyping and no group of types.

A disagreement is an error at the operator or expression:

```console
$ kc
main.kiru:2:31: error: expected int, found text
use std;
fn main() { let total: int = "1" + "2"; };
                              ^^
```

A compiled program cannot fail because a value had the wrong type: every
operator, field access, and call was checked before the binary existed.
