//! The relational verifier over the linked program model.
//!
//! The structural verifier proves that every id points at a node of the right
//! table. This verifier proves the relations between those nodes: the namespace
//! tree, the name and type registries, the method pools, file membership,
//! ownership, visibility, type arity, module values, deliveries, and the
//! closure of the reachable set. It runs after resolution, after checking, and
//! after pruning, and it reports every violation instead of stopping at the
//! first one.
//!
//! A violation is a compiler bug unless it is one of the two known graph
//! defects the migration ledger names. The boundary decides which checks are
//! meaningful: a method family is a linked fact before the check and a defect
//! after it, and a delivery target is valid before pruning and not remapped
//! after it. The ledger is deliberately small: a new entry needs the owner's
//! decision, never a delegate's.

use std::collections::HashSet;

use crate::compiler::model::{
    DeclarationId, DeclarationKind, Expression, FileId, Function, FunctionBinding, NamespaceId,
    Pattern, Program, Statement, Symbol, Type, TypeDecl, TypeHead, TypeId, TypeKind, UseTarget,
    Visitor, reachable, walk_expression, walk_statements,
};
use crate::syntax::Span;

/// The stage after which the relations are checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RelationBoundary {
    /// Every name and body is linked; method families are still unresolved.
    Resolved,
    /// Every body is checked; no method family remains.
    Checked,
    /// The reachable declarations are compacted and every id is remapped.
    Pruned,
}

/// One relation the program fails to satisfy.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RelationViolation {
    pub(crate) kind: RelationViolationKind,
    pub(crate) detail: String,
}

/// Every distinct way a relation can fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RelationViolationKind {
    /// The namespace table has no root.
    MissingRootNamespace,
    /// The root namespace names a parent.
    RootHasParent,
    /// A non-root namespace names no parent.
    NamespaceParentMissing,
    /// A namespace parent id points past the namespace table.
    NamespaceParentOutOfRange,
    /// A namespace child id points past the namespace table.
    NamespaceChildOutOfRange,
    /// A child's parent is not the namespace that lists it.
    NamespaceChildParentMismatch,
    /// A namespace is not listed by its parent under its own name.
    NamespaceNotListedByParent,
    /// A parent lists a child under a symbol that is not the child's name.
    NamespaceChildSymbolMismatch,
    /// A non-root namespace's name was never interned.
    NamespaceNameNotInterned,

    /// A name table entry points past the declaration table.
    NameDeclarationOutOfRange,
    /// A name table entry names a declaration from another namespace.
    NameDeclarationNamespaceMismatch,
    /// A name table entry names a declaration whose kind owns no name.
    NameDeclarationDoesNotOwnName,
    /// A name table key is not the symbol of the declaration's name.
    NameSymbolMismatch,
    /// A name-owning declaration outside every method pool is not in its
    /// namespace's name table.
    OwnedDeclarationMissingFromNames,

    /// A type table entry points past the type table.
    TypeEntryOutOfRange,
    /// A type table entry names a type declared in another namespace.
    TypeNamespaceMismatch,
    /// A type table key is not the symbol of the type's name.
    TypeSymbolMismatch,
    /// A file-backed type is not listed in the file's declarations. A type is
    /// not a declaration node, so this is the known stage 2 defect.
    TypeNotInFile,

    /// A method pool entry points past the declaration table.
    MethodEntryOutOfRange,
    /// A method pool entry names a declaration from another namespace.
    MethodNamespaceMismatch,
    /// A method pool key is not the symbol of the declaration's name.
    MethodSymbolMismatch,
    /// A method pool entry is neither a function with the method binding nor a
    /// native.
    MethodKindNotBound,
    /// A method pool entry's declared type is not a function type.
    MethodTypeNotFunction,
    /// A method's receiver head is not its first parameter's head.
    MethodReceiverMismatch,
    /// An associated-function pool entry points past the declaration table.
    AssociateEntryOutOfRange,
    /// An associated-function pool entry names a declaration from another
    /// namespace.
    AssociateNamespaceMismatch,
    /// An associated-function pool key is not the symbol of the declaration's
    /// name.
    AssociateSymbolMismatch,
    /// An associated-function pool entry is neither a function with the
    /// associated binding nor a native.
    AssociateKindNotBound,
    /// An associated-function pool entry's declared type is not a function
    /// type.
    AssociateTypeNotFunction,
    /// An associated function's first parameter has the pool's own head, so it
    /// is a method.
    AssociateReceiverMismatch,

    /// A file declaration list points past the declaration table.
    FileDeclarationOutOfRange,
    /// A file lists a declaration that names another file.
    FileDeclarationFileMismatch,
    /// A file lists a local binding, which no file owns.
    FileDeclarationIsLocal,
    /// A file declaration is listed by no file.
    FileDeclarationNotListed,
    /// A file declaration is listed more than once.
    FileDeclarationListedMoreThanOnce,

    /// An anonymous function names a container past the declaration table.
    AnonymousContainerOutOfRange,
    /// An anonymous function's container is neither a function nor a module
    /// value whose initializer writes the literal.
    AnonymousContainerNotOwner,

    /// A local binding names no owner.
    LocalOwnerMissing,
    /// A local binding's owner points past the declaration table.
    LocalOwnerOutOfRange,
    /// A local binding's owner is neither a function nor a module value whose
    /// initializer declares it.
    LocalOwnerNotFunctionOrValue,
    /// A local binding's owner is declared in another namespace.
    LocalOwnerNamespaceMismatch,
    /// A function parameter points past the declaration table.
    FunctionParameterOutOfRange,
    /// A function parameter is not a local binding.
    FunctionParameterNotLocal,
    /// A top-level function parameter is not owned by its function.
    FunctionParameterOwnerMismatch,
    /// A nested function parameter names no owner.
    FunctionParameterOwnerMissing,
    /// A capture points past the declaration table.
    CaptureOutOfRange,
    /// A capture is not a local binding.
    CaptureNotLocal,
    /// A capture names no owner.
    CaptureOwnerMissing,
    /// A binding statement's declaration points past the declaration table.
    BindingTargetOutOfRange,
    /// A binding statement's declaration is not a local binding.
    BindingTargetNotLocal,
    /// A binding statement's declaration names no owner.
    BindingTargetOwnerMissing,
    /// A pattern binding points past the declaration table.
    PatternBindingOutOfRange,
    /// A pattern binding is not a local binding.
    PatternBindingNotLocal,
    /// A pattern binding names no owner.
    PatternBindingOwnerMissing,

    /// A reference points past the declaration table.
    ReferenceOutOfRange,
    /// A reference names a declaration its site namespace may not reach.
    ReferenceNotReachable,

    /// A named type points past the type table.
    NamedTypeOutOfRange,
    /// A named type applies a number of arguments its declaration does not
    /// take.
    TypeArityMismatch,

    /// A module value id points past the declaration table.
    ModuleValueOutOfRange,
    /// A module value id names a declaration that is not a module value.
    ModuleValueNotModuleValue,
    /// A module value appears more than once in the evaluation order.
    ModuleValueDuplicate,
    /// A module value declaration appears in no evaluation order.
    ModuleValueMissing,

    /// A delivery target no longer satisfies its name or visibility relation.
    DeliveryMismatch,
    /// A delivery target was not remapped after pruning. This is the known
    /// stage 5 defect.
    BroughtNotRemapped,

    /// A method family survived the check.
    MethodFamilyAfterCheck,

    /// The entry id points past the declaration table.
    EntryOutOfRange,
    /// The entry is not in the reachable set.
    EntryNotReachable,
    /// A reachable id points past the declaration table.
    ReachableDeclarationOutOfRange,
    /// A reachable body references a declaration outside the reachable set.
    ReachableReferenceOutsideClosure,
    /// A reachable body binds a declaration outside the reachable set.
    ReachableBindingOutsideClosure,
    /// A reachable function parameter is outside the reachable set.
    ReachableParameterOutsideClosure,
    /// A reachable function capture is outside the reachable set.
    ReachableCaptureOutsideClosure,
    /// A reachable declaration's owner is outside the reachable set.
    ReachableOwnerOutsideClosure,
    /// A reachable anonymous function's container is outside the reachable
    /// set.
    ReachableAnonymousContainerOutsideClosure,
    /// A reachable pattern binding is outside the reachable set.
    ReachablePatternBindingOutsideClosure,
    /// A reachable local binding names no owner.
    ReachableLocalOwnerMissing,
}

