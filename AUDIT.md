# Architecture audit — every deviation from the model

The model being enforced, restated from `AGENTS.md`:

- **The type model is a graph of groups:** scalar, rec, enum, function,
  parameter. Each group declares everything about its kind — arity, written and
  display name, whether methods may bind, whether it has fields, what shape it
  fits, how it unifies and substitutes, its runtime shape. Every type, intrinsic,
  declared, or native-provided, is marked with a group and inherits its facts.
  Relations read the group; nothing re-derives a kind by matching on it, and an
  unmarked kind is a compile error rather than a wildcard arm that swallows it.
- **The intrinsics are `txt`, `bool`, `int`.** `Vec` is a declared `rec` in
  `std::dynamic`, not an intrinsic, and it has no brackets: a value is built and
  changed through `new`, `push`, `replace`, `at`, `len`.
- **A namespace is a directory.** A file belongs to its directory's namespace and
  its filename is arbitrary. There is no `mod` and no `import`. Resolution is
  order-free: every declaration is collected first, then references resolve.
- **Every absolute path resolves on its own; a `use` only shortens a path.** A
  `use` may end at a namespace, a type, or a plain function, and a type brings its
  bound functions with it. A bound function is declared in an `impl` block and
  reached only through its type — a value call, a `Type.name` call, or `Type.name`
  as a value. It is never a namespace-level name, never shortened by a `use`,
  never reached by a namespace-qualified path. Reachability is `pub` on the
  function and `pub` on the type.
- **A name is unique in its namespace.** Only functions bound to different types
  may share one. Module values are never `pub`. A call always carries
  parentheses. Natives are the floor, public, and named for their syscall or
  their kind.

Produced by five parallel audits, then re-verified against the tree as it is now.
Each finding is marked **✅ closed** with the current evidence, or **⚠ stands**
with the current evidence. File references to modules that no longer exist are
gone; the logic is cited where it lives today.

> **Overall status.** The old root causes — "a function is a kind", the
> type-name special forms, the implicit defaults, the deleted stdlib wrappers,
> and the import machinery — are closed. What stands is either the deliberate
> shape of the engine (address spaces, instruction variants, the checker's
> library semantics) or one live deviation from the model, recorded in section 8.

---

## 1. A function is a KIND, not a value

Closed by building one declaration node whose value is a function expression.

- **✅ Closed.** `Declaration` is one node — a name, a type, and an optional
  value — not an enum of kinds. `model/declarations.rs:17`.
- **✅ Closed.** One value node for a function: `Expression::Function(Function)`,
  the value of a named `fn` and of a `fn(...) { .. }` literal alike.
  `model/expressions.rs:104`; the syntax node is `Expression::Closure`
  (`syntax/ast.rs:332`).
- **✅ Closed.** `fn` still parses in three positions, but each is one of the
  model's forms — a declaration, a function type, a function literal — and all
  three land in the one `Expression::Function` node. `syntax/parser/declarations.rs:269`
  (declaration), `:177` (type), `:297` (literal).
- **✅ Closed.** No `DeclarationKind`. `model/declarations.rs:17`.
- **✅ Closed.** `Function` carries `ty`, `parameters`, `body`,
  `type_parameters`, `captures`, `span`; `is_method` and `is_closure` are gone.
  `model/expressions.rs:13-26`.
- **✅ Closed.** `Declaration::function()` returns `Option<&Function>` and there
  is no `initializer()` kind branch. `model/declarations.rs:40`.
- **✅ Closed.** One name pool per namespace (`NameTable`) plus a method pool and
  an associated-function pool keyed by `(head, name)` (`MethodTable`). This is
  the model's mechanism, not a kind split: a name is unique in its namespace and
  only bound functions for different heads repeat. `model/program/tables.rs:60`,
  `:103`.
- **✅ Closed.** `resolve`/`resolve_name`/`search` take no registry.
  `resolve_names/name_resolution.rs:23`, `:86`, `:110`.
- **✅ Closed.** No `unknown_name` other-pool message. `resolve_name` reports
  unknown or ambiguous. `resolve_names/name_resolution.rs:94-102`.
- **✅ Closed.** A name resolves local first, then its namespace and its
  `use`s; no value-first/function-fallback. `resolve_names/name_resolution.rs:29-36`.
