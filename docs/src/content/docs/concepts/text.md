---
title: Text
description: Literals, escapes, comments, and + as the only operator.
---

Text is the only scalar value Kiru has. Numbers, versions, paths, and flags are
all text, so text is where a program starts. In this section we'll look at how
to write text, the escapes it understands, and the one operator that joins it.

A literal is written with double quotes and may span lines:

```kiru
txt name = "backend";
txt message = "first line
second line";
txt empty = "";
```

## Escapes

Six escapes are recognized:

```text
\n    newline
\t    tab
\r    carriage return
\e    escape (the ANSI escape character)
\\    backslash
\"    double quote
```

Any other escape is an error:

```console
$ kc main.kiru
main.kiru:1:14: error: invalid escape `\q`; only \n, \t, \r, \e, \\, and \" are allowed
txt s = "bad \q";
             ^^
```

## Comments and identifiers

A comment starts with `#` and runs to the end of the line:

```kiru
txt retries = "3";   # numbers are text
```

An identifier starts with a letter or `_`, then letters, digits, or `_`.

## Concatenation

`+` joins two texts and is the only operator:

```kiru
fn label(txt name, txt code) -> txt {
  return("  " + name + ": " + code + "\n");
};
```

Both sides must be text; anything else is an error. There is no arithmetic and
no comparison operator; compare text with [Switch](/concepts/switch/).

`+` never changes its operands, so a command line is built exactly as written.
Text that must reach a command as data rather than as shell syntax is escaped
with `std::process::quote`; [std::process](/stdlib/process/) covers it.