/// Assert every relation at a boundary, allowing only the ledger's known
/// defects. A stray violation panics with the boundary, its kind, and its
/// detail.
pub(crate) fn assert_known_relations(program: &Program, boundary: RelationBoundary) {
    for violation in verify_relations(program, boundary) {
        debug_assert!(
            known_defect(boundary, violation.kind),
            "the program violates a relation after {boundary:?}: {:?}: {}",
            violation.kind,
            violation.detail
        );
    }
}

/// The known-defect ledger: the only violations that may survive a boundary.
fn known_defect(boundary: RelationBoundary, kind: RelationViolationKind) -> bool {
    match kind {
        // A type is not a declaration node and is not listed in
        // File.declarations. Stage 2 makes it one.
        RelationViolationKind::TypeNotInFile => true,
        // Prune does not remap Namespace.brought. Stage 5 removes the remap
        // entirely.
        RelationViolationKind::BroughtNotRemapped => boundary == RelationBoundary::Pruned,
        _ => false,
    }
}

/// Check every relation the current model can express.
pub(crate) fn verify_relations(
    program: &Program,
    boundary: RelationBoundary,
) -> Vec<RelationViolation> {
    let mut violations = Vec::new();
    check_namespace_tree(program, &mut violations);
    check_names(program, &mut violations);
    check_types(program, &mut violations);
    check_method_pools(program, &mut violations);
    check_files(program, &mut violations);
    check_anonymous_containers(program, &mut violations);
    check_owners(program, &mut violations);
    check_declaration_types(program, &mut violations);
    check_type_declarations(program, &mut violations);
    check_bodies(program, boundary, &mut violations);
    check_module_values(program, &mut violations);
    check_deliveries(program, boundary, &mut violations);
    violations.sort();
    violations
}

/// Check the pre-prune reachable closure: the entry is reachable, every
/// reachable id exists, and every declaration edge out of a reachable
/// declaration points into the reachable set.
pub(crate) fn verify_reachability(
    program: &Program,
    entry: DeclarationId,
    reachable: &HashSet<DeclarationId>,
) -> Vec<RelationViolation> {
    let mut violations = Vec::new();
    if entry.0 >= program.declarations.len() {
        violations.push(violation(
            RelationViolationKind::EntryOutOfRange,
            format!("the entry names declaration id {}", entry.0),
        ));
    } else if !reachable.contains(&entry) {
        violations.push(violation(
            RelationViolationKind::EntryNotReachable,
            format!(
                "{} is not in the reachable set",
                declaration_label(program, entry)
            ),
        ));
    }
    for id in reachable {
        if id.0 >= program.declarations.len() {
            violations.push(violation(
                RelationViolationKind::ReachableDeclarationOutOfRange,
                format!("the reachable set holds declaration id {}", id.0),
            ));
        }
    }
    for id in reachable {
        let Some(declaration) = program.declarations.get(id.0) else {
            continue;
        };
        if declaration.kind.local() && declaration.owner.is_none() {
            violations.push(violation(
                RelationViolationKind::ReachableLocalOwnerMissing,
                format!(
                    "{} is reachable but names no owner",
                    declaration_label(program, *id)
                ),
            ));
        }
        if let Some(owner) = declaration.owner {
            if owner.0 >= program.declarations.len() {
                violations.push(violation(
                    RelationViolationKind::ReachableOwnerOutsideClosure,
                    format!(
                        "{} names owner id {}, which is out of range",
                        declaration_label(program, *id),
                        owner.0
                    ),
                ));
            } else if !reachable.contains(&owner) {
                violations.push(violation(
                    RelationViolationKind::ReachableOwnerOutsideClosure,
                    format!(
                        "{} names {}, which is not reachable",
                        declaration_label(program, *id),
                        declaration_label(program, owner)
                    ),
                ));
            }
        }
        // An anonymous function is reachable only through the body that writes
        // its literal, so that container must be reachable too.
        if let Some(container) = declaration.anonymous_container() {
            if container.0 >= program.declarations.len() {
                violations.push(violation(
                    RelationViolationKind::ReachableAnonymousContainerOutsideClosure,
                    format!(
                        "{} names container id {}, which is out of range",
                        declaration_label(program, *id),
                        container.0
                    ),
                ));
            } else if !reachable.contains(&container) {
                violations.push(violation(
                    RelationViolationKind::ReachableAnonymousContainerOutsideClosure,
                    format!(
                        "{} names {}, which is not reachable",
                        declaration_label(program, *id),
                        declaration_label(program, container)
                    ),
                ));
            }
        }
        let mut collector = ReachableEdgeCheck {
            program,
            reachable,
            violations: &mut violations,
        };
        if let Some(function) = declaration.function() {
            collector.check_function_edges(function);
            walk_statements(&mut collector, &function.body);
        } else if let Some(value) = declaration.value() {
            walk_expression(&mut collector, value);
        }
    }
    violations.sort();
    violations
}

fn violation(kind: RelationViolationKind, detail: impl Into<String>) -> RelationViolation {
    RelationViolation {
        kind,
        detail: detail.into(),
    }
}

