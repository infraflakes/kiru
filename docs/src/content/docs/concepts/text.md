---
title: Text
description: Literals, escapes, comments, and + as the only operator.
---

Text is the only scalar value Kiru has. Numbers, versions, paths, and flags are
all text, so text is where a program starts. In this section we'll look at how
to write text, the escapes it understands, and the one operator that joins it.

A literal is written with double quotes and may span lines:

```kiru
let name<txt> = "backend";
let message<txt> = "first line
second line";
let empty<txt> = "";
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
$ kc main.kiru
main.kiru:1:19: error: invalid escape `\q`; only \n, \t, \r, \e, \\, \", and \@ are allowed
let s<txt> = "bad \q";
                  ^^
```

## Comments and identifiers

A comment starts with `#` and runs to the end of the line:

```kiru
let retries<txt> = "3";   # numbers are text
```

An identifier starts with a letter or `_`, then letters, digits, or `_`.

## Concatenation

`+` joins two texts and is the only operator:

```kiru
fn label(name<txt>, code<txt>) -> txt {
  return "  " + name + ": " + code + "\n";
};
```

Both sides must be text; anything else is an error. There is no arithmetic and
no comparison operator; compare text with [Match](/concepts/match/).

`+` never changes its operands, so a command line is built exactly as written.
Text that must reach a command as data rather than as shell syntax is escaped
with `std::quote`; [std](/stdlib/std/) covers it.

## Interpolation

A string may insert a text expression with `@(…)`:

```kiru
let name<txt> = "backend";
let message<txt> = "building @(name)…";
```

The expression inside `@(…)` must be text, and may be any text expression: a
name, a field, a call, or another string. `\@` writes a literal `@(`:

```kiru
let at<txt> = "user\@host";   # user@host
```
