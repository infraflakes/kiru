---
title: Match
description: Exact matching on any value, patterns, and the default arm.
---

Kiru has no `if`. To make a decision, you compare a value with `match`. It
compares the value against patterns and runs the first arm that matches:

```kiru
use std::text;
fn main() {
  let cmd: txt = "build";
  match cmd {
    "build" => { std::print("building"); };
    "test" => { std::print("testing"); };
    _ => { std::print("usage: build | test"); };
  };
};
```

## The subject

The subject is any value: text, a vec, an integer, a boolean, or a declared
type. Every pattern must have the same type as the subject, so a `{ … }` pattern
against a text subject is an error:

```console
$ kc main.kiru
main.kiru:3:5: error: expected text, found a `{ … }` literal
    {} => {};
    ^^
```

## Patterns

A pattern is a literal, a name, or a field path; it names the value to compare
and does not compute one:

```kiru
use std::text;
let fast: txt = "fast";

fn classify(mode: txt) -> txt {
  let result: txt = "other";
  match mode {
    "fast" => { result = "literal"; };
    fast => { result = "value"; };
  };
  return result;
};
fn main() { std::print(classify("slow")); };
```

A `{ … }` literal or a vec literal is a pattern too, and matches the whole
value exactly:

```kiru
use std;
use std::text;
use std::vec;
rec Spec { cmd: txt };
fn describe(spec: Spec, tags: Vec<txt>) -> txt {
  let by_spec: txt = match spec {
    { cmd = "build" } => "spec hit";
    _ => "spec miss";
  };
  let by_vec: txt = match tags {
    ["api", "prod"] => "vec hit";
    _ => "vec miss";
  };
  return "@(by_spec) @(by_vec)";
};
fn main() { std::print(describe({ cmd = "build" }, ["api"])); };
```

[Declared Types](/concepts/declared-types/) and [Vecs](/concepts/vecs/) cover the
two literal forms.

A pattern is built only from literals, names, and field paths — never a call, an
operator, or an interpolation:

```console
$ kc main.kiru
main.kiru:5:5: error: a match arm is a literal, a name, or a field path
    1 + 2 => {};
    ^^^^^
```

## Matching

Matching is exact. Text compares byte for byte, a vec compares element by
element in order, and a declared type compares field by field, so field order
does not matter:

```kiru
use std;
use std::text;
rec Spec { cmd: txt, Nix: txt };
fn main() {
  let spec: Spec = { cmd = "build", Nix = "true" };

  match spec {
    { Nix = "true", cmd = "build" } => { std::print("same spec"); };
    _ => {};
  };
};
```

The first match wins. The `_` arm is the default: it is not compared, runs when
nothing matches, and must be the last arm. Two arms with the same value are an
error, because the second can never match:

```console
$ kc main.kiru
main.kiru:4:6: error: duplicate match pattern `a`
    "a" => {};
     ^^
```

An arm after `_` is an error, because `_` already matches everything:

```console
$ kc main.kiru
main.kiru:4:6: error: the `_` arm must be last
    "x" => {};
     ^^
```

## A match is a value

A match is a term, so its role comes from where it sits. In expression position
its arms are expressions and it yields the taken arm's value:

```kiru
use std::text;
fn main() {
  let mode: txt = "fast";
  let label: txt = match mode {
    "fast" => "quick";
    _ => "slow";
  };
  std::print(label);
};
```

An expression match must be exhaustive, so it always takes an arm and always
yields a value. Without full coverage it is an error:

```console
$ kc main.kiru
main.kiru:2:20: error: a match that produces a value must cover every case; add a `_` arm
  let label: txt = match "fast" {
                   ^^^^^^^^^^^^^^
    "fast" => "quick";
^^^^^^^^^^^^^^^^^^^^^^
  };
^^^
```

A `_` arm is always enough. A boolean match that has arms for both `true` and
`false`, and an enum match that names every variant, already cover every value,
so no `_` arm is needed:

```kiru
use std;
use std::text;
fn describe(flag: bool) -> txt {
  let label: txt = match flag {
    true => "on";
    false => "off";
  };
  return label;
};
fn main() { std::print(describe(true)); };
```

[Declared Types](/concepts/declared-types/) covers variant patterns.

In statement position its arms are blocks, it runs for its effect, and the `_`
arm is optional — when nothing matches, nothing runs:

```kiru
use std::text;
fn main() {
  let mode: txt = "fast";
  match mode {
    "fast" => { std::print("quick"); };
    _ => { std::print("slow"); };
  };
};
```

## Arms and scope

Each arm is its own scope. An arm can reassign the enclosing function's
bindings, but a name declared in one arm does not leak to another. There is no
fallthrough and no arm-level `break`: an arm runs and the match is done.