fn declaration_label(program: &Program, id: DeclarationId) -> String {
    match program.declarations.get(id.0) {
        Some(declaration) => format!("declaration `{}` (id {})", declaration.name, id.0),
        None => format!("declaration id {}", id.0),
    }
}

fn namespace_label(program: &Program, id: NamespaceId) -> String {
    match program.namespaces.get(id.0) {
        Some(_) if id.0 == 0 => "the root namespace".to_owned(),
        Some(namespace) => format!("namespace `{}` (id {})", namespace.name, id.0),
        None => format!("namespace id {}", id.0),
    }
}

fn type_label(program: &Program, id: TypeId) -> String {
    match program.types.get(id.0) {
        Some(declaration) => format!("type `{}` (id {})", declaration.name, id.0),
        None => format!("type id {}", id.0),
    }
}

fn file_label(program: &Program, id: FileId) -> String {
    match program.files.get(id.0) {
        Some(file) => format!("file `{}` (id {})", file.path.display(), id.0),
        None => format!("file id {}", id.0),
    }
}

fn symbol_label(symbol: Symbol) -> String {
    format!("symbol #{}", symbol.0)
}

fn target_label(program: &Program, target: UseTarget) -> String {
    match target {
        UseTarget::Namespace(id) => namespace_label(program, id),
        UseTarget::Type(id) => type_label(program, id),
        UseTarget::Function(id) => declaration_label(program, id),
    }
}

/// The head of a type, or `None` when the type's declaration id is out of
/// range. The arity check reports the bad id; the pool check must not panic on
/// it while deriving the receiver head.
fn safe_head(ty: &Type, types: &[TypeDecl]) -> Option<TypeHead> {
    if let Type::Named(id, _) = ty
        && id.0 >= types.len()
    {
        return None;
    }
    Some(ty.head(types))
}

fn check_namespace_tree(program: &Program, violations: &mut Vec<RelationViolation>) {
    if program.namespaces.is_empty() {
        violations.push(violation(
            RelationViolationKind::MissingRootNamespace,
            "the program has no namespace table",
        ));
        return;
    }
    let namespace_count = program.namespaces.len();
    for (index, namespace) in program.namespaces.iter().enumerate() {
        let id = NamespaceId(index);
        if index == 0 {
            if let Some(parent) = namespace.parent {
                violations.push(violation(
                    RelationViolationKind::RootHasParent,
                    format!("the root namespace names parent id {}", parent.0),
                ));
            }
        } else {
            if namespace.parent.is_none() {
                violations.push(violation(
                    RelationViolationKind::NamespaceParentMissing,
                    format!(
                        "{} is not the root and names no parent",
                        namespace_label(program, id)
                    ),
                ));
            }
            if program.intern.resolve(&namespace.name).is_none() {
                violations.push(violation(
                    RelationViolationKind::NamespaceNameNotInterned,
                    format!(
                        "{} has the uninterned name `{}`",
                        namespace_label(program, id),
                        namespace.name
                    ),
                ));
            }
        }
        if let Some(parent) = namespace.parent {
            if parent.0 >= namespace_count {
                violations.push(violation(
                    RelationViolationKind::NamespaceParentOutOfRange,
                    format!(
                        "{} names parent id {}, which is out of range",
                        namespace_label(program, id),
                        parent.0
                    ),
                ));
            } else if let Some(symbol) = program.intern.resolve(&namespace.name)
                && program.namespaces[parent.0].children.get(&symbol) != Some(&id)
            {
                violations.push(violation(
                    RelationViolationKind::NamespaceNotListedByParent,
                    format!(
                        "{} is not listed by {} under {}",
                        namespace_label(program, id),
                        namespace_label(program, parent),
                        symbol_label(symbol)
                    ),
                ));
            }
        }
        for (symbol, child) in &namespace.children {
            if child.0 >= namespace_count {
                violations.push(violation(
                    RelationViolationKind::NamespaceChildOutOfRange,
                    format!(
                        "{} lists child id {} under {}, which is out of range",
                        namespace_label(program, id),
                        child.0,
                        symbol_label(*symbol)
                    ),
                ));
                continue;
            }
            let child_namespace = &program.namespaces[child.0];
            if child_namespace.parent != Some(id) {
                violations.push(violation(
                    RelationViolationKind::NamespaceChildParentMismatch,
                    format!(
                        "{} lists {} as a child, but its parent is {:?}",
                        namespace_label(program, id),
                        namespace_label(program, *child),
                        child_namespace.parent.map(|parent| parent.0)
                    ),
                ));
            }
            if program.intern.resolve(&child_namespace.name) != Some(*symbol) {
                violations.push(violation(
                    RelationViolationKind::NamespaceChildSymbolMismatch,
                    format!(
                        "{} lists {} under {}, which is not the symbol of its name",
                        namespace_label(program, id),
                        namespace_label(program, *child),
                        symbol_label(*symbol)
                    ),
                ));
            }
        }
    }
}

fn check_names(program: &Program, violations: &mut Vec<RelationViolation>) {
    let declaration_count = program.declarations.len();
    for (namespace_index, namespace) in program.namespaces.iter().enumerate() {
        let namespace_id = NamespaceId(namespace_index);
        for (symbol, declaration_id) in namespace.names.iter_entries() {
            if declaration_id.0 >= declaration_count {
                violations.push(violation(
                    RelationViolationKind::NameDeclarationOutOfRange,
                    format!(
                        "{} registers declaration id {} under {}",
                        namespace_label(program, namespace_id),
                        declaration_id.0,
                        symbol_label(symbol)
                    ),
                ));
                continue;
            }
            let declaration = &program.declarations[declaration_id.0];
            if declaration.namespace != namespace_id {
                violations.push(violation(
                    RelationViolationKind::NameDeclarationNamespaceMismatch,
                    format!(
                        "{} registers {}, which is declared in {}",
                        namespace_label(program, namespace_id),
                        declaration_label(program, declaration_id),
                        namespace_label(program, declaration.namespace)
                    ),
                ));
            }
            if !declaration.owns_name() {
                violations.push(violation(
                    RelationViolationKind::NameDeclarationDoesNotOwnName,
                    format!(
                        "{} registers {}, whose kind {:?} owns no name",
                        namespace_label(program, namespace_id),
                        declaration_label(program, declaration_id),
                        declaration.kind
                    ),
                ));
            }
            if program.intern.resolve(&declaration.name) != Some(symbol) {
                violations.push(violation(
                    RelationViolationKind::NameSymbolMismatch,
                    format!(
                        "{} registers {} under {}, which is not the symbol of its name",
                        namespace_label(program, namespace_id),
                        declaration_label(program, declaration_id),
                        symbol_label(symbol)
                    ),
                ));
            }
        }
    }
    let filed: HashSet<DeclarationId> = program
        .namespaces
        .iter()
        .flat_map(|namespace| {
            namespace
                .methods
                .iter()
                .chain(namespace.associates.iter())
                .copied()
        })
        .collect();
    for (index, declaration) in program.declarations.iter().enumerate() {
        if !declaration.owns_name() {
            continue;
        }
        let id = DeclarationId(index);
        if filed.contains(&id) {
            continue;
        }
        let Some(namespace) = program.namespaces.get(declaration.namespace.0) else {
            continue;
        };
        let Some(symbol) = program.intern.resolve(&declaration.name) else {
            violations.push(violation(
                RelationViolationKind::OwnedDeclarationMissingFromNames,
                format!(
                    "{} owns its name, but `{}` was never interned",
                    declaration_label(program, id),
                    declaration.name
                ),
            ));
            continue;
        };
        if namespace.names.get(symbol) != Some(id) {
            violations.push(violation(
                RelationViolationKind::OwnedDeclarationMissingFromNames,
                format!(
                    "{} owns its name and is in no method pool, but {} does not register it under {}",
                    declaration_label(program, id),
                    namespace_label(program, declaration.namespace),
                    symbol_label(symbol)
                ),
            ));
        }
    }
}