- **✅ Closed.** No `context.current_is_function`.
- **✅ Closed.** No `function_used_as_value`; a function reference is an ordinary
  value. `model/expressions.rs:62-68`.
- **⚠ Stands.** A generic function is not a value: it has no single function
  type. This is the one rule about functions that is not "a function is a
  value", and it follows from generics rather than from a kind. Enforced at the
  reference/call check; a generic name used where a value is expected is a type
  mismatch.
- **✅ Closed.** `call` accepts any callee whose type is a function type; there
  is no `Function | Native` restriction. `model/expressions.rs:71-76`.
- **✅ Closed.** A method is a function in an `impl` block whose first parameter
  accepts the block's type; the parameter name is irrelevant.
  `resolve_names/register_declarations.rs:128-144`.
- **✅ Closed.** The method table is `MethodTable`, keyed by the receiver head
  the group model derives, and it is the model's mechanism for a repeated name.
  `model/program/tables.rs:103`; heads come from `Type::head`
  (`model/types.rs:89`).
- **✅ Closed.** No `Function.is_method` and no `verify.rs` ordering exemption;
  `EdgeCheck` checks ids only, never position. `model/verify_graph.rs:184-206`.
- **⚠ Stands (by definition).** The entry is the program root's `main` by name,
  and its shape rules are its definition, not a kind rule.
  `resolve_names/select_entry.rs:15-83`. The shape is checked through the one
  type relation (`fits`) and the one type resolver, so `Vec<txt>` is a
  declaration like any other.
- **✅ Closed.** No "a function cannot reference itself" and no forward-reference
  ban: resolution is order-free and recursion is legal.
  `resolve_names/name_resolution.rs:1-9`.
- **✅ Closed.** No `Skeleton.order`/`Pending.order`.
  `resolve_names/builder/registration.rs:37-57`.
- **✅ Closed.** `EdgeCheck` carries no order rule and no method exemption; it
  checks that every edge points into the declaration table.
  `model/verify_graph.rs:188-206`.
- **✅ Closed.** One `Expression::Call`; there is no `Expression::CallValue`.
  The instruction set still splits a native call from a value call
  (`CallNative`/`CallValue`, `AsyncNative`/`AsyncValue`), which is native-by-index
  versus function-value, an address-space difference and not a value kind.
  `bytecode/instruction.rs:96`, `:162`, `:181`, `:186`.
- **✅ Closed.** Lowering no longer branches on a declaration kind (there is
  none); it lowers the one `Expression::Call` by the callee's node.
  `lower_expressions.rs:448-482`.
- **✅ Closed.** A native registers as an ordinary callable node with a `native`
  marker, not in a separate table. `resolve_names/builder/registration.rs:79-84`;
  `resolve_names/register_builtins.rs:38-99`.
- **⚠ Stands (by design).** `LoadGlobal`/`LoadLocal`/`LoadConst` are three
  address spaces — global table, frame register, constant pool — not a value
  kind. `bytecode/instruction.rs:34-45`.
- **✅ Closed.** The stdlib wrappers that existed only because a native could not
  be a method are gone; the text kernel natives are methods now.
  `std/text/text.kiru:1-12`.

## 2. Type-relation pluralism — one job, many relations

Consolidated under the one relation.

- **✅ Closed.** `fits` is `unify` with no binders: one compatibility relation.
  `model/types/relation.rs:92`.
- **✅ Closed (by design).** `TypeHead`/`Type::head` is derived from the group,
  not a second kind match. `model/types.rs:89`.
- **✅ Closed.** `unify` is the one relation; `fits` specializes it.
  `model/types/relation.rs:102-108`.
- **✅ Closed.** `has_parameter` is a one-line wrapper over the one walk,
  `type_parameters_of`. `model/types/relation.rs:197`.
- **✅ Closed.** `require` is a one-line wrapper over `expression` with the
  position's declared type. `validate_program/expression_typing.rs:574-585`.
- **⚠ Stands.** `receiver_type` still infers a receiver's type at link time,
  before the checker exists, so a forward method reference can dispatch. It is
  an approximation the checker re-verifies, and it matches on linked expressions,
  not on type kinds. `resolve_names/receiver_typing.rs:23-65`.
- **⚠ Stands.** A match's type is the common type of its arm expressions.
  `validate_program/match_rules.rs:160`, `:330`.
