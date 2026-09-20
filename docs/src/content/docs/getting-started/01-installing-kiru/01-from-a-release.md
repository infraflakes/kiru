---
title: From a Release
description: Install a prebuilt kc binary from a GitHub release.
---

The install script downloads the latest release binary for Linux and places
it in `$HOME/.local/bin`:

```console
$ curl -sSf https://raw.githubusercontent.com/infraflakes/kiru/main/install.sh | sh
Fetching kc v0.4.0...
Installed kc v0.4.0 to /home/you/.local/bin/kc
```

With `$HOME/.local/bin` on `PATH`, the install is ready to use:

```console
$ kc version
kc 0.4.0
```

A release binary holds the compiler and the runtime in one file. It
compiles programs, and a compiled program is a copy of the same binary with
the program appended;
[compiling](/getting-started/02-compiling-and-running/01-compiling/)
describes that layout.

:::note
`kc` targets Linux. Every command runs in its own process group and, on
Linux, receives a parent-death signal, which is behavior the runtime relies
on. Other platforms are not supported yet.
:::

## Updating

Re-run the install script; it always fetches the latest release:

```console
$ curl -sSf https://raw.githubusercontent.com/infraflakes/kiru/main/install.sh | sh
```
