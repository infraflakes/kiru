---
title: Match
description: Exact matching on text, records, and lists, patterns, and the default arm.
---

Kiru has no `if`. To make a decision, you compare a value with `match`. In this
section we'll see which subjects and patterns it accepts and how it matches.

`match` compares a value against patterns and runs the first arm that matches:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  let cmd<txt> = "build";
  match cmd {
    "build" => { std::print("building"); };
    "test" => { std::print("testing"); };
    _ => { std::print("usage: build | test"); };
  };
};
```

## The subject

The subject is any text, record, or list expression. Every pattern must have the
same type as the subject, so a record pattern against a text subject is an
error:

```console
$ kc main.kiru
main.kiru:3:5: error: expected text, found record
    {} => {};
    ^^
```

## Patterns

A pattern is a literal, a name, or a field path; it names the value to compare
and does not compute one:

```kiru
let fast<txt> = "fast";

fn classify(mode<txt>) -> txt {
  let mut result<txt> = "other";
  match mode {
    "fast" => { result = "literal"; };
    fast => { result = "value"; };
  };
  return result;
};
```

A record literal or a list literal is a pattern too, and matches the whole
value exactly:

```kiru
fn describe(spec<rec>, tags<list>) -> txt {
  let by_record<txt> = match spec {
    { cmd = "build" } => "record hit";
    _ => "record miss";
  };
  let by_list<txt> = match tags {
    ["api", "prod"] => "list hit";
    _ => "list miss";
  };
  return by_record + " " + by_list;
};
```

A pattern is built only from literals, names, and field paths — never a call, a
concatenation, or an interpolation:

```console
$ kc main.kiru
main.kiru:5:5: error: a match arm is a literal, a name, or a field path
    name() => {};
    ^^^^^^
```

## Matching

Matching is exact. Text compares byte for byte, a list compares element by
element in order, and a record compares as a map, so field order does not
matter:

```kiru
fn main() {
  let spec<rec> = { cmd = "build", Nix = "true" };

  match spec {
    { Nix = "true", cmd = "build" } => { std::print("same record"); };
    _ => {};
  };
};
```

The first match wins. The `_` arm is the default: it is not compared and runs
when nothing matches. Two arms with the same value are an error, because the
second can never match:

```console
$ kc main.kiru
main.kiru:4:6: error: duplicate match pattern `a`
    "a" => {};
     ^^
```

## A match is a value

A match is a term, so its role comes from where it sits. In expression position
its arms are expressions and it yields the taken arm's value:

```kiru
let mode<txt> = "fast";
let label<txt> = match mode {
  "fast" => "quick";
  _ => "slow";
};
```

An expression match must have a `_` arm, so it always takes an arm and always
yields a value. Without one it is an error:

```console
$ kc main.kiru
main.kiru:2:20: error: a match that produces a value needs a `_` arm
  let label<txt> = match "fast" {
                   ^^^^^^^^^^^^^^
    "fast" => "quick";
^^^^^^^^^^^^^^^^^^^^^^
  };
^^^
```

In statement position its arms are blocks, it runs for its effect, and the `_`
arm is optional — when nothing matches, nothing runs:

```kiru
match mode {
  "fast" => { std::print("quick"); };
  _ => { std::print("slow"); };
};
```

## Arms and scope

Each arm is its own scope. An arm can reassign the enclosing function's `mut`
bindings, but a name declared in one arm does not leak to another. There is no
fallthrough and no arm-level `break`: an arm runs and the match is done.
