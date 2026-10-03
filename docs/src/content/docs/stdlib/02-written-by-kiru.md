---
title: Written by Kiru
description: command, print, log, and eprint, the library written in Kiru.
---

The rest of the standard library is Kiru source embedded in the compiler
and loaded into every program.

```text
std::command(spec, line) -> text    run one line and return its exit code
std::print(text) -> nothing         write a line to stdout
std::log(text) -> nothing           write an "INFO:" line to stdout
std::eprint(text) -> nothing        write an "ERROR:" line to stderr, then panic
```

All four live in the `std` namespace, so the unqualified `run` and `quote`
inside them are `std::run` and `std::quote`.

## command

```kiru
fn command(rec spec, txt line) {
  txt command_line = line;

  switch(spec.Dir) {
    case("") {};
    default { command_line = "cd " + quote(spec.Dir) + " && " + command_line; };
  };

  switch(spec.Env) {
    case("") {};
    default { command_line = "export " + spec.Env + "; " + command_line; };
  };

  switch(spec.Direnv) {
    case("true") { command_line = "direnv exec " + quote(spec.Dir) + " sh -c " + quote(command_line); };
    default {};
  };

  switch(spec.Stream) {
    case("stderr") { command_line = "{ " + command_line + "; } > /dev/null"; };
    case("null") { command_line = "{ " + command_line + "; } > /dev/null 2>&1"; };
    default {};
  };

  return(run(command_line));
};
```

`command` is text: it returns the exit code. `Stream` is implemented here by
wrapping the line, so an empty entry renders both streams live, `"stderr"`
hides stdout, and `"null"` hides both. `Direnv` wraps the line only when it is
exactly `"true"`.

## print

```kiru
fn print(txt message) {
  run("printf '%s\\n' " + quote(message));
};
```

`print` is `nothing`. The message is quoted as one shell word by
`std::quote`.

## log

```kiru
fn log(txt message) {
  emit("33", "INFO:", "1", message);
};
```

`log` is `print` with an `INFO:` prefix. The prefix is yellow when stdout is a
terminal.

## eprint

```kiru
fn eprint(txt message) {
  emit("31", "ERROR:", "2", message);
  panic;
};
```

`eprint` writes an `ERROR:` line to stderr, red when stderr is a terminal, and
then panics; the run exits nonzero.

## emit

`log` and `eprint` share `emit`, which colors a prefix only when the target
stream is a terminal and writes to the stream the caller names:

```kiru
fn emit(txt color, txt prefix, txt fd, txt message) {
  txt quoted = quote(message);
  txt colored = "'\\033[" + color + "m" + prefix + "\\033[0m %s\\n'";
  txt plain = "'" + prefix + " %s\\n'";
  run("if [ -t " + fd + " ]; then printf " + colored + " " + quoted + " >&" + fd + "; else printf " + plain + " " + quoted + " >&" + fd + "; fi");
};
```

`color` is an ANSI SGR code (`33` yellow, `31` red) and `fd` is `1` for stdout
or `2` for stderr. On a pipe the prefix is written plain, so a redirected log
stays clean.

## Embedding

The library is loaded into every program before the entry file, so its names
are always in `std`. A program that never prints does not carry `eprint`,
because unused library declarations are dropped at compile time.
