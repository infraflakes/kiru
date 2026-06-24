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

With **kiru** you declare and clone multiple repos, write shell functions, and chain them into pipelines, all in one DSL.

Static validation catches invalid syntax before anything executes. 

All without even need to stay in the same directory as your repositories!

---

## Quick start

Get the binary via [Releases](https://github.com/infraflakes/kiru/releases) or this quick script:

```bash
# install
curl -sSf https://raw.githubusercontent.com/infraflakes/kiru/main/install.sh | sh
```

Config lives at `~/.config/kiru/main.kiru`. Override with `-c <path>`.

---

## The four things

| thing | what it is |
|---|---|
| **sanctuary** | the root directory where all your repos live |
| **pr** | a repo: url, local path, optional branch, sync mode |
| **fn** | a function with execution primitives `exec`, `cd`, `log`, `env`, `var`, `case` |
| **run** | an orchestration block — chains of fn calls, concurrent by default |

---
## Examples:

- An introduction to [kiru](./assets/introduction.kiru).
- Here's an [example](./assets/example.kiru) of what a `.kiru` file would look like.
- We also have [ebnf](./assets/kiru.ebnf) and our own kiru [files](./.kiru) (yes we are dogfooding):

---

## Commands

| command | what it does |
|---------|-------------|
| `kiru sync` | clone/update all declared repos into sanctuary |
| `kiru run <name> <project>` | execute a run block (interactive TUI) |
| `kiru fn <name> <project>` | execute one function (plain output) |
| `kiru validate` | parse and validate the config |
| `kiru version` | print version |

When `SANCTUARY=0`, kiru runs in standalone mode — no sanctuary, no projects, just top-level `fn` and `run` blocks. Config defaults to `.kiru/main.kiru`. Useful for CI/CD.

---

## Contributing

Contributions are welcome! Open issues or submit pull requests.

## License

[MIT](./LICENSE)
