---
title: Compiling and Running
description: The kc command, its output, and what the compiler checks.
---

The `kc` compiler has three commands:

```console
$ kc <path/to/entry> [-o <output>]
$ kc help
$ kc version
```

`kc <path>` checks the entry file and everything it imports, then writes a
standalone executable next to the entry path with the `.kiru` suffix removed:

```console
$ kc tools/release.kiru
$ ./tools/release
```

With `-o`, you choose the output path:

```console
$ kc main.kiru -o app
$ ./app
```

The output is one file. It holds the compiler and your program, so you can copy,
rename, and ship it on its own. There is no linking step and no interpreter to
install. The compiler refuses to overwrite the entry file.

## The Exit Status

A program exits with a status that other tools can read. By default that status
is `0`. If `main` declares `-> txt`, the text it returns becomes the status:

- no return means `0`;
- numeric text is that exit code, clamped to `0..=255`;
- non-numeric text is printed to stderr and exits `1`.

A `panic;` anywhere exits `1`. [Panic](/failure/panic/) covers failure.

```kiru
fn main() -> txt {
  return "2";
};
```

## What the Compiler Checks

A program that compiles has been checked for:

- syntax and lexical rules (an invalid escape, a missing `;`);
- modules and imports (a malformed `mod` block, a missing file, an import cycle,
  a file imported more than once);
- names (an unknown name, a forward reference, a duplicate declaration, a
  function that references itself);
- arity (the wrong number of arguments);
- parameters (a parameter without a type, `main` with a `txt` parameter);
- statements (a bare value used as a statement);
- types (text where a record is required, a call that returns no value bound or
  passed);
- returns (a function with a return type that can reach the end without
  returning a value);
- match arms (a pattern that is not a literal, a name, or a field path; two
  equal patterns);
- loops (`break` outside a loop);
- threads (an `async` nested inside another `async`).

A failed check prints the file, position, and message, and exits `1`.

## What Is Checked at Run Time

Some failures can only happen while the program runs. The runtime reports them
and exits `1`:

```text
a command that cannot start
too many concurrent commands
panic
```

[Panic](/failure/panic/) covers what happens then.