fn check_types(program: &Program, violations: &mut Vec<RelationViolation>) {
    for (namespace_index, namespace) in program.namespaces.iter().enumerate() {
        let namespace_id = NamespaceId(namespace_index);
        for (symbol, type_id) in namespace.types.iter_entries() {
            if type_id.0 >= program.types.len() {
                violations.push(violation(
                    RelationViolationKind::TypeEntryOutOfRange,
                    format!(
                        "{} registers type id {} under {}",
                        namespace_label(program, namespace_id),
                        type_id.0,
                        symbol_label(symbol)
                    ),
                ));
                continue;
            }
            let declaration = &program.types[type_id.0];
            if declaration.namespace != namespace_id {
                violations.push(violation(
                    RelationViolationKind::TypeNamespaceMismatch,
                    format!(
                        "{} registers {}, which is declared in {}",
                        namespace_label(program, namespace_id),
                        type_label(program, type_id),
                        namespace_label(program, declaration.namespace)
                    ),
                ));
            }
            if program.intern.resolve(&declaration.name) != Some(symbol) {
                violations.push(violation(
                    RelationViolationKind::TypeSymbolMismatch,
                    format!(
                        "{} registers {} under {}, which is not the symbol of its name",
                        namespace_label(program, namespace_id),
                        type_label(program, type_id),
                        symbol_label(symbol)
                    ),
                ));
            }
        }
    }
    for (index, declaration) in program.types.iter().enumerate() {
        if let Some(file) = declaration.file {
            violations.push(violation(
                RelationViolationKind::TypeNotInFile,
                format!(
                    "{} is written in {}, but a type is not a declaration node and no File.declarations lists it",
                    type_label(program, TypeId(index)),
                    file_label(program, file)
                ),
            ));
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pool {
    Method,
    Associate,
}

fn check_method_pools(program: &Program, violations: &mut Vec<RelationViolation>) {
    for (namespace_index, namespace) in program.namespaces.iter().enumerate() {
        let namespace_id = NamespaceId(namespace_index);
        for (head, symbol, declaration) in namespace.methods.iter_entries() {
            check_pool_entry(
                program,
                namespace_id,
                head,
                symbol,
                declaration,
                Pool::Method,
                violations,
            );
        }
        for (head, symbol, declaration) in namespace.associates.iter_entries() {
            check_pool_entry(
                program,
                namespace_id,
                head,
                symbol,
                declaration,
                Pool::Associate,
                violations,
            );
        }
    }
}

fn check_pool_entry(
    program: &Program,
    namespace_id: NamespaceId,
    head: TypeHead,
    symbol: Symbol,
    declaration_id: DeclarationId,
    pool: Pool,
    violations: &mut Vec<RelationViolation>,
) {
    let entry_out_of_range = match pool {
        Pool::Method => RelationViolationKind::MethodEntryOutOfRange,
        Pool::Associate => RelationViolationKind::AssociateEntryOutOfRange,
    };
    if declaration_id.0 >= program.declarations.len() {
        violations.push(violation(
            entry_out_of_range,
            format!(
                "{} registers declaration id {} under head {:?} and {}",
                namespace_label(program, namespace_id),
                declaration_id.0,
                head,
                symbol_label(symbol)
            ),
        ));
        return;
    }
    let declaration = &program.declarations[declaration_id.0];
    let namespace_mismatch = match pool {
        Pool::Method => RelationViolationKind::MethodNamespaceMismatch,
        Pool::Associate => RelationViolationKind::AssociateNamespaceMismatch,
    };
    if declaration.namespace != namespace_id {
        violations.push(violation(
            namespace_mismatch,
            format!(
                "{} registers {}, which is declared in {}",
                namespace_label(program, namespace_id),
                declaration_label(program, declaration_id),
                namespace_label(program, declaration.namespace)
            ),
        ));
    }
    let symbol_mismatch = match pool {
        Pool::Method => RelationViolationKind::MethodSymbolMismatch,
        Pool::Associate => RelationViolationKind::AssociateSymbolMismatch,
    };
    if program.intern.resolve(&declaration.name) != Some(symbol) {
        violations.push(violation(
            symbol_mismatch,
            format!(
                "{} registers {} under {}, which is not the symbol of its name",
                namespace_label(program, namespace_id),
                declaration_label(program, declaration_id),
                symbol_label(symbol)
            ),
        ));
    }
    let kind_not_bound = match pool {
        Pool::Method => RelationViolationKind::MethodKindNotBound,
        Pool::Associate => RelationViolationKind::AssociateKindNotBound,
    };
    // A filed function carries the binding that files it: a method holds its
    // receiver head and an associated function holds its type head. A native
    // has no function binding; its row's signature files it instead.
    let matching_binding = match (pool, declaration.binding) {
        (Pool::Method, Some(FunctionBinding::Method { receiver })) => receiver == head,
        (Pool::Associate, Some(FunctionBinding::Associated { owner })) => owner == head,
        _ => false,
    };
    if !matching_binding && declaration.kind != DeclarationKind::Native {
        violations.push(violation(
            kind_not_bound,
            format!(
                "{} registers {}, whose binding {:?} is not filed under head {:?}",
                namespace_label(program, namespace_id),
                declaration_label(program, declaration_id),
                declaration.binding,
                head
            ),
        ));
    }
    let type_not_function = match pool {
        Pool::Method => RelationViolationKind::MethodTypeNotFunction,
        Pool::Associate => RelationViolationKind::AssociateTypeNotFunction,
    };
    let Type::Function { parameters, .. } = &declaration.ty else {
        violations.push(violation(
            type_not_function,
            format!(
                "{} registers {}, whose declared type is not a function type",
                namespace_label(program, namespace_id),
                declaration_label(program, declaration_id)
            ),
        ));
        return;
    };
    let first = parameters
        .first()
        .and_then(|parameter| safe_head(parameter, &program.types));
    match pool {
        Pool::Method if first != Some(head) => violations.push(violation(
            RelationViolationKind::MethodReceiverMismatch,
            format!(
                "{} registers {} under head {:?}, but its first parameter's head is {:?}",
                namespace_label(program, namespace_id),
                declaration_label(program, declaration_id),
                head,
                first
            ),
        )),
        Pool::Associate if first == Some(head) => violations.push(violation(
            RelationViolationKind::AssociateReceiverMismatch,
            format!(
                "{} registers {} as an associated function under head {:?}, but its first parameter has that head",
                namespace_label(program, namespace_id),
                declaration_label(program, declaration_id),
                head
            ),
        )),
        _ => {}
    }
}

fn check_files(program: &Program, violations: &mut Vec<RelationViolation>) {
    let mut listed = vec![0usize; program.declarations.len()];
    for (file_index, file) in program.files.iter().enumerate() {
        let file_id = FileId(file_index);
        for declaration_id in &file.declarations {
            if declaration_id.0 >= program.declarations.len() {
                violations.push(violation(
                    RelationViolationKind::FileDeclarationOutOfRange,
                    format!(
                        "{} lists declaration id {}, which is out of range",
                        file_label(program, file_id),
                        declaration_id.0
                    ),
                ));
                continue;
            }
            let declaration = &program.declarations[declaration_id.0];
            if declaration.file != Some(file_id) {
                violations.push(violation(
                    RelationViolationKind::FileDeclarationFileMismatch,
                    format!(
                        "{} lists {}, whose file is {}",
                        file_label(program, file_id),
                        declaration_label(program, *declaration_id),
                        declaration
                            .file
                            .map(|file| file_label(program, file))
                            .unwrap_or_else(|| "none".to_owned())
                    ),
                ));
            }
            if declaration.kind.local() {
                violations.push(violation(
                    RelationViolationKind::FileDeclarationIsLocal,
                    format!(
                        "{} lists {}, which is a local binding",
                        file_label(program, file_id),
                        declaration_label(program, *declaration_id)
                    ),
                ));
            }
            listed[declaration_id.0] += 1;
        }
    }
    for (index, declaration) in program.declarations.iter().enumerate() {
        // An anonymous function carries the file its literal is written in,
        // but its container is its binding, not the file's declaration list.
        if declaration.file.is_none()
            || declaration.kind.local()
            || declaration.is_anonymous_function()
        {
            continue;
        }
        let id = DeclarationId(index);
        match listed[index] {
            0 => violations.push(violation(
                RelationViolationKind::FileDeclarationNotListed,
                format!(
                    "{} is a file declaration, but no File.declarations lists it",
                    declaration_label(program, id)
                ),
            )),
            1 => {}
            count => violations.push(violation(
                RelationViolationKind::FileDeclarationListedMoreThanOnce,
                format!(
                    "{} is listed {count} times across File.declarations",
                    declaration_label(program, id)
                ),
            )),
        }
    }
}

/// The ownership fact: a local is owned by a function, or by the module value
/// whose initializer declares it. A named function and an anonymous function
/// both carry a body. A module value is not callable, but its initializer is
/// the body that owns the pattern bindings written in it; a closure in that
/// initializer is an anonymous function and owns its own parameters and locals.
fn owns_locals(kind: DeclarationKind) -> bool {
    (kind.callable() && kind.has_body()) || kind.module_value()
}

/// Check every anonymous function's containing edge: the container exists and
/// is a body that can write a literal.
fn check_anonymous_containers(program: &Program, violations: &mut Vec<RelationViolation>) {
    let declaration_count = program.declarations.len();
    for (index, declaration) in program.declarations.iter().enumerate() {
        let Some(container) = declaration.anonymous_container() else {
            continue;
        };
        let id = DeclarationId(index);
        if container.0 >= declaration_count {
            violations.push(violation(
                RelationViolationKind::AnonymousContainerOutOfRange,
                format!(
                    "{} names container id {}, which is out of range",
                    declaration_label(program, id),
                    container.0
                ),
            ));
            continue;
        }
        let container_declaration = &program.declarations[container.0];
        if !owns_locals(container_declaration.kind) {
            violations.push(violation(
                RelationViolationKind::AnonymousContainerNotOwner,
                format!(
                    "{} names {}, whose kind {:?} owns no body or initializer",
                    declaration_label(program, id),
                    declaration_label(program, container),
                    container_declaration.kind
                ),
            ));
        }
    }
}

fn check_owners(program: &Program, violations: &mut Vec<RelationViolation>) {
    let declaration_count = program.declarations.len();
    for (index, declaration) in program.declarations.iter().enumerate() {
        if !declaration.kind.local() {
            continue;
        }
        let id = DeclarationId(index);
        let Some(owner) = declaration.owner else {
            violations.push(violation(
                RelationViolationKind::LocalOwnerMissing,
                format!("{} names no owner", declaration_label(program, id)),
            ));
            continue;
        };
        if owner.0 >= declaration_count {
            violations.push(violation(
                RelationViolationKind::LocalOwnerOutOfRange,
                format!(
                    "{} names owner id {}, which is out of range",
                    declaration_label(program, id),
                    owner.0
                ),
            ));
            continue;
        }
        let owner_declaration = &program.declarations[owner.0];
        if !owns_locals(owner_declaration.kind) {
            violations.push(violation(
                RelationViolationKind::LocalOwnerNotFunctionOrValue,
                format!(
                    "{} names {}, whose kind {:?} is not a function or a module value",
                    declaration_label(program, id),
                    declaration_label(program, owner),
                    owner_declaration.kind
                ),
            ));
        }
        if owner_declaration.namespace != declaration.namespace {
            violations.push(violation(
                RelationViolationKind::LocalOwnerNamespaceMismatch,
                format!(
                    "{} is declared in {}, but its owner {} is declared in {}",
                    declaration_label(program, id),
                    namespace_label(program, declaration.namespace),
                    declaration_label(program, owner),
                    namespace_label(program, owner_declaration.namespace)
                ),
            ));
        }
    }
}

fn check_declaration_types(program: &Program, violations: &mut Vec<RelationViolation>) {
    for (index, declaration) in program.declarations.iter().enumerate() {
        check_type_arity(
            program,
            &declaration.ty,
            &format!(
                "{}'s declared type",
                declaration_label(program, DeclarationId(index))
            ),
            violations,
        );
    }
}

fn check_type_declarations(program: &Program, violations: &mut Vec<RelationViolation>) {
    for (index, declaration) in program.types.iter().enumerate() {
        let id = TypeId(index);
        match &declaration.kind {
            TypeKind::Rec(fields) => {
                for field in fields {
                    check_type_arity(
                        program,
                        &field.ty,
                        &format!("{} field `{}`", type_label(program, id), field.name),
                        violations,
                    );
                }
            }
            TypeKind::Enum(variants) => {
                for variant in variants {
                    for payload in &variant.payload {
                        check_type_arity(
                            program,
                            payload,
                            &format!(
                                "{} variant `{}` payload",
                                type_label(program, id),
                                variant.name
                            ),
                            violations,
                        );
                    }
                }
            }
        }
    }
}

fn check_type_arity(
    program: &Program,
    ty: &Type,
    context: &str,
    violations: &mut Vec<RelationViolation>,
) {
    match ty {
        Type::Named(id, arguments) => {
            match program.types.get(id.0) {
                Some(declaration) => {
                    if arguments.len() != declaration.parameters.len() {
                        violations.push(violation(
                            RelationViolationKind::TypeArityMismatch,
                            format!(
                                "{context} applies {} argument(s) to `{}`, which declares {}",
                                arguments.len(),
                                declaration.name,
                                declaration.parameters.len()
                            ),
                        ));
                    }
                }
                None => violations.push(violation(
                    RelationViolationKind::NamedTypeOutOfRange,
                    format!("{context} names type id {}, which is out of range", id.0),
                )),
            }
            for argument in arguments {
                check_type_arity(program, argument, context, violations);
            }
        }
        Type::Function {
            parameters,
            returns,
        } => {
            for parameter in parameters {
                check_type_arity(program, parameter, context, violations);
            }
            if let Some(returns) = returns {
                check_type_arity(program, returns, context, violations);
            }
        }
        Type::Text | Type::Bool | Type::Int | Type::Parameter(_) => {}
    }
}

fn check_bodies(
    program: &Program,
    boundary: RelationBoundary,
    violations: &mut Vec<RelationViolation>,
) {
    for (index, declaration) in program.declarations.iter().enumerate() {
        let Some(value) = declaration.value() else {
            continue;
        };
        let id = DeclarationId(index);
        let mut verifier = BodyVerifier {
            program,
            boundary,
            site_namespace: declaration.namespace,
            violations,
        };
        if declaration.kind.callable() && declaration.kind.has_body() {
            let Expression::Function(function) = value else {
                continue;
            };
            for parameter in &function.parameters {
                verifier.check_owned_parameter(id, *parameter);
            }
            for capture in &function.captures {
                verifier.check_capture(*capture);
            }
            if let Some(return_type) = &function.declared_return {
                verifier.check_type(return_type, "a function's return type");
            }
            walk_statements(&mut verifier, &function.body);
        } else {
            walk_expression(&mut verifier, value);
        }
    }
}

fn check_module_values(program: &Program, violations: &mut Vec<RelationViolation>) {
    let mut seen = HashSet::new();
    for id in &program.module_values {
        if id.0 >= program.declarations.len() {
            violations.push(violation(
                RelationViolationKind::ModuleValueOutOfRange,
                format!("the evaluation order holds declaration id {}", id.0),
            ));
            continue;
        }
        if !program.declarations[id.0].kind.module_value() {
            violations.push(violation(
                RelationViolationKind::ModuleValueNotModuleValue,
                format!(
                    "the evaluation order holds {}, which is not a module value",
                    declaration_label(program, *id)
                ),
            ));
        }
        if !seen.insert(*id) {
            violations.push(violation(
                RelationViolationKind::ModuleValueDuplicate,
                format!(
                    "{} appears more than once in the evaluation order",
                    declaration_label(program, *id)
                ),
            ));
        }
    }
    for (index, declaration) in program.declarations.iter().enumerate() {
        let id = DeclarationId(index);
        if declaration.kind.module_value() && !seen.contains(&id) {
            violations.push(violation(
                RelationViolationKind::ModuleValueMissing,
                format!(
                    "{} appears in no evaluation order",
                    declaration_label(program, id)
                ),
            ));
        }
    }
}

fn check_deliveries(
    program: &Program,
    boundary: RelationBoundary,
    violations: &mut Vec<RelationViolation>,
) {
    for (namespace_index, namespace) in program.namespaces.iter().enumerate() {
        let source = NamespaceId(namespace_index);
        for (symbol, deliveries) in &namespace.brought {
            for delivery in deliveries {
                let Some(reason) = delivery_failure(program, *symbol, delivery.target) else {
                    continue;
                };
                let kind = if boundary == RelationBoundary::Pruned {
                    RelationViolationKind::BroughtNotRemapped
                } else {
                    RelationViolationKind::DeliveryMismatch
                };
                violations.push(violation(
                    kind,
                    format!(
                        "{} delivers {} as {}, but {reason}",
                        namespace_label(program, source),
                        target_label(program, delivery.target),
                        symbol_label(*symbol)
                    ),
                ));
            }
        }
    }
}

/// The delivery relation the current model can express: the target exists,
/// its name is the delivered symbol, and a function target is useable. A
/// delivery may name a private type or function: visibility is enforced where
/// a body reads the delivered name, so an unused private `use` is legal. The
/// taken status that would make visibility a delivery fact is a stage 3 fact.
fn delivery_failure(program: &Program, symbol: Symbol, target: UseTarget) -> Option<String> {
    match target {
        UseTarget::Namespace(id) => {
            let Some(namespace) = program.namespaces.get(id.0) else {
                return Some(format!("namespace id {} is out of range", id.0));
            };
            if program.intern.resolve(&namespace.name) != Some(symbol) {
                return Some(format!(
                    "the namespace name `{}` does not resolve to the delivered symbol",
                    namespace.name
                ));
            }
            None
        }
        UseTarget::Type(id) => {
            let Some(declaration) = program.types.get(id.0) else {
                return Some(format!("type id {} is out of range", id.0));
            };
            if program.intern.resolve(&declaration.name) != Some(symbol) {
                return Some(format!(
                    "the type name `{}` does not resolve to the delivered symbol",
                    declaration.name
                ));
            }
            None
        }
        UseTarget::Function(id) => {
            let Some(declaration) = program.declarations.get(id.0) else {
                return Some(format!("declaration id {} is out of range", id.0));
            };
            if !declaration.kind.useable() {
                return Some(format!(
                    "the target's kind {:?} is not a use target",
                    declaration.kind
                ));
            }
            if program.intern.resolve(&declaration.name) != Some(symbol) {
                return Some(format!(
                    "the declaration name `{}` does not resolve to the delivered symbol",
                    declaration.name
                ));
            }
            None
        }
    }
}

/// The checks that walk one declaration's body.
struct BodyVerifier<'a> {
    program: &'a Program,
    boundary: RelationBoundary,
    site_namespace: NamespaceId,
    violations: &'a mut Vec<RelationViolation>,
}

impl BodyVerifier<'_> {
    fn check_owned_parameter(&mut self, owner: DeclarationId, parameter: DeclarationId) {
        if parameter.0 >= self.program.declarations.len() {
            self.violations.push(violation(
                RelationViolationKind::FunctionParameterOutOfRange,
                format!(
                    "{} names parameter id {}, which is out of range",
                    declaration_label(self.program, owner),
                    parameter.0
                ),
            ));
            return;
        }
        let local = &self.program.declarations[parameter.0];
        if !local.kind.local() {
            self.violations.push(violation(
                RelationViolationKind::FunctionParameterNotLocal,
                format!(
                    "{} names {}, which is not a local binding",
                    declaration_label(self.program, owner),
                    declaration_label(self.program, parameter)
                ),
            ));
        }
        if local.owner != Some(owner) {
            self.violations.push(violation(
                RelationViolationKind::FunctionParameterOwnerMismatch,
                format!(
                    "{} names {}, whose owner is {:?}",
                    declaration_label(self.program, owner),
                    declaration_label(self.program, parameter),
                    local.owner.map(|owner| owner.0)
                ),
            ));
        }
    }

    fn check_expression_parameter(&mut self, parameter: DeclarationId) {
        if parameter.0 >= self.program.declarations.len() {
            self.violations.push(violation(
                RelationViolationKind::FunctionParameterOutOfRange,
                format!(
                    "a function literal names parameter id {}, which is out of range",
                    parameter.0
                ),
            ));
            return;
        }
        let local = &self.program.declarations[parameter.0];
        if !local.kind.local() {
            self.violations.push(violation(
                RelationViolationKind::FunctionParameterNotLocal,
                format!(
                    "a function literal names {}, which is not a local binding",
                    declaration_label(self.program, parameter)
                ),
            ));
        }
        if local.owner.is_none() {
            self.violations.push(violation(
                RelationViolationKind::FunctionParameterOwnerMissing,
                format!(
                    "a function literal names {}, which has no owner",
                    declaration_label(self.program, parameter)
                ),
            ));
        }
    }

    fn check_capture(&mut self, capture: DeclarationId) {
        if capture.0 >= self.program.declarations.len() {
            self.violations.push(violation(
                RelationViolationKind::CaptureOutOfRange,
                format!(
                    "a function captures declaration id {}, which is out of range",
                    capture.0
                ),
            ));
            return;
        }
        let local = &self.program.declarations[capture.0];
        if !local.kind.local() {
            self.violations.push(violation(
                RelationViolationKind::CaptureNotLocal,
                format!(
                    "a function captures {}, which is not a local binding",
                    declaration_label(self.program, capture)
                ),
            ));
        }
        if local.owner.is_none() {
            self.violations.push(violation(
                RelationViolationKind::CaptureOwnerMissing,
                format!(
                    "a function captures {}, which has no owner",
                    declaration_label(self.program, capture)
                ),
            ));
        }
    }

    fn check_binding(&mut self, statement: &Statement, declaration: DeclarationId) {
        if declaration.0 >= self.program.declarations.len() {
            self.violations.push(violation(
                RelationViolationKind::BindingTargetOutOfRange,
                format!(
                    "a binding statement at {:?} names declaration id {}, which is out of range",
                    statement.span(),
                    declaration.0
                ),
            ));
            return;
        }
        let local = &self.program.declarations[declaration.0];
        if !local.kind.local() {
            // Boundary rule, not a ledger entry. A Bind target is a resolution
            // fact at every boundary: a local binding is the only thing a
            // `let` declares. An Assign or FieldAssign target may be a module
            // value at Resolved because the checker owns that rejection and
            // has not run yet; after the check every assignment target is a
            // local.
            let resolution_fact = matches!(statement, Statement::Bind { .. })
                || self.boundary != RelationBoundary::Resolved;
            if resolution_fact {
                self.violations.push(violation(
                    RelationViolationKind::BindingTargetNotLocal,
                    format!(
                        "a binding statement at {:?} names {}, which is not a local binding",
                        statement.span(),
                        declaration_label(self.program, declaration)
                    ),
                ));
                if local.owner.is_none() {
                    self.violations.push(violation(
                        RelationViolationKind::BindingTargetOwnerMissing,
                        format!(
                            "a binding statement at {:?} names {}, which has no owner",
                            statement.span(),
                            declaration_label(self.program, declaration)
                        ),
                    ));
                }
            }
            return;
        }
        if local.owner.is_none() {
            self.violations.push(violation(
                RelationViolationKind::BindingTargetOwnerMissing,
                format!(
                    "a binding statement at {:?} names {}, which has no owner",
                    statement.span(),
                    declaration_label(self.program, declaration)
                ),
            ));
        }
    }

    fn check_pattern_binding(&mut self, binding: DeclarationId, span: Span) {
        if binding.0 >= self.program.declarations.len() {
            self.violations.push(violation(
                RelationViolationKind::PatternBindingOutOfRange,
                format!(
                    "a pattern binding at {span:?} names declaration id {}, which is out of range",
                    binding.0
                ),
            ));
            return;
        }
        let local = &self.program.declarations[binding.0];
        if !local.kind.local() {
            self.violations.push(violation(
                RelationViolationKind::PatternBindingNotLocal,
                format!(
                    "a pattern binding at {span:?} names {}, which is not a local binding",
                    declaration_label(self.program, binding)
                ),
            ));
        }
        if local.owner.is_none() {
            self.violations.push(violation(
                RelationViolationKind::PatternBindingOwnerMissing,
                format!(
                    "a pattern binding at {span:?} names {}, which has no owner",
                    declaration_label(self.program, binding)
                ),
            ));
        }
    }

    fn check_type(&mut self, ty: &Type, context: &str) {
        check_type_arity(self.program, ty, context, self.violations);
    }
}

impl Visitor for BodyVerifier<'_> {
    fn reference(&mut self, expression: &Expression) {
        let Expression::Reference { declaration, .. } = expression else {
            unreachable!("the walk names a reference");
        };
        if declaration.0 >= self.program.declarations.len() {
            self.violations.push(violation(
                RelationViolationKind::ReferenceOutOfRange,
                format!(
                    "a reference at {:?} names declaration id {}, which is out of range",
                    expression.span(),
                    declaration.0
                ),
            ));
            return;
        }
        let target = &self.program.declarations[declaration.0];
        if !reachable(target.public, target.namespace, self.site_namespace) {
            self.violations.push(violation(
                RelationViolationKind::ReferenceNotReachable,
                format!(
                    "a reference at {:?} names {}, which {} may not reach",
                    expression.span(),
                    declaration_label(self.program, *declaration),
                    namespace_label(self.program, self.site_namespace)
                ),
            ));
        }
    }

    fn function(&mut self, expression: &Expression) {
        let Expression::Function(function) = expression else {
            unreachable!("the walk names a function");
        };
        for parameter in &function.parameters {
            self.check_expression_parameter(*parameter);
        }
        for capture in &function.captures {
            self.check_capture(*capture);
        }
        if let Some(return_type) = &function.declared_return {
            self.check_type(return_type, "a function literal's return type");
        }
    }

    fn bind(&mut self, statement: &Statement) {
        let Statement::Bind { declaration, .. } = statement else {
            unreachable!("the walk names a binding statement");
        };
        self.check_binding(statement, *declaration);
    }

    fn assign(&mut self, statement: &Statement) {
        let Statement::Assign { declaration, .. } = statement else {
            unreachable!("the walk names an assignment");
        };
        self.check_binding(statement, *declaration);
    }

    fn field_assign(&mut self, statement: &Statement) {
        let Statement::FieldAssign { declaration, .. } = statement else {
            unreachable!("the walk names a field assignment");
        };
        self.check_binding(statement, *declaration);
    }

    fn variant_pattern(&mut self, pattern: &Pattern) {
        let Pattern::Variant { bindings, span, .. } = pattern else {
            unreachable!("the walk names a variant pattern");
        };
        for binding in bindings.iter().flatten() {
            self.check_pattern_binding(*binding, *span);
        }
    }

    fn method(&mut self, expression: &Expression) {
        if self.boundary != RelationBoundary::Resolved {
            self.violations.push(violation(
                RelationViolationKind::MethodFamilyAfterCheck,
                format!(
                    "a method family at {:?} survived the check",
                    expression.span()
                ),
            ));
        }
    }

    fn text(&mut self, _: &Expression) {}
    fn map(&mut self, _: &Expression) {}
    fn boolean(&mut self, _: &Expression) {}
    fn integer(&mut self, _: &Expression) {}
    fn variant(&mut self, _: &Expression) {}
    fn interpolated(&mut self, _: &Expression) {}
    fn call(&mut self, _: &Expression) {}
    fn field(&mut self, _: &Expression) {}
    fn binary(&mut self, _: &Expression) {}
    fn unary(&mut self, _: &Expression) {}
    fn match_expression(&mut self, _: &Expression) {}
    fn expression_statement(&mut self, _: &Statement) {}
    fn return_statement(&mut self, _: &Statement) {}
    fn panic(&mut self, _: &Statement) {}
    fn asynchronous(&mut self, _: &Statement) {}
    fn wait(&mut self, _: &Statement) {}
    fn forever(&mut self, _: &Statement) {}
    fn break_statement(&mut self, _: &Statement) {}
    fn value_pattern(&mut self, _: &Pattern) {}
    fn expression_body(&mut self, _: &crate::compiler::model::MatchBody) {}
    fn block_body(&mut self, _: &crate::compiler::model::MatchBody) {}

    fn enters_deferred_body(&self) -> bool {
        true
    }
}

