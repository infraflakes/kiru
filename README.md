<h1 align="center">kiru</h1>
<p align="center">A statically validated DSL and CLI for multiple git projects orchestration.</p>
<p align="center">
    <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/License-MIT-yellow.svg"></a>
    <a href="https://github.com/infraflakes/kiru/releases"><img alt="GitHub Release" src="https://img.shields.io/github/v/release/infraflakes/kiru?logo=github"></a>
</p>

<img src="./assets/kiru.png" alt="TUI" width="600">

---

> [!CAUTION]
> `Kiru` is still in early development, breaking changes might happen!

With **kiru** you declare multiple git repos, write shell functions, and chain them into concurrent pipelines — all in one DSL.

Static validation catches invalid syntax, undefined variables, and broken function references before anything executes.

---

## Quick start

Get the binary via [Releases](https://github.com/infraflakes/kiru/releases) or this quick script:

```bash
# install
curl -sSf https://raw.githubusercontent.com/infraflakes/kiru/main/install.sh | sh
```

Config lives at `~/.config/kiru/main.kiru`. Override with `-c <path>`.

---

## DSL overview

| construct | what it does |
|---|---|
| **var** | declare a global or project-scoped variable (`string` or `shell`) |
| **import** | split config across multiple `.kiru` files |
| **pr** | declare a git repo with metadata fields — `url`, `dir`, `sync`, `branch` |
| **fn** | a function with execution primitives `log`, `exec`, `cd`, `var`, `env`, `case` |
| **run** | an orchestration block — chains of concurrent and sequential function calls |

---

## Examples

- [Introduction to kiru](./assets/introduction.kiru) — walks through every DSL feature.
- [Example](./assets/example.kiru) — a compact `.kiru` file.
- [EBNF grammar](./assets/kiru.ebnf) — the formal DSL specification.
- We use kiru to build kiru — see our own [.kiru/](./.kiru) config.

---

## Commands

| command | what it does |
|---------|-------------|
| `kiru sync` | clone / update all declared repos |
| `kiru run <name> <project>` | execute a run block |
| `kiru fn <name> <project>` | execute one function |
| `kiru validate` | parse, resolve, and validate the config |
| `kiru version` | print version |

### Environment

`KIRU_CWD=1` — run `fn` and `run` commands in the current working directory instead of depending on `dir` field in `pr`. Useful for CI/CD pipelines where you're already in the right directory.

---

## Contributing

Contributions are welcome! Open issues or submit pull requests.

## License

[MIT](./LICENSE)
