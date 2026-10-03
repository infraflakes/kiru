---
title: Text Concatenation
description: Concatenation, and why it is the only operator.
---

`+` concatenates text and is the only operator in the language:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn label(txt name, txt code) {
  return("  " + name + ": " + code + "\n");
};

fn main() {
  std::print(label("clippy", "0"));
};
```

`label` joins its two arguments with literal separators and appends a
newline, so the program prints `  clippy: 0`.

Both operands must be text. Anything else is a compile error:

```console
$ kc main.kiru
main.kiru:1:27: error: expected text, found record
fn main() { txt x = "a" + { b = "c" }; };
                          ^^^^^^^^^^^
```

## Left to Right

`+` groups left to right, which for concatenation is exactly the order the
text is written:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt line = "a" + "b" + "c";    # "abc"
```

The expression adds `"a"` and `"b"` first, then appends `"c"`, so `line` is
`abc`.

## There Are No Comparisons

There is no `==`, no `<`, and no `!=`. `switch` is the comparison:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn same(txt a, txt b) {
  txt result = "different";
  switch(a) {
    case(b) { result = "same"; };
  };
  return(result);
};

fn main() {
  std::print(same("a", "a"));
};
```

`same` starts with `different` and overwrites it when `a` matches `b`, so
the program prints `same`. A case pattern may be any text expression,
including another variable or a call, so comparing two texts is a switch with
one case and no default.

## No Arithmetic

There is no numeric operator. Counts, versions, and durations are text, and
any computation over them is a command's job:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt count = "1";
txt below_two = std::command({}, "test " + count + " -lt 2");

fn main() {
  switch(below_two) {
    case("0") { std::print("count is below two"); };
    default { std::print("count is two or more"); };
  };
};
```

`below_two` is a module value: the command runs once at startup, on the
machine that runs the program, and `switch` reads its exit code. A
computation reaches Kiru as an exit code, never as command output. [Module
values](/language/03-names-and-scope/05-module-values/) covers that
evaluation.

## Building Lines Safely

Because `+` only concatenates, a command line is always a string built from
strings. The language never parses or reinterprets it. A value that should
reach a command as data rather than as shell syntax is escaped with
`std::quote`, or carried through the spec's `Env` entry with caller-side
quoting; [written by Kiru](/stdlib/02-written-by-kiru/) shows how. The
plus operator never changes quoting, so the shell sees exactly the text
that was built.
