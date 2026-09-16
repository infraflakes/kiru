<h1 align="center">kiru</h1>
<p align="center">
    <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/License-MIT-yellow.svg"></a>
    <a href="https://github.com/infraflakes/kiru/releases"><img alt="GitHub Release" src="https://img.shields.io/github/v/release/infraflakes/kiru?logo=github"></a>
</p>

<img src="./assets/kiru.png" alt="TUI" width="600">

---

> [!CAUTION]
> `kiru` is still in early development, breaking changes may happen.
>
> Many docs are temporarily LLM generated for now.

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

**`main.kiru`** - the work. Projects (`project`), their functions (`fn`), and the pipelines (`run`) that call them. This one travels well, so keep it under version control.

```kiru
var app = (todo);

project todo {
  fn build {
    log(Building @(app)...);
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

## The language in one paragraph

Every statement is a call, and the keyword and its call parens must be adjacent (`log(x)`, never `log (x)`); whitespace between other tokens is free. Three template forms exist: `()` is literal text, `$(command)` runs a command, and `@(name)` interpolates a variable. The built-in primitives (`log`, `cd`, `env`, `switch`, `case`, `default`, and the declaration keywords) are reserved - they cannot be used as identifiers or function names, which is what keeps `bar()` unambiguous against them.

A `fn` at the top level (outside any project) is a global function: a reusable template that is never run directly. There are two ways to reference it. Inside a project body, `name();` binds it into the project as a function of that name, resolved against the project's vars, so a run block can reach it as `project::name`. Inside any function body, `name();` splices the body right there, carbon-copy. Resolution follows the including project: `@(app)` inside the template finds the project's `app` first, then a global var, then fails; calls inside the template resolve the same way, so a project function shadows a global function of the same name. Calls take no arguments - everything the callee needs comes from the scope it is expanded into - and recursive calls are a compile error.

**`kiru.toml`** - your machine. Which shell to use, an optional command timeout, and which repos kiru should clone for you. Projects are keyed by project name, matching `project <name>` in the DSL.

```toml
shell = "sh"
timeout = 300           # optional, seconds per command

[project.todo]
url     = "git@github.com:you/todo.git"
dir     = "~/projects/todo"
direnv  = true
```

Set `direnv = true` on a project entry to run that project's commands through `direnv exec`. Before a function of the project runs, kiru calls `direnv allow` on the repo directory for you, so the environment always loads. Everything else is direnv's business: a missing binary, a failing `.envrc`, or a directory without one fails the command with direnv's own error. Projects without the flag run their commands plain.

## Compile and run

kiru does not read `main.kiru` directly. First, compile it into the `kirufile` that the rest of the commands use:

```bash
kiru compile -c main.kiru -o ~/.config/kiru
```

Compile parses and checks `main.kiru`, then writes `kirufile` into the output directory. If it reports errors, fix them and compile again. Every edit to `main.kiru` needs a fresh `kiru compile` before the other commands see it.

| command | what it does |
|---------|-------------|
| `kiru compile -c main.kiru -o DIR` | turn a `main.kiru` into the `kirufile` |
| `kiru status` | show the kiru.toml config and the compiled run blocks |
| `kiru run ci` | run the `ci` pipeline |
| `kiru sync` | clone or update the repos in `kiru.toml` |
| `kiru version` | print the version |

Start with `kiru status`. It never runs anything, just tells you whether your config is sound.

`kiru run` works without a `kiru.toml` too: commands then run in the directory you invoke kiru from, with the default shell. The one command that requires the toml is `kiru sync` - it has nothing to do without repos to clone.

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
