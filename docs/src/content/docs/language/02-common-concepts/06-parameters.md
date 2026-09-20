---
title: Parameters
description: Declaring a parameter's kind, call checking, and read-only bindings.
---

Each parameter declares its kind before its name: `txt` accepts text and
`rec` accepts a record. Parameters are read-only bindings.

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn clone(rec repo) {
  return(std::command("git clone \"$URL\" \"$DIR\"")
    .env({ URL = repo.url, DIR = repo.dir })
    .stream()
    .code());
};

fn main() {
  switch(clone({ name = "backend", url = "example", dir = "/tmp/backend" })) {
    case("0") { std::print("cloned"); };
    default { std::eprint("clone failed"); };
  };
};
```

`clone` takes a record, reads `repo.url` and `repo.dir`, builds a command
chain, sets the two values as environment variables, runs it, and returns
the exit code as text; `main` switches on that code. The chain is consumed
inside `clone`, so no command crosses the function boundary.

## Kinds Are Declared

A parameter's kind is written at its declaration. `txt` accepts text, and
`rec` accepts a record. A call is checked against the declared kind, and a
disagreement is a compile error at the call. A parameter is the only place
a kind is written, and every other kind is fixed by the declaration or the
expression form; there is no inference.

## Command Chains Cannot Cross a Boundary

A command chain is not data, so it cannot be passed or returned. A
parameter is declared `txt` or `rec`, and neither is a chain; using a text
value as a command is rejected at the use:

```console
$ kc main.kiru
main.kiru:3:3: error: expected command, found text
  j.stream();
  ^
```

A function that needs to run a command builds it itself, or receives the
text it needs. A helper that runs a command in a directory takes a record
and a command line, not a chain, and builds the command inside. A call
that never returns, such as `std::panic`, is not a value either: it may
stand as a statement, but it cannot be passed as an argument, except as the
invocation `std::async` spawns.

## Read-Only

A parameter cannot be assigned. [Assignment and read-only
parameters](/language/03-names-and-scope/03-assignment/) covers the rule and
the workaround of declaring a `txt`.

## Returning Parameters

A function may return a parameter:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn identity(txt value) {
  return(value);
};

fn main() {
  std::print(identity("x"));
};
```

`identity` takes text and returns it, and the call prints `x`.
