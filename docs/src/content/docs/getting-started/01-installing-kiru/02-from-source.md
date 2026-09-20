---
title: From Source
description: Build Kiru from the source tree with cargo.
---

Building from source needs a Rust toolchain (edition 2024, so a recent
stable or newer). The source tree ships a Nix flake that pins every tool:

```console
$ git clone https://github.com/infraflakes/kiru
$ cd kiru
$ nix develop
$ cargo build --release
```

```console
$ ./target/release/kc version
kc 0.4.0
```
