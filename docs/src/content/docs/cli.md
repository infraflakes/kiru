---
title: CLI
description: Every command, flag, and exit code.
---

`kiru` reads one configuration file and works with one profile per invocation.
The profile determines the source file, the compiled program, the shell, and
the projects; the commands below determine what happens to them.

## Commands

| Command | What it does |
| --- | --- |
| `kiru compile` | Compiles the profile's `source` into the program at `output`. |
| `kiru run <name>` | Executes the run block `name`. The program must be compiled first. |
| `kiru status` | Shows the profile, its projects, and its run names. Never executes anything. |
| `kiru sync` | Clones or fast-forward-pulls the profile's projects. |
| `kiru version` | Prints the version. |

`compile` parses and checks `source`, creates missing parent directories of
`output`, and writes the program there. kiru never reads the `.kiru` source
directly: every edit needs a fresh `kiru compile` before the other commands see
it, and nothing compiles implicitly. If it reports errors, fix them and compile
again.

`status` is the read-only view: it prints the profile (source, output, shell,
timeout), the projects, and, when a compiled program exists, the names of the
run blocks inside it. A typo in `kiru.toml` shows up here before it shows up in
a run.

## Flags

| Flag | Meaning |
| --- | --- |
| `-c`, `--config <path>` | The `kiru.toml` to read. Defaults to `~/.config/kiru/kiru.toml`. |
| `-p`, `--profile <name>` | Required for every command except `version`. An unknown profile is an error that lists the available ones. |

## Compiling and Running

The compiled program is one portable file. It contains the whole structure of
the work - nodes, run roots, and the variable table - and no machine state, so
the same artifact can be archived, inspected, or produced on one machine and
run on another.

```console
$ kiru compile -c kiru.toml -p ci
$ kiru status -c kiru.toml -p ci
$ kiru run build -c kiru.toml -p ci
```

[Profiles](../configuration/profiles/) and [Projects and
Direnv](../configuration/projects/) describe the configuration these commands
read; [Syncing](../configuration/syncing/) describes what `sync` does with the
project entries.

## Exit Codes

| Code | Meaning |
| --- | --- |
| `0` | Everything succeeded. |
| `1` | A task failed, or an infrastructure error was reported. |
| `130` | Cancelled: Ctrl+C, `q`, or a termination signal. Running commands are stopped first. |

[Failure and Cancellation](../runs/failure-and-cancellation/) explains the rule
behind the codes.
