---
title: Written by Kiru
description: command, print, log, and eprint, the library written in Kiru.
---

The rest of the standard library is Kiru source embedded in the compiler and
loaded into every program:

| Name | Signature | Behavior |
| --- | --- | --- |
| `std::command` | `(rec spec, txt line) -> text` | run one line and return its exit code |
| `std::print` | `(txt message) -> nothing` | write a line to stdout |
| `std::log` | `(txt message) -> nothing` | write an `INFO:` line to stdout |
| `std::eprint` | `(txt message) -> nothing` | write an `ERROR:` line to stderr, then panic |
| `std::emit` | `(txt color, txt prefix, txt file_descriptor, txt message) -> nothing` | write a prefixed, optionally colored line to one stream |

All five live in the `std` namespace, so the unqualified `run` and `quote`
inside them are `std::run` and `std::quote`.

```kiru
fn command(rec spec, txt line) -> txt {
  txt command_line = line;

  switch(spec.Dir) {
    case("") {};
    default { command_line = "cd " + quote(spec.Dir) + " && " + command_line; };
  };

  switch(spec.Env) {
    case("") {};
    default { command_line = "export " + spec.Env + "; " + command_line; };
  };

  switch(spec.Nix) {
    case("true") { command_line = "nix develop -c sh -c " + quote(command_line); };
    default {};
  };

  switch(spec.Direnv) {
    case("true") { command_line = "direnv exec " + quote(spec.Dir) + " sh -c " + quote(command_line); };
    default {};
  };

  switch(spec.Timeout) {
    case("") {};
    default { command_line = "timeout " + spec.Timeout + " sh -c " + quote(command_line); };
  };

  switch(spec.Stream) {
    case("stderr") { command_line = "{ " + command_line + "; } > /dev/null"; };
    case("null") { command_line = "{ " + command_line + "; } > /dev/null 2>&1"; };
    default {};
  };

  return(run(command_line));
};

fn print(txt message) {
  run("printf '%s\\n' " + quote(message));
};

fn log(txt message) {
  emit("33", "INFO:", "1", message);
};

fn eprint(txt message) {
  emit("31", "ERROR:", "2", message);
  panic;
};

fn emit(txt color, txt prefix, txt file_descriptor, txt message) {
  txt quoted = quote(message);
  txt colored = "'\\033[" + color + "m" + prefix + "\\033[0m %s\\n'";
  txt plain = "'" + prefix + " %s\\n'";
  run("if [ -t " + file_descriptor + " ]; then printf " + colored + " " + quoted + " >&" + file_descriptor + "; else printf " + plain + " " + quoted + " >&" + file_descriptor + "; fi");
};
```

`emit` colors a prefix only when the target stream is a terminal, so a
redirected log stays clean. `color` is an ANSI SGR code (`33` yellow, `31`
red) and `file_descriptor` is `1` for stdout or `2` for stderr. `std::eprint`
ends in `panic;`, so a call to it stops the run and can end a value function's
path. The `std::command` spec is documented in
[the command spec](/effects/06-commands/02-builders/).

## Embedding

The library is loaded into every program before the entry file, so its names
are always in `std`. A program that never prints does not carry `eprint`,
because unused library declarations are dropped at compile time.
