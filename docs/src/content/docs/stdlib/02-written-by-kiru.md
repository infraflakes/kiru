---
title: Written by Kiru
description: command, print, and eprint, the library written in Kiru.
---

The rest of the standard library is Kiru source embedded in the compiler
and loaded into every program.

```text
std::command(spec, text) -> text    run one line and return the mode's text
std::print(text) -> nothing         write a line to stdout
std::eprint(text) -> nothing        write a line to stderr, then panic; "" silently
```

All three live in the `std` namespace, so the unqualified `run` inside
`command` is `std::run`.

## command

```kiru
fn command(rec spec, txt line) {
  txt command_line = line;

  switch(spec.Dir) {
    case("") {};
    default { command_line = "cd " + spec.Dir + " && " + command_line; };
  };

  switch(spec.Env) {
    case("") {};
    default { command_line = "export " + spec.Env + "; " + command_line; };
  };

  switch(spec.Direnv) {
    case("") {};
    default { command_line = "direnv exec " + spec.Dir + " sh -c '" + command_line + "'"; };
  };

  switch(spec.Mode) {
    case("") {
      command_line = "{ " + command_line + "; } >/dev/null 2>&1";
    };
    default {};
  };

  switch(spec.Mode) {
    case("stdout")    { return run(command_line).out; };
    case("exit code") { return run(command_line).code; };
    default           { run(command_line); return ""; };
  };
};
```

`command` is text: every path returns, and an empty `Mode` returns `""`.

## print

```kiru
fn print(txt message) {
  command({ Env = "KIRU_MESSAGE='" + message + "'", Mode = "stdout" }, "printf '%s\\n' \"$KIRU_MESSAGE\"");
};
```

`print` is void. The message travels through the environment as one single
quoted shell word.

## eprint

```kiru
fn eprint(txt message) {
  switch(message) {
    case("") {};
    default {
      command({ Env = "KIRU_MESSAGE='" + message + "'", Mode = "stdout" }, "printf '%s\\n' \"$KIRU_MESSAGE\" >&2");
    };
  };
  panic;
};
```

`eprint` is void: it has no `return`, and its body ends in `panic;`. An empty
message skips the print and panics silently.

## Embedding

The library is loaded into every program before the entry file, so its names
are always in `std`. A program that never prints does not carry `eprint`,
because unused library declarations are dropped at compile time.
