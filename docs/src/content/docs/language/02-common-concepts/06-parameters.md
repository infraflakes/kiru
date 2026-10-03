---
title: Parameters
description: Declaring a parameter's kind, call checking, and read-only bindings.
---

Each parameter declares its kind before its name: `txt` accepts text and
`rec` accepts a record. Parameters are read-only bindings.

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn clone(rec repo) {
  return(std::command({ Env = "URL='" + repo.url + "' DIR='" + repo.dir + "'" }, "git clone \"$URL\" \"$DIR\""));
};

fn main() {
  switch(clone({ name = "backend", url = "example", dir = "/tmp/backend" })) {
    case("0") { std::print("cloned"); };
    default { std::eprint("clone failed"); };
  };
};
```

`clone` takes a record, reads `repo.url` and `repo.dir`, sets them as
environment variables in the spec, runs the command, and returns the exit
code as text; `main` switches on that code. The line is consumed inside
`clone`, so no command crosses the function boundary.

## Kinds Are Declared

A parameter's kind is written at its declaration. `txt` accepts text, and
`rec` accepts a record. A call is checked against the declared kind, and a
disagreement is a compile error at the call. A parameter is the only place
a kind is written, and every other kind is fixed by the declaration or the
expression form; there is no inference.

## A Spec and a Line Can Cross a Boundary

The command spec is an ordinary record and the line is text, so a helper can
take both and call `std::command` itself. A function that runs a command in a
directory takes a record and a command line, and builds the call inside. A
`panic;` is a statement, not a value, so it cannot be passed as an argument;
`async` takes a call, not a statement.

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
