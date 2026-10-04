---
title: From a Release
description: Install a prebuilt kc binary from a GitHub release.
---

The install script fetches the latest Linux release and places it in
`$HOME/.local/bin`:

```console
$ curl -sSf https://raw.githubusercontent.com/infraflakes/kiru/main/install.sh | sh
Fetching kc v0.4.0...
Installed kc v0.4.0 to /home/you/.local/bin/kc
```

With that directory on `PATH`:

```console
$ kc version
kc 0.4.0
```

Re-run the install script to update. The binary is self-contained;
[compiling](/getting-started/02-compiling-and-running/01-compiling/) describes
its layout.

:::note
`kc` targets Linux. The runtime relies on each command running in its own
process group and receiving a parent-death signal; [process
groups](/effects/06-commands/05-process-groups/) describes that guarantee.
:::
