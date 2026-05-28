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

**TUI** — interactive terminal UI during `run`. shows chain headers, live spinner per task, and status colors. press `q` or `Ctrl+C` to abort. after completion, a colored ANSI dump shows full output per task.

---

## Usage

```
kiru sync                       clone all declared repos into sanctuary
kiru run <name> <project>       run a run block (with TUI)
kiru fn <name> <project>        run a function directly (plain output)
kiru validate                   validate the configuration file
kiru version                    print version
kiru -c <path> <command>        use a custom config file
kiru --config <path> <command>  same as -c
```

Config is discovered at `~/.config/kiru/config.kiru` by default. Override with `-c`.

---

## Config

Everything lives in one `.kiru` file — project declarations, execution logic, variables. No separate config and script files.

```
shell = `bash`;

var shell workdir = `echo $HOME/dev`;
var string app    = `todo`;

sanctuary = $workdir;

pr todo {
    url  = `git@github.com:yourname/todo.git`;
    dir  = `todo`;
    sync = `clone`;
    branch = `main`;
    use = `.kiru/main.kiru`;

    var shell version = `git describe --tags --always --dirty 2>/dev/null || echo dev`;

    fn build {
        log `Building ${app} at ${version}`;
        var shell os = `uname -s`;
        case $os {
            `Linux`  {
                cd(`cmd`);
                exec(`go build -ldflags='${version}' -o bin/${app} .`);
            };
            `Darwin` {
                cd(`cmd`);
                exec(`go build -ldflags='${version}' -o bin/${app} .`);
            };
            _        { log `unsupported OS: ${os}`; };
        };
    }

    fn test {
        env [
            CGO_ENABLED = `0`,
            GOPATH = `$HOME/go`;
        ] {
            exec(`go test -race ./...`);
            exec(`go vet ./...`);
        };
    }

    run ci {
        test => build;
    }
}
```

---

## DSL reference

### Declarations

| declaration | description |
|-------------|-------------|
| `shell = \`...\`;` | required, must be first. declares the shell for `exec` and `var shell` |
| `sanctuary = \`...\` \| $var;` | required. absolute path to workspace root |
| `import ./path;` | import other `.kiru` files, relative paths only |
| `var string name = \`...\` \| $var;` | string variable (global or project-scoped) |
| `var shell name = \`...\`;` | runs content via declared shell, stores stdout |
| `pr name { ... }` | project declaration |
| `fn name { ... }` | execution block |
| `run name { ... }` | orchestration block with chain syntax |

### Variable scope

- **Global vars** (declared at the top level) — accessible everywhere: global scope, project fields, project vars, function bodies.
- **Project vars** (declared inside `pr { }`) — accessible only within that project's function bodies.
- **Fn-local vars** (declared inside `fn { }` or `env { }`) — scoped to that block, shadow any outer var with the same name.

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
| `var <type> name = ...;` | fn-local variable. shadows outer var with same name |
| `case <expr> { ... };` | conditional branching. first-matching arm wins |

### case statement

```
case <expr> {
    `literal`    { ... };
    $var_ref     { ... };
    _            { ... };   # default / catch-all
};
```

- Condition is an expression (backtick or `$var` reference)
- Patterns support `${interpolation}` inside backticks
- First matching arm wins; execution continues after the `case` block
- Each arm body ends with `;`; the entire `case` block ends with `;`

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

### Delimiter rules

| delimiter | job |
|-----------|-----|
| `()` | primitive args — `exec()`, `cd()`, `log()` |
| `[]` | typed list — `env[]` |
| `{}` | statement block |
| `;` | statement terminator inside `{}` and run chain separator |
| `,` | item separator inside `[]` |
| `=>` | sequential chain separator inside run blocks |

### TUI controls

During `kiru run`, an interactive TUI shows live progress:
- **Spinner** animates on running tasks
- **Status colors**: green (ok), red (failed), yellow (running), gray (pending/skipped)
- Press **`q`** or **`Ctrl+C`** to abort
- After completion, a colored ANSI summary dump is printed

### Rules

- `shell` must be declared before any `exec`, `var shell`, or `fn`.
- `sanctuary` must be declared before any `pr`.
- Variables must be declared before they are referenced.
- `cd` cannot escape the project directory. hard fail at runtime.
- Circular imports fail at parse time.
- Two projects cannot share the same `dir`. parse error.
- `env` blocks save and restore variable scope — declarations inside are local.

---

## Per-project config

If a project declares `use`, that file is parsed after `kiru sync` clones the repo. It can define fns scoped to that project. It cannot declare `sanctuary`, `pr`, or `shell`.

```
# calendar/.kiru/main.kiru

fn build {
    exec(`pnpm build`);
}

fn dev {
    exec(`pnpm dev`);
}
```

---

## Contributing

Contributions are welcome! Feel free to open issues or submit pull requests.

## License

[MIT](./LICENSE)
