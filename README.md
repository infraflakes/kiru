<h1 align="center">kiru</h1>
<p align="center">A statically validated DSL and CLI for multiple git projects orchestration.</p>
<p align="center">
    <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/License-MIT-yellow.svg"></a>
    <a href="https://github.com/infraflakes/kiru/releases"><img alt="GitHub Release" src="https://img.shields.io/github/v/release/infraflakes/kiru?logo=github"></a>
</p>

<img src="./examples/kiru.png" alt="TUI" width="600">

---

Every repo has a build script, a test script, a lint script. Some are makefiles, some are shell scripts, some are npm scripts. They're all slightly different. When you have more than a few repos, the friction adds up: different incantations for the same operations, env vars that need setting, cwd that needs changing, and no single place to see what happened when something fails.

With **kiru** you declare repos, write shell functions, and chain them into pipelines — all in one DSL. `kiru sync` clones everything. `kiru run ci myproject` runs the pipeline. Static validation catches undefined variables and missing functions before anything executes.

---

## Quick start

Get the binary via [Releases](https://github.com/infraflakes/kiru/releases) or this quick script:

```bash
# install
curl -sSf https://raw.githubusercontent.com/infraflakes/kiru/main/install.sh | sh
```

Config lives at `~/.config/kiru/main.kiru`. Override with `-c <path>`.

---

## The five things

| thing | what it is |
|---|---|
| **sanctuary** | the root directory where all your repos live |
| **pr** | a repo: url, local path, optional branch, sync mode |
| **fn** | a function with `exec`, `cd`, `log`, `env`, `var`, `case` |
| **run** | an orchestration block — chains of fn calls, concurrent by default |

---

## A real config

This is the whole thing — no separate files for config, scripts, and orchestration:

<pre>
var shell workdir = `echo $HOME/dev`;
var string app    = `todo`;

sanctuary = $workdir;

pr todo {
    url  = `git@github.com:yourname/todo.git`;
    dir  = `todo`;
    sync = `clone`;
    branch = `main`;
    include = `.kiru/main.kiru`;

    var shell version = `git describe --tags --always --dirty 2>/dev/null || echo dev`;

    fn build {
        log `Building ${app} at ${version}`;
        var shell os = `uname -s`;
        case $os {
            `Linux`  {
                cd `cmd`;
                exec `go build -ldflags='${version}' -o bin/${app} .`;
            };
            `Darwin` {
                cd `cmd`;
                exec `go build -ldflags='${version}' -o bin/${app} .`;
            };
            _        { log `unsupported OS: ${os}`; };
        };
    }

    fn test {
        env [
            CGO_ENABLED = `0`,
            GOPATH = `$HOME/go`
        ] {
            exec `go test -race ./...`;
            exec `go vet ./...`;
        };
    }

    run ci {
        test => build;
    }
}
</pre>

### How it works

1. **`var shell`** captures command output into a variable (`workdir`, `version`)
2. **`fn`** blocks scope all state — vars, env, cwd — to that block. Nothing leaks.
3. **`exec`** runs a shell command. Non-zero exit fails the fn.
4. **`env [...] { }`** sets env vars for one block. Inner env overrides outer. They restore on exit.
5. **`case`** branches on a value. Patterns can be literals, `$var` refs, or `_` (default).
6. **`run`** chains fns. `test => build` runs test first, then build. Multiple chains (`;`-separated) run concurrently.

---

## Commands

| command | what it does |
|---------|-------------|
| `kiru sync` | clone/update all declared repos into sanctuary |
| `kiru run <name> <project>` | execute a run block (interactive TUI) |
| `kiru fn <name> <project>` | execute one function (plain output) |
| `kiru validate` | parse and validate the config |
| `kiru version` | print version |

When `SANCTUARY=0`, kiru runs in standalone mode — no sanctuary, no projects, just top-level `fn` and `run` blocks. Config defaults to `./.kiru/main.kiru`. Useful for CI/CD.

---

## DSL reference

### Declarations

| declaration | description |
|-------------|-------------|
| _(no shell declaration needed)_ | uses `$SHELL` from environment |
| `sanctuary = \`...\` \| $var;` | required. absolute path to workspace root |
| `` import `./path`; `` | import other `.kiru` files, relative paths only |
| `var string name = \`...\` \| $var;` | string variable (global or project-scoped) |
| `var shell name = \`...\`;` | runs content via `$SHELL`, stores stdout |
| `pr name { ... }` | project declaration |
| `fn name { ... }` | execution block |
| `run name { ... }` | orchestration block with chain syntax |

### Variable scope

- **Global vars** (top-level) — accessible everywhere: project fields, project vars, fn bodies.
- **Project vars** (inside `pr { }`) — accessible only within that project's fn bodies.
- **Fn-local vars** (inside `fn { }` or `env { }`) — scoped to that block, shadow outer vars.

### Project fields

| field | required | description |
|-------|----------|-------------|
| `url` | yes | git clone url |
| `dir` | yes | directory name relative to sanctuary, must be unique |
| `sync` | no | `clone` (default, skip if exists) or `ignore` |
| `include` | no | path to a `.kiru` file inside the project, relative to project dir |
| `branch` | no | branch to clone, defaults to repo default |

### fn primitives

| primitive | description |
|-----------|-------------|
| `exec \`...\`;` | run command via `$SHELL`. non-zero exit fails the block |
| `cd \`...\`;` | change cwd relative to project dir. cannot escape project dir |
| `log \`...\`;` | print to output. never fails |
| `env [...] { };` | scoped env vars. inner env overrides outer. no leakage |
| `var <type> name = ...;` | fn-local variable. shadows outer var with same name |
| `case <expr> { ... };` | conditional branching. first-matching arm wins |

### case statement

```
case <expr> {
    `literal`    { ... };
    $var_ref     { ... };
    _            { ... };   # default
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
| `fn_a; fn_b => fn_c;` | two concurrent chains: fn_a alone and fn_b → fn_c |

Chains run concurrently. If a function in a chain fails, the rest of that chain is skipped but other chains continue.

### Values

| syntax | type | notes |
|--------|------|-------|
| `` `...` `` | string | use `${name}` to interpolate variables |
| `$name` | var ref | standalone reference outside backticks |

### Delimiters

| token | job |
|-------|-----|
| no parens | primitives are bare keywords — `exec`, `cd`, `log` |
| `[]` | typed list — `env[]` |
| `{}` | statement block |
| `;` | statement terminator inside `{}` and run chain separator |
| `,` | item separator inside `[]` |
| `=>` | sequential chain separator inside run blocks |

### Rules

- `exec` and `var shell` use the user's current shell (`$SHELL`, fallback `sh`).
- `sanctuary` must be declared before any `pr`.
- Variables must be declared before they are referenced.
- `cd` cannot escape the project directory. Hard fail at runtime.
- Circular imports fail at parse time.
- Two projects cannot share the same `dir`. Parse error.
- `env` blocks save and restore variable scope — declarations inside are local.

### TUI

During `kiru run`, an interactive TUI shows live progress:
- Spinner on running tasks
- Color-coded: green (ok), red (failed), yellow (running), gray (pending/skipped)
- Press `q` or `Ctrl+C` to abort
- After completion, a colored ANSI summary dump is printed

---

## Per-project config

If a project declares `include`, that file is parsed after `kiru sync` clones the repo. It can define fns scoped to that project but cannot declare `sanctuary` or `pr`.

```
# calendar/.kiru/main.kiru

fn build {
    exec `pnpm build`;
}

fn dev {
    exec `pnpm dev`;
}
```

---

## Contributing

Contributions are welcome! Open issues or submit pull requests.

## License

[MIT](./LICENSE)
