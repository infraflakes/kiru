---
title: Hello, World!
description: Write, compile, and run your first Kiru program.
---

Now that you've installed Kiru, it's time to write your first program. It's
traditional to start with a program that prints `Hello, world!` to the screen,
so we'll do the same.

## Writing the Program

Make a directory for your projects and one for this program:

```console
$ mkdir ~/projects
$ cd ~/projects
$ mkdir hello
$ cd hello
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
$ kc main.kiru
$ ./main
hello, world
```

If you see `hello, world`, you've written and run a Kiru program. If not, check
the [Installation](/getting-started/installation/) chapter.

## The Anatomy of a Kiru Program

Let's look at the program piece by piece. First, the outer part:

```kiru
fn main() {
};
```

This declares a function named `main`. Every compiled Kiru program starts at the
`main` in its entry file, so this is where execution begins. The body is wrapped
in `{}`, and the whole declaration ends with a `;`.

Next, the line inside the body:

```kiru
  std::print("hello, world");
```

This calls `std::print`, which writes a line to stdout. `std` is the standard
library namespace, which every program has; `::` separates the namespace from
the function. The argument is a string literal in double quotes.

## main's Signature

`main` may take no parameter or one parameter. The one-parameter form receives
the command line; [A First Program](/getting-started/a-first-program/) shows it.
`main` may also declare a return type of `-> txt`, which becomes the program's
exit status; [Compiling and Running](/getting-started/compiling-and-running/)
covers that.

An entry file with no `main`, or a `main` with two parameters, is a compile
error.
