# Architecture audit — every deviation from the model

The model being enforced:

- **Types only.** The operations are reference, assignment, declaration
  (`name: Type = value`).
- **A function is a value whose type is a function type.** Being a function is
  not a different kind of thing.
- **Same type = same value.** Origin (literal, call, method, field, function) is
  irrelevant; only the type is real.
- No implicit defaults. No contextual keywords. No per-kind special cases.
- **The syscall floor is the only Rust that must exist**; everything above it is
  kiru.

Produced by five parallel audits (type system/checker, resolver, syntax front
end, runtime/natives/stdlib, model/lowering/bytecode/pipeline). Nothing in the
repo was modified. ~150 distinct findings. Grouped by root cause; each item is
`what — where`.

> **Status.** The type-name work — every written type is one `TypeExpr::Named`
> resolved through `Builder::resolve_named_type`; a built-in is a bodyless
> `pub type Name<...>;` declaration in a `std` category
> (`TypeDeclarationKind::Builtin` → `TypeKind::Builtin(BuiltinType)`); and there
> is no prelude — closes the built-in, keyword, and prelude findings below.
> Those items are marked ✅; the rest still stand.

---

## 1. A function is a KIND, not a value

The largest cluster. Every one of these exists because "function" is treated as
a kind rather than a type.

> **Status (after `32b110f`).** Closed by earlier work: the `DeclarationKind`
> enum, the two expression nodes for one function type, the two name tables and
> `Registry::other`, the registry-parameterized `resolve`, `unknown_name`'s other
> pool, `context.current_is_function`, `function_used_as_value`, the
> `Function|Native` call restriction, `Function.is_method`, the per-kind lowering
> branches, `assign_codes`/`has_code`, and natives registering with no value or
> type. Consolidated here: the self-reference bans (one `Builder::check_earlier`)
> and `Skeleton.order`/`Pending.order` (one `Skeleton.order`). What remains is a
> different job in each case: `Expression::Function`/`Closure` is the deliberate
> closure form lowering into one node; `receiver_type` is a link-time dispatch
> approximation that runs before the checker exists and is re-verified by it;
> `CallNative`/`CallValue` and `AsyncNative`/`AsyncValue` are native-by-index vs
> function-value (a native has no value form); `LoadGlobal`/`LoadLocal`/
> `LoadConst` are three address spaces (global table, frame register, constant
> pool); `EdgeCheck` is a structural invariant at the decode boundary, not a
> user-facing diagnostic; the entry's shape rules are the entry's definition.

- `Declaration::Function` is a separate node from `Declaration::Binding` —
  syntax/ast.rs:44 vs :161.
- `Expression::Function` and `Expression::Closure` are two value nodes for one
  function type — syntax/ast.rs:356,359; model/expressions.rs:42-58.
- `fn` has three grammars selected by position: a type, a declaration, a
  closure — parser/declarations.rs:108,195,222; `parse_closure` duplicates
  `parse_function` — :222-240.
- `DeclarationKind::{Function, Value, Binding, Native}` — model/declarations.rs:81-93.
- ✅ `Function { is_method, is_closure, captures }` — model/declarations.rs:144-151.
  (`is_method` is deleted with the method table; `is_closure` remains.)
- `Declaration::function()/initializer()` branch on kind — model/declarations.rs:31-68.
- Two name tables per namespace plus `Registry::{Value, Function}` and
  `Registry::other()` — model/program.rs:136-193.
- `resolve(..., registry)` parameterizes every lookup by kind —
  reference_resolution.rs:21; `search`/`registry_lookup` per pool — :65-94,119-127.