/// The checks that walk one reachable declaration's body.
struct ReachableEdgeCheck<'a> {
    program: &'a Program,
    reachable: &'a HashSet<DeclarationId>,
    violations: &'a mut Vec<RelationViolation>,
}

impl ReachableEdgeCheck<'_> {
    fn check_function_edges(&mut self, function: &Function) {
        for parameter in &function.parameters {
            self.check_edge(
                *parameter,
                RelationViolationKind::ReachableParameterOutsideClosure,
                "a function parameter",
            );
        }
        for capture in &function.captures {
            self.check_edge(
                *capture,
                RelationViolationKind::ReachableCaptureOutsideClosure,
                "a function capture",
            );
        }
    }

    fn check_edge(&mut self, id: DeclarationId, kind: RelationViolationKind, what: &str) {
        if id.0 >= self.program.declarations.len() {
            self.violations.push(violation(
                kind,
                format!(
                    "{what} names declaration id {}, which is out of range",
                    id.0
                ),
            ));
        } else if !self.reachable.contains(&id) {
            self.violations.push(violation(
                kind,
                format!(
                    "{what} names {}, which is not in the reachable set",
                    declaration_label(self.program, id)
                ),
            ));
        }
    }

    fn check_binding(&mut self, statement: &Statement) {
        let (Statement::Bind { declaration, .. }
        | Statement::Assign { declaration, .. }
        | Statement::FieldAssign { declaration, .. }) = statement
        else {
            unreachable!("the walk names a binding statement");
        };
        self.check_edge(
            *declaration,
            RelationViolationKind::ReachableBindingOutsideClosure,
            "a binding statement",
        );
    }
}

