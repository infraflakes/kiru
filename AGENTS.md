- ZERO BACKWARD COMPATIBILITY ALLOWED
- Reasoning in any language for efficiency, but response in English.
- Centralized patterns are reused across the codebase instead of imperatively modified per function
- Do not have the "build on top of old things" mentality, if its fundamentally wrong, break it.
- Do not abuse .clone(), .unwrap(), and Arc:: in general
- src/*.rs re-export module entry points; src/<dir>/ files are internal logic.
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
- Every absolute path resolves on its own, with no use: std::dynamic::Vec.new(), root::kiru::usage(). A use only shortens a path, and may end at a namespace, at a type, or at a plain function. A type brings its bound functions with it. There is no as form. A use sits at the top of a file and belongs to the namespace, not to the file.
- A bound function is declared inside an impl block for its type and is never a namespace level name, so a use cannot name it. A function whose first parameter is its own type is a method, and is reached as a value call or as a namespace qualified call with the receiver written out. A function without such a first parameter is an associated function, reached only through its type: a Type.name call, or Type.name as a value. Its reachability is pub on the type and pub on the function, never a use.
- A name is unique inside its namespace, always. Only functions bound to different types may share one.
- Module values are never pub.
- A call always carries parentheses; a bare name is always a value.
- Natives are the runtime's floor and are public. A native that touches the operating system lives in the syscalls namespace and carries its syscall or C library name; a pure one stays with its kind, the container's in the dynamic namespace and the text kernel's in the text namespace. A native is bound to a type when its first parameter is that type, or when it takes no parameters and returns a declared type.
- Every directory under the library root carries a natives.md listing that namespace's natives: what each takes, what it does, and whether it returns a value.

## Working rules

- An unstated detail is a question, not a guess. A rule that is stated is enforced on every path or it is a bug.
- Brief a helper with absolute, specific steps and exact paths, and never leave a decision to it. Verify its work independently by running the checks yourself, and treat its report as a claim rather than a fact.
- A task that changes no behaviour must leave the test count unchanged. State the expected number in the brief and check it.
- Never change a rule's semantics without the owner's word. When a fix would need a rule widened, stop and ask instead of widening it.

## Where things live

- src/compiler/ is the engine. Its stages run in this order: load_files walks the entry file's directory tree and the embedded library and derives a namespace from each file's directory; resolve_names collects every declaration, then resolves references and links bodies; validate_program types and checks every body; prune_unreachable drops what the entry never reaches; lower_bytecode turns bodies into instructions; package_binary writes the image.
- src/compiler/model/ is the graph and holds no stage logic: ids, declarations, expressions, types (the group model), program (the graph, the name and method and type tables, type naming, namespace paths, field lookup), visitors, verify_graph, verify_spans.
- src/compiler/bytecode/ defines the instruction set and its validation. Nothing outside it names an instruction variant.
- src/runtime/ executes: the virtual machine, its native dispatch, values, and the process, thread and signal pools.
- src/syntax/ is the surface: tokens, lexer, parser, syntax tree, spans.
- src/cli/ is the only place external data enters and the only place a diagnostic is rendered for a terminal.
- The library is std/. A directory is a namespace and a file name is arbitrary. A file's own declarations belong to its directory's namespace, an impl block binds functions to a type, and a natives.md in each directory lists that namespace's natives.

## Adding a native

1. Add one row to the native_registry! table in src/native_registry.rs: its namespace path, its name, and its typed parameters and return. The macro generates the variant, the shape and the typed row together.
2. Add one arm to the handler match in src/runtime/virtual_machine/native_dispatch.rs. That match is exhaustive with no wildcard, so the compiler demands the arm.
3. A native that touches the operating system goes in the syscalls namespace under its syscall or C library name. A pure one stays with its kind: the container's in the dynamic namespace, the text kernel's in the text namespace.
4. Its namespace must be a directory under the library root, and that directory's natives.md must list it with what it takes, what it does and whether it returns.
5. Nothing else changes. A native is public. It is bound to a type when its first parameter is that type, or when it takes no parameters and returns a declared type, and it never earns a special case in the checker.

## Adding or changing a type rule

- A kind's facts are declared in exactly one place: src/compiler/model/types/group.rs. Add a fact as a match over Group with one arm per group and no wildcard arm. The relations in src/compiler/model/types/relation.rs read those facts and are never edited to admit a kind.
- The representation and the marking live in src/compiler/model/types.rs.
- Adding one kind means editing the group declaration and the marking, and nothing else. If a change would need a rule widened to fit an implementation, stop and ask.

## Adding a syntax form

- A token goes in src/syntax/token.rs and src/syntax/lexer.rs. A form goes in the parser file for its category: declarations, statements or expressions. Its node goes in src/syntax/ast.rs.
- No pass may decide kind by matching on the syntax tree. Every pass reads the model.

## Checks

Every change ends green on all four, and the count is stated in the brief and checked:

1. cargo test
2. cargo clippy --all-targets -- -D warnings
3. cargo fmt --check
4. cargo build, then the compiler on pipeline/main.kiru, which builds the project with itself.