- **✅ Closed.** Contextual typing is one mechanism: `expression` takes an
  optional expected type, and `require` supplies it.
  `validate_program/expression_typing.rs:574`.
- **✅ Closed.** No `Type::Void`. A function with no return type has a function
  type whose `returns` is `None`. `model/types.rs:44-50`.
- **✅ Closed.** No `Type::Map(key, value)`; `Map` is a declared `rec`.
  `model/types.rs:33-51`.

## 3. Keywords and forms that exist for a KIND

- **⚠ Stands (by design).** The reserved words are the forms, not the type names:
  `use`, `pub`, `fn`, `impl`, `enum`, `let`, `rec`, `match`, `return`, `panic`,
  `async`, `wait`, `for`, `break`, `true`, `false`. `syntax/lexer.rs:214-232`.
- **✅ Closed.** `vec` is not reserved; `Vec` is an ordinary declaration and
  `map` is an ordinary name. `resolve_names/register_builtins.rs:16-32`.
- **✅ Closed.** No `self` contextual keyword; the receiver is by type.
  `resolve_names/register_declarations.rs:128-144`.
- **⚠ Stands (by design).** `_` is recognized as a pattern binding by position.
  `syntax/parser/statements.rs`.
- **⚠ Stands (by design).** A match's arms are expressions or blocks by position;
  the model does not fix one shape. `syntax/ast.rs:251`.
- **⚠ Stands (by design).** The statement grammar is a per-keyword dispatch.
  `syntax/parser/statements.rs`.
- **✅ Closed.** Two assignment forms (bare and field); the index form is gone
  with the index brackets. `syntax/ast.rs:190-206`.
- **✅ Closed (by design).** `fn(T)` with no arrow is a function type whose
  `returns` is `None`; there is no `void` type. `syntax/parser/declarations.rs:326`.
- **⚠ Stands (by design).** `<` is a type-argument bracket in type positions and
  a comparison in expression positions. `syntax/parser/declarations.rs:336`.
- **⚠ Stands (by design).** A bare `{ … }` subject is recognized by lookahead.
  `syntax/parser/statements.rs`.
- **✅ Closed.** `TokenKind::DotDot` is gone. `syntax/token.rs`.

## 4. Implicit defaults

- **⚠ Stands (by design).** `Value::Nothing` is the result of a call that
  returns nothing and the `none` of the absence. The language never stores it; it
  marks a call whose value a statement discards. `runtime/values.rs:36`.
- **✅ Closed.** A missing key reads as the absence, never empty text.
  `runtime/virtual_machine.rs:354-361`.
- **✅ Closed.** `getenv` returns `Option<txt>`; unset and empty are told apart.
  `native_registry.rs:189`; `std/process/env.kiru`.
- **✅ Closed.** No vec literal, so no empty-literal default.
- **✅ Closed.** Native rows carry `Type`s; there is no `NativeType` universe.
  `native_registry.rs:63-68`.
- **✅ Closed.** No `for item in xs`; the item binding default is gone.
- **✅ Closed.** An absent return arrow means a function type whose `returns` is
  `None`, not a `Void`. `syntax/parser/declarations.rs:326`.
- **⚠ Stands (by definition).** `main` with no return means exit `0`.
  `runtime/virtual_machine.rs:74`.
- **⚠ Stands.** `lower_async` silently drops a non-call with an
  `else { return; }`. `lower_expressions.rs:464-470`.
- **⚠ Stands (stdlib, by design).** `std::cli::Context.no_action` is the default
  action of a command that declares none. `std/cli/cli.kiru`.

## 5. Rust library logic above the syscall floor

- **✅ Closed.** `trim`/`split` are kiru over the byte kernel, ASCII-only.
  `std/text/text.kiru:52-140`.
- **✅ Closed.** `glob` is kiru over `readdir`. `std/fs/fs.kiru:1-260`.
- **⚠ Stands (renamed).** The text kernel is byte access: `strlen`, `byte_at`,
  `memchr`, `substr`. `native_registry.rs:178-181`.
- **✅ Closed.** `temp_file` builds its template from `std::process::temp_dir` in
  kiru; the native is `mkstemp`. `std/fs/fs.kiru:12-15`.
- **✅ Closed.** The signal-name table is `std::process::signal` in kiru; the
  native `killpg` takes the number. `std/process/process.kiru`.
