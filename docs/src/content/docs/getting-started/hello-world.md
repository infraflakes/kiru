---
title: Hello, World!
description: Write, compile, and run your first Kiru program.
---

Now that Kiru is installed, the next step is a program that prints
`Hello, world!` to the screen.

## Writing the Program

Make a directory for your projects and one for this program:

```console
$ mkdir ~/projects
$ cd ~/projects
$ mkdir hello
$ cd hello
```

A Kiru project is one manifest beside its tree. Make a `kiru.toml` that names
this directory as the program tree and the binary `main`:

<span class="filename">Filename: kiru.toml</span>

```toml
[project]
root = "."
name = "main"
```

Kiru source files end in `.kiru`. Make a file called `main.kiru` and enter this
program:

<span class="filename">Filename: main.kiru</span>

```kiru
fn main() {
  std::print("hello, world");
};
```

Compile it with `kc` and run the binary:

```console
$ kc
$ ./main
hello, world
```

If you see `hello, world`, you have written and run a Kiru program. If not,
check the [Installation](/getting-started/installation/) chapter.

## The Anatomy of a Program

The program has two parts. The outer part declares the entry function:

```kiru
fn main() {
};
```

`main` is where execution begins. The body is wrapped in `{}`, and the whole
declaration ends with a `;`.

The line inside the body calls a function:

```kiru
fn main() {
  std::print("hello, world");
};
```

`std::print` writes a line to stdout. `std` is the standard library namespace,
which every program has; `::` separates the namespace from the function. The
argument is a string literal in double quotes.

## main's Signature

`main` takes no parameter or one parameter, and returns nothing or an `int`:

- with no parameter, the program reads no arguments;
- with one parameter, it must be declared `Vec<txt>` (reached with
  `use std::dynamic::Vec;`), and the runtime passes the words after the program
  name. [A First Program](/getting-started/a-first-program/) uses this form;
- a return type must be `int`, and the returned value becomes the program's exit
  status. [Panic and Failure](/failure/panic/) defines the status.

A project whose root namespace declares no `main`, a `main` with two
parameters, or a parameter or return type other than the above is a compile
error.

## The kc Compiler

`kc` compiles a project and has three commands:

```console
$ kc [kiru.toml] [-o <output>]
$ kc help
$ kc version
```

`kc` reads `./kiru.toml`; `kc path/to/kiru.toml` reads that manifest. It checks
the whole tree under `project.root`, finds the root namespace's `main`, and
writes a standalone executable at `project.output`, which defaults to the
manifest directory joined with `project.name` (itself defaulting to the
directory's name):

```console
$ kc
$ ./main
```

With `-o`, you choose the output path:

```console
$ kc -o app
$ ./app
```

The output is one file. It holds the compiler and your program, so you can copy,
rename, and ship it on its own. There is no linking step and no interpreter to
install. The compiler refuses to write over a `.kiru` file.

## What the Compiler Checks

A program that compiles has been checked for:

- syntax and lexical rules (an invalid escape, a missing `;`);
- namespaces (a `use` that reaches nothing, a `use` of a module value or a bound
  function, a declaration reached across a namespace without `pub`);
- names (an unknown name, a duplicate declaration in a namespace);
- arity (the wrong number of arguments);
- parameters (a parameter without a type, a `main` parameter that is not
  `Vec<txt>`);
- statements (a bare value used as a statement);
- types (text where an `int` is required, a call that returns no value bound or
  passed);
- returns (a function with a return type that can reach the end without
  returning a value);
- match arms (a pattern that is not a literal, a name, a field path, or a
  variant; two equal patterns);
- loops (`break` outside a loop);
- threads (an `async` nested inside another `async`).

A failed check prints the file, position, and message, and exits `1`.

Some failures can only happen while the program runs; [Panic and
Failure](/failure/panic/) covers them.
