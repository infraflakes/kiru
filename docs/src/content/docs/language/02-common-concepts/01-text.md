---
title: Text
description: The only scalar, empty strings, and escapes.
---

Text is the only scalar kind. A literal is written with double quotes:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt name = "backend";
txt path = "projects/backend";
txt empty = "";
```

Text may span lines. A newline inside the quotes is data:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt message = "first line
second line";
```

## Escapes

Five escapes are recognized:

```text
\n    newline
\t    tab
\r    carriage return
\\    backslash
\"    double quote
```

Any other escape is a compile error:

```console
$ kc main.kiru
main.kiru:1:14: error: invalid escape `\q`; only \n, \t, \r, \\, and \" are allowed
txt s = "bad \q";
             ^^
```

`""` is an ordinary text value. There are no numeric literals, so numbers are text:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt retries = "3";
txt timeout = "600";
```

Arithmetic does not exist.