- **✅ Closed.** `pid_of` parsing is kiru; the native `waitpid` takes an int.
- **✅ Closed.** `std::process::sleep` checks the count in kiru; the native
  `nanosleep` takes an int. `std/process/time.kiru`.
- **⚠ Stands.** `command_for` builds the OS command from the argv in Rust.
  `runtime/processes/execution.rs:149`.
- **⚠ Stands.** The `128 + signal` exit-code rule is kiru
  (`std/process/process.kiru`, `wait_for`); the stop protocol (SIGTERM then
  SIGKILL) is Rust policy. `runtime/processes/execution.rs`.
- **⚠ Stands.** `report_error` renders a runtime failure itself.
  `runtime.rs:32`.
- **⚠ Stands.** VM-side `exit_code`, `call_entry`'s arity special case,
  `index_failure`, and the runtime arity re-check.
  `runtime/virtual_machine.rs:74`, `:112`, `:484`;
  `runtime/virtual_machine/native_dispatch.rs:25`.
- **✅ Closed.** No `NativeType`. `native_registry.rs`.
- **⚠ Stands.** The checker implements library semantics in Rust: operator
  typing, the `@()` displayable set, pattern atoms and equality, boolean
  coverage, match type and exhaustiveness, record field access.
  `validate_program/expression_typing.rs`, `match_rules.rs`, `statement_check.rs`.
- **⚠ Stands.** Lowering implements formatting and loops: interpolation to
  `ToText`/`Concat`, `for` to a jump-and-test loop.
  `lower_expressions.rs`; `bytecode/instruction.rs:143`, `:157`.
- **⚠ Stands (renamed).** `ToText` and `Concat` remain library operations as
  instructions. `IncrementNumber`, `JumpIfIndexAtEnd`, and `SetIndex` are gone
  with the index brackets. `bytecode/instruction.rs`.

## 6. Duplicated mechanisms — one job, N implementations

- **✅ Closed.** One body checker: `verify_function` is a thin wrapper that
  builds the `Checker` and calls `check_function`; the closure body goes through
  the same checker. `validate_program/validate_declarations.rs:42-59`;
  `statement_check.rs:301`.
- **⚠ Stands.** One call checker plus the arity check inside it.
  `validate_program/expression_typing.rs`.
- **⚠ Stands.** Field lookup is one `rec_field` helper plus the checker's
  visibility check. `model/program/fields.rs`; `statement_check.rs:110`.
- **✅ Closed.** One assignment-target check, `assignment_target`, for the bare
  and field forms. `validate_program/statement_check.rs:272`.
- **⚠ Stands.** The duplicate-name rules: the name pool, the method/associated
  pools, and duplicate fields. Each guards a different table.
  `resolve_names/builder/registration.rs:224`, `:407`.
- **⚠ Stands.** Equality has an engine copy per representation (values, VM,
  instruction compare). `runtime/values.rs`; `runtime/virtual_machine.rs`;
  `bytecode/instruction.rs:129`.
- **⚠ Stands.** Exhaustiveness is one function, `is_exhaustive`, read by the
  match checker. `validate_program/match_rules.rs:19`.
- **✅ Closed.** One store for a binding's type: the declaration node.
  `model/declarations.rs:25`.
- **✅ Closed.** One name-resolution mechanism: the namespace tables.
- **✅ Closed.** One ordering mechanism: none. Resolution is order-free.
- **✅ Closed.** The duplicate-field contradiction is gone: a repeated field is
  always an error and there is no collapsing pass.
  `validate_program/expression_typing.rs:50`.
- **✅ Closed.** Container access has one representation; the index forms are
  gone. `model/expressions.rs`; `bytecode/instruction.rs`.
- **⚠ Stands.** "No value" has one marker (`Value::Nothing`) and the `none`
  variant of the absence; the two are the same representation by design.
  `runtime/values.rs:36`.
- **✅ Closed.** No `DeclarationKind` dispatch; there is no kind to dispatch on.
- **⚠ Stands.** Two path→namespace walks remain — types and values — because a
  type name and a value name resolve in different pools. `model/program/namespace_paths.rs`;
  `resolve_names/name_resolution.rs`.
- **✅ Closed.** One type resolver: `resolve_type_expr` →
  `Builder::resolve_named_type`. `resolve_names/builder/type_resolution.rs:31`, `:94`.