- `unknown_name` searches the *other* pool only to pick a message ("is a
  function; call it" vs "is a value and cannot be called") —
  reference_resolution.rs:96-117; diagnostics.rs:80-87.
- A `Name` resolves value-first then function, producing `Expression::Function`
  on the fallback — linking.rs:507-524; a `Call` resolves
  function-first then value — :525-565.
- `context.current_is_function` exists only to select a self-reference message —
  scopes.rs:14; linking.rs:69,120,652.
- `function_used_as_value` rejects a function referenced as a value —
  walk.rs:737-747; the "function used as a value" rule is the canonical
  violation.
- A generic function may not be a value — walk.rs:294-319.
- `call` accepts only `DeclarationKind::Function | Native`; anything else is
  `value_called` — walk.rs:749-764.
- ✅ A method was a function whose first parameter is *named* `self` —
  register_declarations/files.rs (a contextual keyword by name). A function
  whose first parameter accepts the receiver is now a method, whatever the name.
- ✅ The method table is a second registry keyed by receiver head —
  builder.rs; the function is an ordinary name in its namespace, and the
  receiver's type head selects among the namespaces in scope — linking.rs.
- ✅ `Function.is_method` in the model, and `verify.rs` exempted methods
  from the ordering rule. Both are deleted; a method call is an ordinary call
  and must name an earlier declaration.
- The entry is `main` by name in the root namespace — entry.rs:16-74;
  model.rs:36 (`ENTRY_FUNCTION`); verify.rs:34-43 re-asserts it.
- Entry shape rules by syntax shape: "at most one parameter", "must be declared
  `vec<txt>`", "returns int or nothing" — entry.rs:44-72.
- "A function cannot reference itself" / "a value cannot reference itself", and
  the forward-reference ban — reference_resolution.rs:43-58.
- `Skeleton.order` / `Pending.order` declaration-order bookkeeping —
  builder.rs:35,172-193; files.rs:104,169.
- `EdgeCheck` order rules with a method exemption — model/verify.rs:226-288.
- `Expression::Call` vs `Expression::CallValue`; `Value::Function { captures }`;
  `CallFunction`/`CallNative`/`CallValue` — model/expressions.rs:47-52,71-76;
  values.rs:15-20; bytecode/instruction.rs:175-184.
- Lowering branches by declaration kind (`Binding`→`LoadLocal`,
  `Value`→`LoadGlobal`, `Function|Native`→`unreachable!("a function is not a
  value")`) — lower_bytecode.rs:283-301; calls — :522-546; function vs closure —
  :339-367; `assign_codes`/`has_code` — :65-86.
- Natives register in the *functions* table only, with no value and no type —
  register_declarations/builtins.rs:33-37; `DeclarationKind::Native` —
  model/declarations.rs:92.
- `AsyncNative`/`Async`/`AsyncValue` — three spawn instructions —
  bytecode/instruction.rs:197-209; lower_bytecode.rs:548-586.
- `LoadGlobal`/`LoadLocal`/`LoadConst` split a reference by storage kind —
  instruction.rs:34-45.
- ✅ stdlib wrappers that existed only because a native could not be a method —
  stdlib/text.kiru `len`/`trim`/`split`/`byte` are deleted; the kernel natives
  are the methods now. `std::vec::sort`/`text_less` remain free functions
  because they apply to `vec<txt>` alone.

## 2. Type-relation pluralism — one job, many relations

> **Status (after `32b110f`).** `require` is a one-line wrapper over the one
> relation, `expression`. Consolidated here: the function-type return accessor
> (`Type::return_type`), which three call sites had implemented by hand.
> `Type::Void` is gone with the absence work. What remains is not duplication:
> `receiver_type` runs before the checker exists, which then re-verifies the
> desugared call — there is no phase in which it could call "the one inference";
> `has_parameter` answers "does this type contain a type parameter?", which none
> of `fits`/`unify`/`head` answer, and it preserves two different pinned
> diagnostics; `fits`/`unify`/`head`/`substitute`/`check_type` produce different
> outputs (equality, inference, dispatch head, substitution, id validation), so
> they are different jobs; `Type::Map`'s key is part of type identity
> (`map<int,txt>` differs from `map<txt,txt>`) and is blocked by
> `native_registry`'s `Record` row.

- `fits` is equality — types.rs:118.
- `TypeHead` / `Type::head()` is a second, weaker match used for dispatch —
  types.rs:125-157.
- `unify` is a third relation (inference) — types.rs:176-236.
- `has_parameter` is a fourth structural traversal — types.rs:158-170.
- `require` is a six-branch per-expression-kind ladder, and is the *real*
  compatibility point — walk.rs:879-953.
- `receiver_type` re-implements type inference in the resolver —
  linking.rs:714-771 (fabricates `Type::Function`, guesses `vec<txt>`).
- A match's type is inferred by a common-type join — walk.rs:1053-1120.
- Contextual typing (`expression()` vs `require()`) is a second typing
  mechanism — walk.rs:521-524,525-542.
- `Type::Void` is a kind marker driving control-flow rules —
  types.rs:24; walk.rs:63,182-198,481-497,1046-1052; validate_declarations.rs:63.
- `Type::Map(key, value)` carries a key type nothing uses —
  types.rs:18-20; walk.rs:407,598.

## 3. Keywords and forms that exist for a KIND

- Reserved words that name a kind rather than a type: `fn`, `rec`, `enum`,
  `let`, `match`, `return`, `panic`, `async`, `wait`, `for`, `in`, `break`,
  `true`, `false` — lexer.rs:219-239; token.rs.
- ✅ Closed: `vec` was reserved while `map` was contextual — two mechanisms for
  one concept. Both are ordinary names now; a built-in type is a bodyless
  `type Name<...>;` declaration and every written type is one `TypeExpr::Named`.
- ✅ `self` was a contextual keyword by parameter name —
  register_declarations/files.rs.
- `_` is recognized by lookahead (`Ident("_")` + `=>`) — statements.rs:239-241.
- `parse_match(value: bool)` makes arms expression-or-block by position —
  statements.rs:186-236; `MatchBody::Expression | Block` — syntax/ast.rs:258-262.
- Statement grammar is a per-keyword dispatch plus three shape guards —
  statements.rs:30-63.
- Three assignment statement forms (bare, field, index) —
  syntax/ast.rs:182-204; three lookahead guards — statements.rs:44-56.
- `fn(T)` without `->` silently means `fn(T) -> void` —
  parser/declarations.rs:118-127.
- `<` is both a type-argument bracket and comparison, disambiguated by position —
  parser/declarations.rs:305.
- The brace lookahead for a bare `{ ... }` subject — statements.rs:104,188-191.
- `TokenKind::DotDot` is dead (no production consumes it) — token.rs:40-41.

## 4. Implicit defaults

- `Void`/`Nothing` as "no value" — types.rs:24; values.rs:26; every register and
  global seeded with `Nothing` — virtual_machine.rs:44,120; testing.rs:22.
- A missing field/key reads as `""` — values.rs:68-73 (`get_text`);
  virtual_machine.rs:383-401 (`GetField`).
- `env::var` collapses unset and empty into `""` — natives/environment.rs:7.
- An empty `vec` literal defaults its element type to text — walk.rs:551-566.
- `NativeType::Vec` → `vec<txt>` and `NativeType::Record` → `map<txt,txt>` —
  native_registry.rs:38-40.
- A `for` item binding is hard-coded `Type::Text` at link time, then overridden —
  linking.rs:339; walk.rs:36-42.
- `fn` with no return arrow defaults to `Void` — linking.rs:97-100,674-677.
- `main` with no return defaults to exit `0` — virtual_machine.rs:70-75.
- `lower_async` has `_ => {}` and silently drops a non-call —
  lower_bytecode.rs:584.
- `cli` assigns a default `no_action` — stdlib/cli.kiru:48-62; stream modes
  `""`/`"null"` and `takes`/`fallback` — stdlib/process.kiru:11-17;
  stdlib/cli.kiru:66-85.

## 5. Rust library logic above the syscall floor

- ✅ `trim`/`split` were Unicode *policy* (`str::trim`/`str::split`), not the
  floor — they are kiru over `byte`/`find_byte`/`slice` now, ASCII-only, and a
  degenerate empty needle is an error.
- ✅ `glob` was a Rust crate, not a syscall — it is kiru over the new `list`
  (`readdir`) now.
- The text kernel is byte access: `len`, `byte`, `find_byte`, `slice` —
  runtime/natives/text_operations.rs; rows — native_registry.rs.
- ✅ `temp_file` no longer calls `std::env::temp_dir()` in Rust. The native is
  `mkstemp` over a template, and `std::fs::temp_file` builds the default
  template from `std::env::temp_dir()` in kiru.
- ✅ `parse_signal` name→signal table: `std::process::signal_number` in kiru; the
  native `signal_group` takes the number.
- ✅ `parse_pid` parsing: `std::process::pid_of` in kiru; the native `reap` takes
  an int.
- ✅ `parse_whole_number` parsing (`sleep`'s text seconds):
  `std::time::sleep` uses `std::text::whole` in kiru; the native `sleep_seconds`
  takes an int.
- `command_for` interprets the spec's `Dir`/`Stdin`/`Stdout`/`Stderr` keys in
  Rust — execution.rs:160-187; `stdio_mode`'s implicit default — :189-197.
- `exit_code` (128+signal) shell policy — execution.rs:151-158;
  `wait_for_stop` (SIGTERM then SIGKILL) policy — :122-149.
- `report_error` re-implements stdlib/std.kiru's `emit` formatting —
  runtime.rs:28-35.
- VM-side: `exit_code` — virtual_machine.rs:70-75; `call_entry`'s arity special
  case — :106-112; `index_failure` — :561-567; runtime arity re-check —
  native_dispatch.rs:25-27.
- `NativeType` is a second, monomorphic type universe —
  native_registry.rs:24-45.
- The checker implements library semantics in Rust: operator typing —
  walk.rs:651-719; the `@()` displayable set — :567-573,721-735; pattern atoms,
  equality, and duplicate collapsing — patterns.rs:14-136; boolean coverage —
  :31-46; match type and exhaustiveness — walk.rs:1053-1160; record field
  access/assignment — :382-467,590-641.
- Lowering implements formatting and iteration: interpolation →
  `ToText`/`Concat` — lower_bytecode.rs:481-520; `for` → an index loop —
  :696-735; `for {}` + a break-patch stack — :739-752,214-221.
- `Instruction::ToText`/`Concat`/`IncrementNumber`/`JumpIfIndexAtEnd` are
  library operations as instructions — instruction.rs:156-174,110-113,103-109.

## 6. Duplicated mechanisms — one job, N implementations

- Two body checkers: `verify_function` vs `closure_body` —
  validate_declarations.rs:22 vs walk.rs:324-352 (and a duplicated message —
  validate_declarations.rs:70-73 / walk.rs:345-349).
- Two call checkers plus a third arity check — walk.rs:749-813,815-852,854-872.
- Three field lookups — walk.rs:416-419,603-608; linking.rs:796-811.
- Three assignment-target kind checks — walk.rs:354-378,394-399,447-454.
- Three duplicate-name rules — builder.rs:186-192 (`declare`),
  linking.rs:175-177 (`declare_local`), builder.rs:255-262,276-283 (`fill_type`).
- Three equality implementations — values.rs:40-48; virtual_machine.rs:443-453;
  instruction.rs:142-147.
- Three exhaustiveness mechanisms — walk.rs:53-62; patterns.rs:31-46;
  walk.rs:1150-1160.
- Two stores for a binding's type (`overrides` vs the node) —
  walk.rs:36-42,103-112; linking.rs:339.
- ✅ Two name-resolution mechanisms — namespaces vs the method table.
- ✅ Two ordering mechanisms — skeleton order vs method order.
- **Contradiction:** `reject_duplicate_fields` forbids repeats —
  walk.rs:501-512; `normalized_fields` collapses repeats to the last value —
  patterns.rs:110-122.
- Six representations of container access — ast.rs:182-204;
  model/expressions.rs:77-88; instruction.rs:160-169,60-64,83-89.
- Four representations of "no value" — types.rs:24; values.rs:26;
  expressions.rs:184-189; instruction.rs:185-187.
- `DeclarationKind` dispatch copy-pasted at ~8 sites — lower_bytecode.rs:67-86,
  95-101,286-300,528-545,558-570; verify.rs:82-97,148-155; mark_reachable.rs:26-30;
  walk.rs:738-763.
- Two path→namespace walks (types vs values) — builder.rs:425-450 vs
  reference_resolution.rs:75-93.
- ✅ Closed: `resolve_type_expr` was a whole second resolver. It is now the one
  path — `resolve_type_expr` → `Builder::resolve_named_type` — for every written
  name.
- ✅ Closed: two ways to parse `Name<...>` as a type and as a value with
  arguments. The type position is one `parse_type_expr`; the built-in and `vec`
  special forms are gone.

## 7. Structural and compiled-in assumptions

- ✅ Closed: the stdlib was seeded before the entry "so its declarations come
  first", which made its names a prelude. Seeding remains for load order, but it
  grants no visibility: `find_type` reads only the current namespace and the
  namespaces it `use`s, so a program imports what it writes.
- `Origin::{Embedded, File}` and the reserved-`std` rule keyed on origin —
  model/program.rs:116-122; register_declarations/files.rs:48-56.
- Single-pass registration, which is what forces the `order` rules —
  builder.rs:172-193; files.rs:25-35; linking.rs:43-44.
- The entry is found by name — model.rs:36; entry.rs:20.
- `Bytecode.module_values` is a separate code list from the entry —
  instruction.rs:14-16; virtual_machine.rs:97-103.
- `BUILTIN_NAMESPACE = ["std"]` couples the registry to a namespace —
  native_registry.rs:18-19; files.rs:48-56.
- Duplicate-import and import-cycle errors are file-identity rules —
  read_entry_and_imports.rs:131-146; loaded_program.rs:1-7.
- `push_item` silently drops `Item::Import` — read_entry_and_imports.rs:102-113.
- The `EMBEDDED` stdlib list is compiled into Rust — :22-38.

## Clean under the model

- `src/syntax/span.rs`, `src/model/ids.rs`, `src/compiler/load_files/load_error.rs`
  — pure data, no rules.
- `src/model/visitors.rs`, `src/bytecode/validation.rs`,
  `src/compiler/prune_unreachable/tests.rs` — mechanical, no model logic.
- `src/compiler/validate_program/validate_bodies/flow.rs` — a control-flow
  outcome, not a type case.
- `stdlib/path.kiru`, `stdlib/fs.kiru`, `stdlib/option.kiru` — clean kiru.
