<h1 align="center">kiru</h1>
<p align="center">
    <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/License-MIT-yellow.svg"></a>
    <a href="https://github.com/infraflakes/kiru/releases"><img alt="GitHub Release" src="https://img.shields.io/github/v/release/infraflakes/kiru?logo=github"></a>
</p>

<img src="./assets/kiru.png" alt="TUI" width="600">

---

> [!CAUTION]
> `kiru` is still in early development, breaking changes may happen.

kiru is a small tool that keeps several git repos in sync and runs jobs across them. You describe the work once, in one file, and kiru runs it for you.

## What you get

- One DSL for the repos you work with and the shell steps that build, test, and release them.
- Pipelines that run steps in parallel or one after another, across repos.
- Validation up front: kiru checks your file before it runs anything, so mistakes show up while you edit, not mid-deploy.

## Install

```bash
curl -sSf https://raw.githubusercontent.com/infraflakes/kiru/main/install.sh | sh
```

This puts `kiru` in `~/.local/bin`. Make sure that directory is on your `PATH`.

## Your two files

Everything lives in `~/.config/kiru/`, in two files you write.

**`main.kiru`** - the work. Projects (`pr`), their functions (`fn`), and the pipelines (`run`) that call them. This one travels well, so keep it under version control.

```kiru
var app = (todo);

pr todo {
  fn build {
    log (Building @(app)...);
    $(go build -o bin/@(app) .);
  };

  fn test {
    $(go test -race ./...);
  };
};

run ci {
  todo::test => todo::build;
};
```

**`kiru.toml`** - your machine. Which shell to use and which repos kiru should clone for you.

```toml
shell = "sh"

[[repos]]
name = "todo"
url  = "git@github.com:you/todo.git"
dir  = "~/projects/todo"
```

Set `direnv = true` to run commands through `direnv exec` when the project's `.envrc` is allowed (off by default; requires the `direnv` binary).

## Compile and run

kiru does not read `main.kiru` directly. First, compile it into the `kirufile` that the rest of the commands use:

```bash
kiru compile -c main.kiru -o ~/.config/kiru
```

Compile parses and checks `main.kiru`, then writes `kirufile` into the output directory. If it reports errors, fix them and compile again. Every edit to `main.kiru` needs a fresh `kiru compile` before the other commands see it.

| command | what it does |
|---------|-------------|
| `kiru compile -c main.kiru -o DIR` | turn a `main.kiru` into the `kirufile` |
| `kiru status` | validate the `kirufile` and show what's inside |
| `kiru run ci` | run the `ci` pipeline |
| `kiru sync` | clone or update the repos in `kiru.toml` |
| `kiru version` | print the version |

Start with `kiru status`. It never runs anything, just tells you whether your config is sound.

Flags follow one rule: `-c` points at a config, `-p` at a `kirufile`. Only the flags a command actually needs exist. Defaults are `~/.config/kiru/kiru.toml` for `-c` and `~/.config/kiru/kirufile` for `-p`; `compile -c` defaults to `~/.config/kiru/main.kiru`.

## Learn the DSL

- [Introduction to kiru](./assets/introduction.kiru) - the language, feature by feature.
- [Minimal example](./assets/example.kiru) - the smallest setup that runs.
- [Grammar](./assets/kiru.ebnf) - the formal spec.
- [main.kiru](./main.kiru) - kiru's own config, used to build and test itself.

---

## Contributing

Bug reports, feature ideas, and pull requests are all welcome.

## License

[MIT](./LICENSE)