- **✅ Closed.** One parse of `Name<...>`, `parse_type_expr`.
  `syntax/parser/declarations.rs:336`.

## 7. Structural and compiled-in assumptions

- **⚠ Stands.** `Origin::{Embedded, File}` and the reserved-`std` rule keyed on
  origin. `compiler/load_files/loaded_program.rs`; `register_declarations.rs:66-75`.
- **✅ Closed.** Registration is two passes: reserve every type and record every
  `use`, then register declarations. This is what makes resolution order-free.
  `resolve_names/register_declarations.rs:17-56`.
- **⚠ Stands (by definition).** The entry is found by name.
  `resolve_names/select_entry.rs:15`.
- **⚠ Stands (by design).** `Bytecode.module_values` is a separate code list.
  `bytecode/instruction.rs:14-16`.
- **⚠ Stands.** `BUILTIN_NAMESPACE = ["std"]` couples the registry to a
  namespace. `native_registry.rs:30`.
- **✅ Closed.** No import machinery: no duplicate-import or import-cycle errors,
  no `Item::Import`, no import file.
- **⚠ Stands.** The embedded stdlib list is compiled into Rust.
  `compiler/load_files/loader/embedded.rs:8`.

## 8. The group graph against the model

The type model is now the group graph, so the audit checks it against the model
directly.

- **✅ Closed.** Every fact is declared once, as one match over `Group` with an
  arm for every group and no wildcard: `names`, `arity`, `binds_methods`,
  `has_fields`, `has_variants`, `shape`, `substitution`, `unification`,
  `runtime_shape`. `model/types/group.rs:16`, `:120-219`.
- **✅ Closed.** The marking is total and wildcard-free: `Type::group` maps
  `Text`/`Bool`/`Int` to `Scalar`, `Named` to its declaration's group, `Function`
  to `Function`, `Parameter` to `Parameter`. `model/types.rs:76-83`.
- **✅ Closed.** The relations read the group: `substitute`, `unify`, `fits`,
  `collect_type_parameters`, and `head` all branch on
  `group(types).substitution()` / `.unification()` / `.group()`, never on a type's
  own variants to decide a kind. `model/types/relation.rs:26-240`;
  `model/types.rs:89`.
- **✅ Closed.** A declaration's kind becomes a group in exactly one place:
  `TypeDecl::group`. `model/types/group.rs:225-231`.
- **✅ Closed.** The scalar names live in one table, with the written word and
  the displayed word together, and `scalar_from_name`/`display_name` read it.
  `model/types/group.rs:38-54`, `:236-252`.
- **⚠ Stands (one deviation).** A native is bound to a type by its **first
  parameter** *or*, when it has none, by the head of its **return type**:
  `register_builtins.rs:53-65` files a no-parameter native that returns a
  `Named` type as an associated function of that type (this is what makes
  `Vec.new()` reachable as `Vec.new`). The model says "a native is bound to a
  type only when its first parameter marks it so"; the return-type binding is an
  extension the model does not state.
- **⚠ Stands (one deviation).** A bound function **is** reachable by a
  namespace-qualified path. `resolve_call` reaches a method when a qualified
  path's prefix names a namespace and the first argument's type head selects the
  name: `std::dynamic::sort(xs)` resolves. `resolve_names/name_resolution.rs:57-78`.
  The model says a bound function is "never reached by a namespace qualified
  path"; the resolver and the stdlib both use the qualified form.
- **⚠ Stands (minor).** The structural verifier `check_type` matches the `Type`
  representation with a `_ => {}` wildcard, so a new type variant would be
  silently skipped there rather than fail to compile. This is a recursion over
  the representation, not a kind decision, but it is the one wildcard the model's
  "no wildcard arm" rule would remove. `model/verify_graph.rs:132-154`.

## Clean under the model

- `src/syntax/span.rs`, `src/compiler/model/ids.rs`,
  `src/compiler/load_files/load_error.rs` — pure data, no rules.
- `src/compiler/model/visitors.rs`, `src/compiler/bytecode/validation.rs`,
  `src/compiler/prune_unreachable/tests.rs` — mechanical, no model logic.
- `src/compiler/validate_program/flow.rs` — a control-flow outcome, not a type
  case.
- `std/fs/path.kiru`, `std/fs/fs.kiru`, `std/std.kiru` — clean kiru.
