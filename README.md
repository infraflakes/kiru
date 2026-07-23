<h1 align="center">kiru</h1>
<p align="center">Infrastructure as Code meets local task runner.</p>
<p align="center">
    <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/License-MIT-yellow.svg"></a>
    <a href="https://github.com/infraflakes/kiru/releases"><img alt="GitHub Release" src="https://img.shields.io/github/v/release/infraflakes/kiru?logo=github"></a>
</p>

<img src="./assets/kiru.png" alt="TUI" width="600">

---

> [!CAUTION]
> `kiru` is still in early development, breaking changes may happen.

With kiru you declare multiple git repos, write shell functions once, and wire them into concurrent pipelines — all in one DSL.

## Why kiru?

Keeping several git repositories in sync usually means a folder of brittle shell scripts or a separate Makefile per project. Scattered, hard to read, and easy to break in ways you only notice once something is already running.

kiru gives you one small DSL to declare your repos, write the shell commands you already know as reusable functions, and wire them into concurrent or sequential pipelines.

The part we care about most: **kiru validates your config before running anything**. It catches invalid syntax, undefined variables, and broken function references at config-check time — so you spot mistakes while editing, not halfway through a deploy.

---

## Quick start

```bash
curl -sSf https://raw.githubusercontent.com/infraflakes/kiru/main/install.sh | sh
```

The default config is at `~/.config/kiru/main.kiru`. Override with `-c <path>` or set `KIRU_CWD=1` to use `./main.kiru` instead.

A `.kiru` file reads like the shell you already write:

```kiru
var string app = `todo`;
var shell  os  = `uname -s`;

fn build {
    log `Building ${global::app} on ${global::os}...`;
    case $global::os {
        `Linux`   { exec `go build -o bin/${global::app} .`; };
        `Darwin`  { exec `go build -o bin/${global::app} .`; };
        _         { log `unsupported OS: ${global::os}`; };
    };
}

fn test {
    exec `go test -race ./...`;
}

pr todo [
    url  = `git@github.com:yourname/todo.git`
    dir  = `todo`
    sync = `clone`
] {
    var string version = `dev`;
    use build;
    use test;
}

run ci {
    todo::test => todo::build;
}
```

| command | what it does |
|---------|-------------|
| `kiru status` | check the config is valid and show everything kiru found |
| `kiru sync` | clone or update all declared repos |
| `kiru run ci` | execute the `ci` pipeline (sequentially chains functions) |
| `kiru fn build todo` | run a single function in a project directly |

---

## How the DSL works

### Variables: `string` vs `shell`

| form | what it does |
|------|-------------|
| `var string name = \`value\`;` | store a string as-is; `\${...}` gets substituted |
| `var shell name = \`cmd\`;` | **run the command now** at config-check time and store its output |

A `var shell` command runs where it's declared:
- **Global scope** — runs in the current directory
- **Inside a project** — runs in that project's directory
- **Inside a function** — runs in the host project's directory

Non-zero exit is not an error — `var shell` returns `""` if the command fails. This makes it useful as a probe (e.g. `` var shell has_feature = `test -f x && echo yes` ``).

Set `KIRU_CWD=1` to force all `var shell` and runtime `exec` commands to run in the current directory instead of the project directory.

### Referencing variables: namespaces are mandatory

Every variable reference must be namespaced with `namespace::name`:

| reference | what it targets |
|-----------|----------------|
| `$global::app` or `\${global::app}` | a global variable |
| `$self::version` or `\${self::version}` | the **current project's** variable (rewritten to the project name at config-check time) |

Inside a function body, use `$self::name` to refer to a variable of whatever project eventually hosts the function. When the function is applied to a project via `use`, `self::` gets rewritten to that project's name.

A project can only read its own variables and globals. Reading another project's variables is a compile-time error.

### Functions are templates, not methods

Functions are defined at the **top level**, not inside projects:

```kiru
fn build {
    log `Building ${self::name}...`;
    exec `make build`;
}
```

They are **applied** to a project with `use`:

```kiru
pr myapp [url = `...` dir = `...`] {
    var string name = `myapp`;
    use build;         # becomes myapp::build
}
```

You can rename a function with `as`:

```kiru
pr backend [url = `...` dir = `...`] {
    var string name = `backend`;
    use build as compile;   # becomes backend::compile
}
```

### Function body: what you can write

| statement | what it does |
|-----------|-------------|
| `log \`msg\`;` | print a message |
| `exec \`cmd\`;` | run a shell command (stderr + stdout merged, live output) |
| `cd \`path\`;` | change directory (relative paths join, persists for later statements) |
| `var string x = \`v\`;` | scoped variable (visible to all functions in the project) |
| `var shell x = \`cmd\`;` | scoped variable that runs a shell command at config-check time |
| `env [KEY = \`val\` ...] { ... };` | scoped environment variables for the block body |
| `case $cond { ... };` | branching: `\`literal\``, `$var`, or `_` default; first match wins |

### Run blocks: orchestration

```kiru
run all {
    myapp::lint => myapp::check => myapp::build;  # sequential chain
    myapp::test;                                    # parallel chain
}
```

- **`;`** separates chains that run in parallel
- **`=>`** chains functions within a chain that run sequentially
- All chains start at once; if one function fails, its chain stops but others continue

### Projects: declaring repos

```kiru
pr name [
    url    = `git@github.com:user/repo.git`
    dir    = `./project`
    sync   = `clone`       # "clone" (default) or "ignore"
    branch = `main`         # optional
] {
    var string x = `...`;
    use build;              # apply a global function
}
```

Fields are space-separated inside brackets. Relative `dir` paths resolve against the config file's directory. If `sync` is `ignore`, the repo won't be cloned or updated.

---

## Examples

- [Introduction to kiru](./assets/introduction.kiru) — walks through every DSL feature step by step.
- [dots.kiru](./dots.kiru) — a real-world config managing 4 projects with shared functions.

---

## Environment

`KIRU_CWD=1` — run everything in the current directory (config resolves to `./main.kiru`, project `var shell` and `exec` commands ignore the project's `dir` field). Useful for CI/CD.

---

## Contributing

Bug reports, feature ideas, and pull requests are all welcome.

## License

[MIT](./LICENSE)
