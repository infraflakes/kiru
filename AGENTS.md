- ZERO BACKWARD COMPATIBILITY ALLOWED
- Reasoning in any language for efficiency, but response in English.
- Centralized patterns are reused across the codebase instead of imperatively modified per function
- Do not have the "build on top of old things" mentality, if its fundamentally wrong, break it.
- Do not abuse .clone(), .unwrap(), and Arc:: in general
- src/*.rs re-export module entry points; src/<dir>/ files are internal logic. Applies for every sub directories that followed. 
- Functions are preferable to be doing its own task, import many functions instead of doing everything themselves, only after verifying that the task itself, can be reused by others without variants.
- src/cli/ crates are the main input of external data for `kiru` whereas other crates are the engine for it.
- Variable names should be self explanatory, named after what it does so anyone who isn't familiar with the codebase can understand, recommends verbose semantic terms then academic terms when matched complexity.
- Naming conventions should be natural, self explanatory and verbose English-like.
- Implementations should automatically resolve bugs at the fundamental level with generics, instead of implementing imperative hacky fixes or special cases.
- Correctness is prioritized over simplicity.
- Simplify, merge, decouple at any time possible after considering maintainability, future growth and clean/reasonable separation.
- Extensive documentations and comments for codes, must be timeless and not documenting recent changes.
- Agree to on using project's helpers throughout the codebase export via public API instead of reinventing the wheel.
- No fancy symbols at documentations.
- Overall architecture should be built for ease of extend and scaling whilst also being decoupled.
- For non-obvious domains (process management, signals, TTY, config formats, concurrency), research established patterns with web search before implementing; the first version should match industry practice instead of rediscovering known bugs by testing.

## The language's model

- The type model is a graph of groups: scalar, rec, enum, function, parameter. Each group declares everything about its kind - arity, written name, display name, whether methods may bind to it, whether it has fields, what shape it fits, how it unifies and substitutes, its runtime shape. Every type, whether intrinsic, declared, or provided by a native, is marked with a group and inherits its facts. Relations read the group; nothing re-derives a kind by matching on it, and an unmarked kind is a compile error rather than a wildcard arm that silently swallows it.
- The intrinsics are txt, bool and int. Vec is a declared rec named type in the std dynamic namespace, not an intrinsic, and it has no brackets: a value is built and changed through its new, push, replace, at and len operations.
- A namespace is a directory. A file belongs to its directory's namespace and its filename is arbitrary. There is no mod and no import. Resolution is order free: every declaration is collected first, then references resolve, so a forward reference and a cycle inside one namespace are both legal.
- A compilation names one project manifest, `kiru.toml`. Its `project.root` is the program tree: every `.kiru` file under it joins the program, and the tree's directory structure is its namespaces with `root` as the first segment; `project.output` is the binary's path. The root namespace's `main` is the entry: reachability starts there, the relation graph is built from it, and every declaration the graph never reaches is pruned before lowering. A project whose root namespace declares no `main` does not compile; a `main` in a sub-namespace is an ordinary function.
- Unused facts are warnings, never silence: an unused use, an unused local binding, and a program declaration no reachable body reads. A warning names its file, line and column in the diagnostic format and never withholds the output. The manifest's `[diagnostics]` table may silence a warning kind, never turn one into an error.
- An error withholds the output, never the information: the analysis continues over what the source allows, every fact it can still earn is reported, and only then is no output written. The compiler never stops at the first error when it can still learn something.
- Every absolute path resolves on its own, with no use: std::dynamic::Vec.new(), root::kiru::usage(). A use only shortens a path, and may end at a namespace, at a type, or at a plain function. A type brings its bound functions with it. There is no as form. A use sits anywhere in the file and belongs to the namespace, not to the file.
- A bound function is declared inside an impl block for its type and is never a namespace level name, so a use cannot name it. A function whose first parameter is its own type is a method, and is reached as a value call or as a namespace qualified call with the receiver written out. A function without such a first parameter is an associated function, reached only through its type: a Type.name call, or Type.name as a value. Its reachability is pub on the type and pub on the function, never a use.
- A name is unique inside its namespace, always. Only functions bound to different types may share one.
- Module values are never pub.
- A call always carries parentheses; a bare name is always a variable.
- Natives are the runtime's floor and are public. A native that touches the operating system lives in the syscalls namespace and carries its syscall or C library name; a pure one stays with its kind, the container's in the dynamic namespace and the text kernel's in the text namespace. A native is bound to a type when its first parameter is that type, or when it takes no parameters and returns a declared type.
- Every directory under the library root carries a natives.md listing that namespace's natives: what each takes, what it does, and whether it returns a value.

## Working rules

- An unstated detail is a question, not a guess. A rule that is stated is enforced on every path or it is a bug.
- Brief a helper with absolute, specific steps and exact paths, and never leave a decision to it. Verify its work independently by running the checks yourself, and treat its report as a claim rather than a fact.
- A task that changes no behaviour must leave the test count unchanged. State the expected number in the brief and check it.
- Never change a rule's semantics without the owner's word. When a fix would need a rule widened, stop and ask instead of widening it.
- CodeGraph is how this codebase is read. Run `codegraph sync` before using it and after every commit, then `codegraph explore "<symbols or question>"`, which returns the source of the relevant symbols and the call paths between them in one call.
- Reading a file with a reader is permitted only after CodeGraph has returned nothing for that symbol, and searching with grep only to locate a symbol CodeGraph does not know. Opening files instead of asking the graph is doing the task wrong, not doing it differently.
- Never claim a repository has no index without looking: check for its `.codegraph` directory first, and say it out loud when you fall back to reading. An asserted absence is a lie the next person acts on.
- A helper's report states the `codegraph explore` queries it ran. A helper that read files without them repeated the work the graph exists to remove.

## Where things live

- src/project/ owns the manifest: reading it, validating it, finding the nearest one above a file, and the warning and publication policies it declares. The manifest is the only project identity; the compiler, the command line and the server read the same `Project`.
- src/compiler/ is the engine and holds two processes: compile/ and analyze/. Each process and each stage is an entry file beside its directory. compile/ runs load, resolve, check, prune, lower, package in that order: load reads the manifest's program tree and the embedded library through the program source and derives a namespace from each file's directory; resolve collects every declaration, resolves references and links every body; check types and checks every body; prune drops what the root `main` never reaches; lower turns bodies into instructions; package writes the image at the manifest's output. compile/build.rs is the checked program both processes read. analyze/ loads a whole tree, resolves and checks every body, and groups the facts by file; the server publishes one file or the tree as the manifest asks. The checked program holds no entry: the root `main` is found by name after the program is resolved, and is passed to the stages that start from it.
- src/compiler/ reads no program source from disk. A program source provides the files: the command line supplies the disk's source, the language server supplies the editor's buffers, and the tests supply a virtual tree. Packaging writes the output image; the bytes of the running executable are read by the command line, which hands them to the compiler's trailer format.
- src/compiler/model/ is the graph and holds no stage logic: ids, declarations, expressions, types (the group model), program (the graph, the name and method and type tables, type naming, namespace paths, field lookup), visitors, verify_graph, verify_spans.
- src/compiler/bytecode/ defines the instruction set and its validation. Nothing outside it names an instruction variant.
- src/runtime/ executes: the virtual machine, its native dispatch, values, and the process, thread and signal pools.
- src/sync/ is the crate's one lock policy: a poisoned lock is recovered, never propagated. The runtime and the server both read it.
- src/syntax/ is the surface: tokens, lexer, parser, syntax tree, spans.
- src/cli/ is the only place a terminal is spoken to, and the only place the compiler's disk input enters: it supplies the disk source and renders diagnostics for a terminal.
- src/lsp/ is the language server's client: the protocol session, the position encoding, and the documents over the disk. It supplies every open buffer as a program source and publishes the analysis's diagnostics for the files its project names.

## The language server

- `kc lsp` finds the nearest `kiru.toml` above the file and analyzes the whole tree that manifest names, beside the embedded library: it resolves every name, links and checks every body, and settles every fact a body reads. Checking one file never invents a fact about another: a callee that panics in another file still settles the caller's flow. It selects no entry, prunes nothing, lowers nothing.
- The manifest's `[lsp] scope` decides what is published: `file` (the default) publishes the open file's facts; `tree` publishes every file of the program tree and clears a file that grows clean. A warning kind the `[diagnostics]` table silences is never published.
- A directory named `std` that holds every file the binary embeds is the one special tree: it is analyzed as the library, under the `std` namespace, with the editor's buffers over the disk, and no manifest. It is recognized by its contents, so an installed binary needs no source path. That mode is not reachable from a project's configuration.
- A file with no manifest above it, or outside the manifest's root, is not analyzed: the server clears its diagnostics and says why once. A manifest that cannot be read is the same: one message, no per-file diagnostics.
- The library is std/, embedded into the compiler from the `std/**` sources and mounted under `std` for every program. A directory is a namespace and a file name is arbitrary. A file's own declarations belong to its directory's namespace, an impl block binds functions to a type, and a natives.md in each directory lists that namespace's natives.

## Adding a native

1. Add one row to the native_registry! table in src/compiler/native_registry.rs: its namespace path, its name, and its typed parameters and return. The macro generates the variant, the shape and the typed row together.
2. Add one arm to the handler match in src/runtime/virtual_machine/native_dispatch.rs. That match is exhaustive with no wildcard, so the compiler demands the arm.
3. A native that touches the operating system goes in the syscalls namespace under its syscall or C library name. A pure one stays with its kind: the container's in the dynamic namespace, the text kernel's in the text namespace.
4. Its namespace must be a directory under the library root, and that directory's natives.md must list it with what it takes, what it does and whether it returns.
5. Nothing else changes. A native is public. It is bound to a type when its first parameter is that type, or when it takes no parameters and returns a declared type, and it never earns a special case in the checker.

## Adding or changing a type rule

- A kind's facts are declared in exactly one place: src/compiler/model/types/group.rs. Add a fact as a match over Group with one arm per group and no wildcard arm. The relations in src/compiler/model/types/relation.rs read those facts and are never edited to admit a kind.
- The representation and the marking live in src/compiler/model/types.rs.
- Adding one kind means editing the group declaration and the marking, and nothing else. If a change would need a rule widened to fit an implementation, stop and ask.

## Adding a syntax form

- A token goes in src/syntax/lex/token.rs and its spelling in src/syntax/lex/scanner.rs. A form goes in the parser file for its category under src/syntax/parse/: declarations, statements or expressions. Its node goes in src/syntax/ast.rs.
- No pass may decide kind by matching on the syntax tree. Every pass reads the model.

## When the language changes

A language change is not finished until every description of the language agrees with it. After any change to the syntax, the type model, the reference model, or the library's surface:

1. `src/syntax` and the compiler's passes are the source of truth; they move first.
2. `/home/nixenv/Projects/kiru-tree-sitter`: the grammar, the queries and the corpus. Regenerate, run the corpus, and parse every real `.kiru` file with no error node.
3. `docs/`: every `kiru` block compiled and run, and the site built.
4. `std/*/natives.md`: every native of that namespace.
5. `ROADMAP.md`: the decisions.
6. This file, when the model section moved.

A change that lands without those is an unfinished change, whatever the test suite says.

## Checks

Every change ends green on all four, and the count is stated in the brief and checked:

1. cargo test
2. cargo clippy --all-targets -- -D warnings
3. cargo fmt --check
4. cargo build, then `kc pipeline/kiru.toml`, which builds the project with itself; its payload's sha is the gate.

<!-- CODEGRAPH_START -->
## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- **MCP tool** (when available): `codegraph_explore` answers most code questions in one call — the relevant symbols' verbatim source plus the call paths between them, including dynamic-dispatch hops grep can't follow. Name a file or symbol in the query to read its current line-numbered source. If it's listed but deferred, load it by name via tool search.
- **Shell** (always works): `codegraph explore "<symbol names or question>"` prints the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely — indexing is the user's decision.
<!-- CODEGRAPH_END -->
