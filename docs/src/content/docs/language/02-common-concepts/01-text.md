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

All three declarations bind text, including `empty`, which binds the empty
string.

Text may span lines. A newline inside the quotes is data:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt message = "first line
second line";
```

`message` contains the newline character between the two lines.

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

Everything else between the quotes is data, including a literal newline.

## The Empty String

`""` is an ordinary value everywhere: as an argument, a record value, an
assignment, a switch pattern, and a switch subject.

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn label(txt value) {
  txt result = value;
  switch(value) {
    case("") { result = "unset"; };
  };
  return(result);
};

fn main() {
  std::print(label(""));
};
```

The `case("")` arm matches the empty argument, so the program prints
`unset`.

## No Numbers

There are no numeric literals, so numbers are text:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt retries = "3";
txt timeout = "600";
```

Arithmetic does not exist. A count is compared or concatenated as text, and
anything that needs arithmetic is a command's job.
