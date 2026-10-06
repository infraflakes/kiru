---
title: Running Commands
description: command, spawn, wait_for, and capture.
---

Running other programs is what Kiru is for. There are two ways to do it:
through the shell, with `command`, and directly with an argv, using `spawn` and
`wait_for`. In this section we'll use both.

`std::process::command(spec, line)` runs one command line through the POSIX
shell (`/bin/sh -c`) and returns the exit code as text. An empty spec is the
common case:

```kiru
txt code = std::process::command({}, "make");
switch(code) {
  case("0") {};
  default { std::io::print("make failed"); };
};
```

[The Command Spec and Streams](/commands/the-command-spec/) covers the
spec record. When a shell is not wanted, `spawn` runs an argv directly and
returns a pid, and `wait_for` returns its exit code:

```kiru
txt pid = std::process::spawn({}, ["make", "-j4"]);
txt code = std::process::wait_for(pid);
```

`std::process::capture(line)` runs one line and returns `{ code, out, err }`.
It writes stdout and stderr to temporary files and reads them back, so a large
output never blocks on a pipe:

```kiru
rec result = std::process::capture("git rev-parse HEAD");
std::io::print(result.out);
```

`command` is the one way to run a shell line; `spawn` and `wait_for` are the
argv-level primitives underneath it.
