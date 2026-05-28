<h1 align="center">kiru</h1>
<p align="center">A statically validated DSL and CLI for multiple git projects orchestration.</p>
<p align="center">
    <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/License-MIT-yellow.svg"></a>
    <a href="https://github.com/infraflakes/kiru/releases"><img alt="GitHub Release" src="https://img.shields.io/github/v/release/infraflakes/kiru?logo=github"></a>
</p>

<img src="./examples/tui_example.png" alt="TUI" width="600">

---

## The problem

You switch machines. You have fifteen repos. You have a different setup script per project, a makefile that forgets `cd` between lines, env vars leaking everywhere, and a shell script graveyard nobody trusts.

`kiru` fixes this with a single config file, a small DSL, and zero shell archaeology.

---

## Concepts

**`sanctuary`** — your master workspace directory. all repos live relative to it.

**`pr`** — a project declaration. url, local dir, sync behavior, optional per-project config.

**`fn`** — a named execution block scoped to a project directory. primitives never leak between blocks.

**`run`** — orchestration block. concurrent chains of sequential function calls. each chain runs in order; chains run concurrently.

---

## Usage

```
kiru sync                       clone all declared repos into sanctuary
kiru run <name> <project>       run a run block
kiru fn <name> <project>        run a function directly
kiru -c <path> <command>        use a custom config file
kiru --config <path> <command>  same as -c
```

Config is discovered at `~/.config/kiru/config.kiru` by default. Override with `-c`.

---

## Config

Everything lives in one `.kiru` file — project declarations, execution logic, variables. No separate config and script files.

For example:

```
shell = `bash`; # this argument is mandatory if you want to use shell related features! You can choose whatever shell you like.

var shell workdir = `echo $HOME/dev`; # Define a shell variable which returns value
var string app    = `todo`; # Define a string variable

sanctuary = $workdir; # Referencing variable with `$`

pr todo { # project field to define project properties
    url  = `git@github.com:yourname/todo.git`;
    dir  = `todo`; # project will be cloned on path relative to sanctuary, in this case `$HOME/dev/todo`.
    sync = `clone`; # affects how `sync` works, `clone` will clone the project, `ignore` will neglect.
}

pr calendar {
    url  = `git@github.com:yourname/calendar.git`;
    dir  = `calendar`;
    sync = `clone`;
    use  = `.kiru/main.kiru`;
}

fn build {
    var shell version = `git describe --tags --always --dirty 2>/dev/null || echo dev`;
    cd(`cmd`); # `cd()` always map to sanctuary/project_dir, meaning `cd(`.`)` will always return to project's root.
    env [CGO_ENABLED = `0`, GOOS = `linux`] {
        exec(`go build -ldflags='-X main.version=${version}' -o bin/${app} .`);
    };
}

fn test {
    env [CGO_ENABLED = `0`] {
        exec(`go test -race ./...`);
        exec(`go vet ./...`);
    };
}

run ci {
    test(todo);
    build(todo);
    build(calendar);
}
```

---

## DSL reference

### Declarations

| declaration | description |
|-------------|-------------|
| `shell = \`...\`;` | required, must be first. declares the shell for `exec` and `var shell` |
| `sanctuary = \`...\` \| $var;` | required. absolute path to workspace root |
| `var string name = \`...\` \| $var;` | string variable, global or fn-scoped |
| `var shell name = \`...\`;` | runs content via declared shell, stores stdout |
| `import ./path;` | import other `.kiru` files, relative paths only |
| `pr name { ... }` | project declaration |
| `fn name { ... }` | execution block |
| `run name { ... }` | orchestration block with chain syntax |

### Project fields

| field | required | description |
|-------|----------|-------------|
| `url` | yes | git clone url |
| `dir` | yes | directory name relative to sanctuary, must be unique |
| `sync` | no | `clone` (default) — skip if exists. `ignore` — skip entirely |
| `use` | no | path to a `.kiru` file inside the project, relative to project dir |
| `branch` | no | branch to clone. defaults to repo default branch |

### fn primitives

| primitive | description |
|-----------|-------------|
| `exec(\`...\`);` | run command via declared shell. non-zero exit fails the block |
| `cd(\`...\`);` | change cwd relative to project dir. cannot escape project dir |
| `log(\`...\`);` | print to TUI output. never fails |
| `env [...] { };` | scoped env vars. inner `env` overrides outer. no leakage |
| `var <type> name = ...;` | fn-local variable. shadows global with same name |

### run block

| syntax | description |
|--------|-------------|
| `fn_name;` | single function call as a concurrent chain |
| `fn_a => fn_b => fn_c;` | sequential chain: fn_a → fn_b → fn_c in order |
| `fn_a; fn_b => fn_c;` | two concurrent chains (first: fn_a alone, second: fn_b → fn_c) |

Chains are separated by `;`. Each chain runs sequentially; chains run concurrently. If a function in a chain fails, the rest of that chain is skipped but other chains continue.

### Types and values

| syntax | type | notes |
|--------|------|-------|
| `` `...` `` | string | use `${name}` to interpolate variables |
| `$name` | var ref | standalone reference, outside backticks |
| integer | number | whole numbers only |

### Delimiter rules

| delimiter | job |
|-----------|-----|
| `()` | primitive args — `exec()`, `cd()`, `log()` |
| `[]` | typed list — `env[]` |
| `{}` | statement block |
| `;` | statement terminator inside `{}` and run chain separator |
| `,` | item separator inside `[]` |
| `=>` | sequential chain separator inside run blocks |

### Rules

- `shell` must be declared before any `exec`, `var shell`, or `fn`.
- `sanctuary` must be declared before any `pr`.
- variables must be declared before they are referenced.
- `cd` cannot escape the project directory. hard fail at runtime.
- `override fn` is required to redefine a global `fn` in a `use` file. silent redeclaration is a parse error.
- circular imports fail at parse time.
- two projects cannot share the same `dir`. parse error.

---

## Per-project config

If a project declares `use`, that file is parsed after `kiru sync` clones the repo. It can define or `override` global fns. It cannot declare `sanctuary`, `pr`, or `shell`.

```
# calendar/.kiru/main.kiru

override fn build {
    exec(`pnpm build`);
}

fn dev {
    exec(`pnpm dev`);
}
```

---

## TUI

`kiru` renders a live accordion TUI during execution. each task has a colored left bar indicating status, expandable stdout, and pruned history for long output.

```
run  ci                                                3 tasks

 ✓  build(todo)                                          ▶
 ⠋  build(calendar)                                      ▼
    log  Building project...
    var  version = 3a1b2c4
    env  CGO_ENABLED=0 GOOS=linux
    exec go build -ldflags='-X main.version=3a1b2c4' -o bin/todo .
    ⠋

 ✓ 2 ok  ⠋ 1 running
```

---

## Contributing

Contributions are welcome! Feel free to open issues or submit pull requests.

## License

[MIT](./LICENSE)
