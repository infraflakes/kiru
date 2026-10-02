# tree-sitter-kiru

A [Tree-sitter](https://tree-sitter.github.io/) grammar for the
[kiru](https://github.com/infraflakes/kiru) build language, plus editor
queries for syntax highlighting and local scope resolution.

Kiru source files use the `.kiru` extension. The grammar covers the accepted
syntax in the language spec: an optional `module` path, `import`
declarations, `fn`/`txt`/`rec` declarations, assignments, `return`,
`switch`/`case`/`default`, `defer`, `::` qualified paths (including a leading
`::`), strings that may span lines, and `#` comments. There are no numeric
literals in kiru.

## Layout

```text
tree-sitter-kiru/
  grammar.js                 grammar definition
  tree-sitter.json           tree-sitter CLI configuration
  package.json               npm package metadata
  queries/kiru/highlights.scm  syntax highlighting captures
  queries/kiru/locals.scm      scopes, definitions, and references
  ftdetect/kiru.lua          maps the .kiru extension to the kiru filetype
  ftplugin/kiru.lua          starts the highlighter for kiru buffers
  src/                       generated parser (parser.c, grammar.json, node-types.json)
  parser/                    built shared library (parser/kiru.so, git-ignored)
  test/corpus/               corpus tests, one sample per construct
```

## Build the parser

The parser is generated from `grammar.js`. Its output is committed under
`src/`, so consumers do not need the CLI.

```sh
# From tree-sitter-kiru/
tree-sitter generate
tree-sitter test
```

If the CLI is not installed, any of these provide it:

```sh
nix run nixpkgs#tree-sitter -- generate   # Nix
bunx tree-sitter-cli generate             # Bun
npx tree-sitter-cli generate              # Node
```

Build the shared library Neovim loads. The generated parser uses ABI 14 so
it works across current Neovim releases:

```sh
tree-sitter generate --abi 14
cc -shared -fPIC -O2 -I src src/parser.c -o parser/kiru.so
```

## Try it in Neovim

With the library built at `parser/kiru.so`, this checkout is a self-contained
Neovim runtime directory: `parser/`, `queries/kiru/`, `ftdetect/`, and
`ftplugin/` are all where Neovim looks.

From the repository root, with no plugin manager in the way:

```sh
nvim --cmd "set runtimepath+=tree-sitter-kiru" main.kiru
```

Plugin managers such as lazy.nvim replace `runtimepath` during startup, which
discards paths added with `--cmd`. In that case add the path after startup
and set the filetype explicitly; the shipped `ftplugin` then starts the
highlighter:

```sh
nvim main.kiru -c "set runtimepath+=tree-sitter-kiru" -c "set ft=kiru"
```

For a permanent setup, add the directory to your configuration instead:

```lua
vim.opt.runtimepath:append("/absolute/path/to/tree-sitter-kiru")
```

### Captures

The `highlights.scm` query keeps the palette small on purpose. Tree-sitter's
guidance is that an entity should be colored the same wherever it appears, so
a name never changes color between its declaration, its uses and its call
sites, and a namespace is colored like any other name. Color is spent only on
what a reader scans a file for: comments, strings, the keywords that shape a
run, the two value kinds, and the names you can call. Every other name, be it
a value, a parameter, a field, a record key or a namespace, shares one color,
and operators and punctuation keep the colorscheme's own subdued styling.

Kiru has one kind of callable, so a function in the root namespace, a function
in another namespace, and a function the standard library provides are all
captured as `@function`, the same color as the declaration they come from.
The captures are the standard Neovim names, so any colorscheme that
supports Tree-sitter highlighting works without extra groups: `@comment`,
`@string`, `@keyword`, `@keyword.function`, `@type`, `@variable`, `@function`,
`@operator`, `@punctuation.bracket` and `@punctuation.delimiter`.
The `locals.scm` query defines the scopes needed for local variable references.

## Helix

Helix ships Tree-sitter and reads a `languages.toml`. Add a language entry
that points at this grammar. The generated `src/` is enough for runtime
loading:

```toml
[[language]]
name = "kiru"
scope = "source.kiru"
file-types = ["kiru"]
roots = []
grammar = "kiru"

[[grammar]]
name = "kiru"
source = { path = "/path/to/tree-sitter-kiru" }
```

Then reload and build:

```sh
hx --health kiru
hx --grammar fetch
hx --grammar build
```

Helix uses the same `highlights.scm` and `locals.scm`.

## Zed

Create a Zed extension that provides a Tree-sitter language for `source.kiru`
with `file-types: ["kiru"]`. Point `grammar.js` at this directory, mark
`highlights.scm` as the highlight query, and, if desired, `locals.scm` as the
locals query. Zed compiles the parser from source using the extension's
`tree-sitter` dependency.

## License

MIT. See the repository `LICENSE`.
