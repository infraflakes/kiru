---
title: Text
description: Literals, escapes, comments, interpolation, ordering, and bytes.
---

Text is the type a program shares with the operating system: paths, versions,
and command output are all text. A literal is written with double quotes and may
span lines:

```kiru
use std::text;
fn main() {
  let name: txt = "backend";
  let message: txt = "first line
second line";
  let empty: txt = "";
  std::print(name);
};
```

## Escapes

Seven escapes are recognized:

```text
\n    newline
\t    tab
\r    carriage return
\e    escape (the ANSI escape character)
\\    backslash
\"    double quote
\@    a literal @(
```

Any other escape is an error:

```console
$ kc
main.kiru:1:19: error: invalid escape `\q`; only \n, \t, \r, \e, \\, \", and \@ are allowed
let s: txt = "bad \q";
                  ^^
```

## Comments and identifiers

A comment starts with `#` and runs to the end of the line:

```kiru
use std;
fn main() { let retries: int = 3;   # an integer
};
```

An identifier starts with a letter or `_`, then letters, digits, or `_`.

## Building text

Text is built by interpolation, not by `+`. `+` is integer addition; to join a
value into text, write it inside `@(…)`:

```kiru
use std;
use std::text;
fn label(name: txt, code: int) -> txt {
  return "  @(name): @(code)\n";
};
fn main() { std::print(label("build", 3)); };
```

`std::process::quote` escapes text that must reach a command as data rather than
as shell syntax; [std::process](/stdlib/process/) covers it.

## Interpolation

A string may insert an expression with `@(…)`. The expression may be text, an
integer, or a boolean; an integer is written in decimal and a boolean as `true`
or `false`:

```kiru
use std;
use std::text;
fn main() {
  let name: txt = "backend";
  let count: int = 3;
  let message: txt = "building @(name) (@(count))…";
  let enabled: txt = "verbose: @(true)";
  std::print(message);
  std::print(enabled);
};
```

The expression may be any expression of those types: a name, a field, a call, an
arithmetic or comparison expression, or another string. `\@` writes a literal
`@(`:

```kiru
use std::text;
fn main() {
  let at: txt = "user\@host";   # user@host
  std::print(at);
};
```

## Ordering

`<`, `<=`, `>`, and `>=` order text byte by byte as well as integers, and `==`
and `!=` compare any two values for equality:

```kiru
use std;
fn main() {
  let before: bool = "apple" < "banana";   # true
  let same: bool = "a" == "a";             # true
  std::print("@(before) @(same)");
};
```

A hash map and a vec cannot be ordered, only compared for equality.

## Bytes

Text is a sequence of bytes, and every index counts bytes rather than
characters. `strlen` is the byte count, `byte_at` reads one byte, and
`substr(start, end)` is the half-open range of bytes:

```kiru
use std;
use std::text;
fn main() {
  let word: txt = "héllo";
  std::print("@(word.strlen())");       # 6, not 5
  std::print(word.substr(0, 3));        # hé
};
```

Because the byte count and the character count differ, a `substr` range that
falls inside a character is refused: `substr` stops the run rather than produce
text that is not UTF-8. `find` returns a byte index, and an absent needle is the
absence ([Absence](/concepts/absence/) covers it):

```kiru
use std;
use std::text;
fn main() {
  let line: txt = "a=b";
  let needle: txt = "=";
  std::print("@(line.find(needle).or(-1))");   # 1
};
```
