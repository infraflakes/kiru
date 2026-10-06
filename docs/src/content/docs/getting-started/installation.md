---
title: Installation
description: Get the kc compiler from a release or build it from source.
---

The first step is to install Kiru, which gives you the `kc` compiler. You can
download a release or build from source.

## From a Release

The install script fetches the latest Linux release and puts `kc` in
`$HOME/.local/bin`:

```console
$ curl -sSf https://raw.githubusercontent.com/infraflakes/kiru/main/install.sh | sh
Fetching kc v0.4.0...
Installed kc v0.4.0 to /home/you/.local/bin/kc
```

Make sure that directory is on your `PATH`, then check the version:

```console
$ kc version
kc 0.4.0
```

Re-run the script to update.

## From Source

Building needs a Rust toolchain (edition 2024, so a recent stable or newer). The
flake in the repository pins one:

```console
$ git clone https://github.com/infraflakes/kiru
$ cd kiru
$ nix develop
$ cargo build --release
$ ./target/release/kc version
kc 0.4.0
```

:::note
`kc` targets Linux. It runs each command in its own process group with a
parent-death signal; [Process Groups](/commands/process-groups/) explains why.
:::