impl Visitor for ReachableEdgeCheck<'_> {
    fn reference(&mut self, expression: &Expression) {
        let Expression::Reference { declaration, .. } = expression else {
            unreachable!("the walk names a reference");
        };
        self.check_edge(
            *declaration,
            RelationViolationKind::ReachableReferenceOutsideClosure,
            "a reference",
        );
    }

    fn function(&mut self, expression: &Expression) {
        let Expression::Function(function) = expression else {
            unreachable!("the walk names a function");
        };
        self.check_function_edges(function);
    }

    fn bind(&mut self, statement: &Statement) {
        self.check_binding(statement);
    }
    fn assign(&mut self, statement: &Statement) {
        self.check_binding(statement);
    }
    fn field_assign(&mut self, statement: &Statement) {
        self.check_binding(statement);
    }

    fn variant_pattern(&mut self, pattern: &Pattern) {
        let Pattern::Variant { bindings, .. } = pattern else {
            unreachable!("the walk names a variant pattern");
        };
        for binding in bindings.iter().flatten() {
            self.check_edge(
                *binding,
                RelationViolationKind::ReachablePatternBindingOutsideClosure,
                "a pattern binding",
            );
        }
    }

    fn text(&mut self, _: &Expression) {}
    fn map(&mut self, _: &Expression) {}
    fn boolean(&mut self, _: &Expression) {}
    fn integer(&mut self, _: &Expression) {}
    fn variant(&mut self, _: &Expression) {}
    fn interpolated(&mut self, _: &Expression) {}
    fn call(&mut self, _: &Expression) {}
    fn method(&mut self, _: &Expression) {}
    fn field(&mut self, _: &Expression) {}
    fn binary(&mut self, _: &Expression) {}
    fn unary(&mut self, _: &Expression) {}
    fn match_expression(&mut self, _: &Expression) {}
    fn expression_statement(&mut self, _: &Statement) {}
    fn return_statement(&mut self, _: &Statement) {}
    fn panic(&mut self, _: &Statement) {}
    fn asynchronous(&mut self, _: &Statement) {}
    fn wait(&mut self, _: &Statement) {}
    fn forever(&mut self, _: &Statement) {}
    fn break_statement(&mut self, _: &Statement) {}
    fn value_pattern(&mut self, _: &Pattern) {}
    fn expression_body(&mut self, _: &crate::compiler::model::MatchBody) {}
    fn block_body(&mut self, _: &crate::compiler::model::MatchBody) {}

    fn enters_deferred_body(&self) -> bool {
        true
    }
}
