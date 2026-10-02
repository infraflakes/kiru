---
title: Switch
description: Exact text matching, first match, defaults, and scope.
---

`switch` compares a subject against patterns and runs the first arm that
matches:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn build() {
  std::print("building");
  return "";
};

fn test() {
  std::print("testing");
  return "";
};

fn main(rec args) {
  switch(args.cmd) {
    case("build") { build(); };
    case("test") { test(); };
    default { std::eprint("usage: build | test"); };
  };
};
```

`./app build` matches the first arm and prints `building`; an unknown or
missing command runs the `default` arm.

## The Rules

- The subject produces text.
- Every case pattern produces text: a literal, a variable, a call,
  `.code`, `.out`, or any other text expression.
- Matching is exact text comparison; the first match wins.
- The untaken arms are not executed.
- `default` is optional, is not compared, and runs when nothing matched.
- When nothing matches and there is no `default`, nothing runs.
- Duplicate case data is a compile error; identical patterns are detected
  structurally.

## First Match Wins

Two arms may both match; only the first runs:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt fast = "fast";

fn classify(txt mode) {
  txt result = "other";
  switch(mode) {
    case("fast") { result = "literal"; };
    case(fast) { result = "value"; };
  };
  return result;
};

fn main() {
  std::print(classify("fast"));
};
```

The literal arm comes first, so the program prints `literal`; the second
arm with the variable is never evaluated for that input. Duplicate case data
is detected structurally: two patterns that are the same literal, the same
reference, or the same call shape are a duplicate, because the later one can
never run:

```console
$ kc main.kiru
main.kiru:4:10: error: duplicate case pattern `a`
    case("a") {};
         ^^^
```

The same rule catches `case(x)` twice and `case(name())` twice. Two
patterns that differ syntactically, such as two different calls, are not
compared; the first matching arm wins at run time.

## Patterns Are Text Expressions

A pattern may be anything that produces text, including a parameter or a
call:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn name() {
  return "a";
};

fn pick(txt target) {
  txt result = "none";
  switch("a") {
    case(target) { result = "chosen"; };
    case(name()) { result = "named"; };
  };
  return result;
};

fn main() {
  std::print(pick("a"));
};
```

The pattern `target` holds text, so it compares like a literal; the program
prints `chosen`.

A record is not text, so it cannot be a pattern:

```console
$ kc main.kiru
main.kiru:3:10: error: expected text, found record
    case({}) {};
         ^^
```

## Arms Are Their Own Scope

Each `case` arm is its own scope. An arm shares the bindings of the body
the `switch` is in, so it can assign them, but a `txt` declared in one arm
does not leak to another:

```console
$ kc main.kiru
main.kiru:4:28: error: unknown name `hidden`
    case("b") { std::print(hidden); };
                           ^^^^^^
```

The `hidden` binding belongs to the first arm, so the second arm cannot
name it. [Bodies and
scopes](/language/03-names-and-scope/04-bodies-and-scopes/) describes the
shared body the arms do see.

## No Fallthrough

There is no fallthrough and no `break`. An arm runs its body and the switch
is done. To share behavior, call a function from several arms:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn run(txt mode) {
  return "running " + mode;
};

fn main(rec args) {
  switch(args.flag) {
    case("fast") { std::print(run("fast")); };
    case("quick") { std::print(run("fast")); };
    default { std::print(run("safe")); };
  };
};
```

Both `fast` and `quick` call `run("fast")`; the default calls
`run("safe")`.
