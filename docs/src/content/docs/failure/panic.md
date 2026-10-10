---
title: Panic and Failure
description: panic, std::eprint, runtime errors, and exit status.
---

A program can fail on purpose. In this section we'll see how, and what happens
when it does.

`panic;` marks the program as failed and exits `1`:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn give_up() {
  panic;
};
```

`panic;` is a statement, not a value. It stops the whole program: it cancels
every other thread and sends `SIGTERM` to every running process group, so no
command a Kiru program started outlives the failure.

`std::eprint(message)` writes an `ERROR:` line to stderr, red when stderr
is a terminal, then panics:

```kiru
fn fail(reason<txt>) {
  std::eprint("cannot continue: " + reason);
};
```

```console
$ ./app
ERROR: cannot continue: missing token
$ echo $?
1
```

A function whose every path ends in `panic;` or a call that panics never
returns, so it can stand where a value is expected:

```kiru
fn fail(reason<txt>) -> txt {
  std::eprint(reason);
};
```

## Runtime errors

The runtime detects some failures the compiler cannot, and reports them the
same way: an `ERROR:` line naming the call, then the run stops.

```text
ERROR: std::get(...): index 2 is outside a list of 2 element(s)
ERROR: std::command(...): a command that cannot start
ERROR: std::spawn(...): too many concurrent commands
```

## Exit status

A normal run exits with the status `main` returns: no return means `0`, numeric
text is that exit code, and non-numeric text is printed to stderr and exits
`1`. [The Entry File and main](/getting-started/compiling-and-running/) covers it.

A command's exit code is ordinary data. An unobserved code fails nothing; a
program decides when a nonzero code means failure:

```kiru
let code<txt> = std::command({}, "false");
match code {
  "0" => {};
  _ => { std::eprint("command failed"); };
};
```

[Signals and Shutdown](/failure/signals/) covers Ctrl+C and the
other signals that exit `130`.
