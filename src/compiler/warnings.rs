//! The warnings the checked program earns for unused syntax.
//!
//! A warning is a statement of a fact the compiler already holds: the
//! language is deliberately simple, so a warning says what is, not what to
//! do. The facts are an unused `use`, a redundant `use` (a second item that
//! delivers a target an earlier one already delivered in its namespace), an
//! unused local binding, and a program declaration no reachable body reads. The unused-`use` fact is recorded
//! where resolutions read the deliveries and is emitted beside those reads, in
//! the resolver; the unused-binding fact is the reference edges the check
//! settled, and this pass reads them; the unused-declaration fact is the
//! reachable set the prune stage marks, and this pass reads it.

use std::collections::HashSet;

use crate::compiler::Diagnostic;
use crate::compiler::model::collect_reads;
use crate::compiler::model::{
    DeclarationId, DeclarationKind, FileOrigin, FunctionBinding, Program, join_path, namespace_path,
};

/// The warnings for the program declarations no reachable body reads. Only a
/// named namespace-level function and a module value are named: an anonymous
/// function is contained by the body that writes it, a method or associated
/// function belongs to its type, a native is reached as a whole or not at all,
/// and the embedded library's own declarations are never the program's to
/// prune.
pub(crate) fn unused_declarations(
    program: &Program,
    reachable: &HashSet<DeclarationId>,
) -> Vec<Diagnostic> {
    let mut warnings: Vec<Diagnostic> = program
        .declarations
        .iter()
        .enumerate()
        .filter(|(index, _)| !reachable.contains(&DeclarationId(*index)))
        // An anonymous function is contained by the body that writes it and
        // owns no namespace name, so it is never an unused declaration.
        .filter(|(_, declaration)| !declaration.is_anonymous_function())
        // A method or associated function is filed under its type and owns no
        // namespace name, so it is never an unused declaration either.
        .filter(|(_, declaration)| {
            !declaration
                .binding
                .is_some_and(FunctionBinding::is_method_or_associated)
        })
        .filter(|(_, declaration)| match declaration.kind {
            DeclarationKind::Function | DeclarationKind::ModuleValue => true,
            DeclarationKind::Local
            | DeclarationKind::Rec
            | DeclarationKind::Enum
            | DeclarationKind::Impl
            | DeclarationKind::Native => false,
        })
        .filter_map(|(_, declaration)| {
            let file = declaration.file?;
            let source = &program.files[file.0];
            if source.origin != FileOrigin::Program {
                return None;
            }
            let path = &source.path;
            let mut qualified = namespace_path(&program.namespaces, declaration.namespace);
            qualified.push(declaration.name.clone());
            Some(Diagnostic::warning(
                path.clone(),
                declaration.name_span,
                format!("`{}` is unused", join_path(&qualified)),
            ))
        })
        .collect();
    warnings.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.span.start.cmp(&right.span.start))
    });
    warnings
}

/// The warnings for the local bindings the program declares but never reads.
/// A read is an edge: a `Reference` node the program's bodies hold. Writing
/// the binding (`x = …`) and matching it (`Name.variant(x, …)`) write the
/// name without reading it, so a binding only assigned or only matched is
/// never read. The binding's own file is the file of the body it is written
/// in, which the walk carried.
pub(crate) fn unused_bindings(program: &Program) -> Vec<Diagnostic> {
    let reads = collect_reads(program, None);
    // A method's first parameter is its receiver: the method exists because
    // the value is passed, so a body that ignores it is not an unused local.
    // Every other parameter and binding still answers the question.
    let receivers: HashSet<DeclarationId> = program
        .namespaces
        .iter()
        .flat_map(|namespace| namespace.methods.iter().copied())
        .filter_map(|method| {
            program
                .declaration(method)
                .function()?
                .parameters
                .first()
                .copied()
        })
        .collect();
    reads
        .bindings
        .iter()
        .filter(|bound| {
            !reads.referenced.contains(&bound.declaration)
                && !receivers.contains(&bound.declaration)
        })
        .map(|bound| {
            let declaration = program.declaration(bound.declaration);
            Diagnostic::warning(
                &program.files[bound.file.0].path,
                declaration.name_span,
                format!("the local `{}` is never read", declaration.name),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{checked_program_and_entry, mark_reachable};

    /// The unused-declaration warnings of a checked virtual program.
    fn warned(source: &str) -> Vec<Diagnostic> {
        let (program, facts, entry) = checked_program_and_entry(source);
        let reachable = mark_reachable(&program, &facts, entry);
        unused_declarations(&program, &reachable)
    }

    #[test]
    fn a_reachable_function_is_silent() {
        let warnings = warned("fn helper() {};\nfn main() { helper(); };");
        assert!(warnings.is_empty(), "expected nothing, found {warnings:?}");
    }

    #[test]
    fn an_unreached_function_is_warned_with_its_qualified_name() {
        let warnings = warned("fn helper() {};\nfn main() {};");
        assert_eq!(warnings.len(), 1, "one declaration is unreached");
        let warning = &warnings[0];
        assert_eq!(warning.path, std::path::PathBuf::from("/virtual/main.kiru"));
        assert_eq!(warning.severity, crate::compiler::Severity::Warning);
        assert_eq!(warning.message, "`root::helper` is unused");
    }

    #[test]
    fn both_tree_origins_are_recorded() {
        let (program, _, _) = checked_program_and_entry("fn main() {};");
        assert!(
            program
                .files
                .iter()
                .any(|file| file.origin == FileOrigin::Program),
            "the program tree records its own files"
        );
        assert!(
            program
                .files
                .iter()
                .any(|file| file.origin == FileOrigin::Embedded),
            "the embedded library records its files"
        );
    }

    #[test]
    fn a_library_declaration_is_silent() {
        let (program, facts, entry) = checked_program_and_entry("fn main() {};");
        let reachable = mark_reachable(&program, &facts, entry);
        let unreached_library_function =
            program
                .declarations
                .iter()
                .enumerate()
                .any(|(index, declaration)| {
                    declaration.kind == DeclarationKind::Function
                        && !reachable.contains(&DeclarationId(index))
                        && declaration.file.is_some_and(|file| {
                            program.files[file.0].origin == FileOrigin::Embedded
                        })
                });
        assert!(
            unreached_library_function,
            "the library holds functions the entry never reaches"
        );
        let warnings = unused_declarations(&program, &reachable);
        assert!(
            warnings.is_empty(),
            "the embedded library is never warned about: {warnings:?}"
        );
    }

    #[test]
    fn an_unreached_module_value_is_warned() {
        let warnings = warned("let greeting: txt = \"hi\";\nfn main() {};");
        assert_eq!(warnings.len(), 1, "one module value is unreached");
        assert_eq!(warnings[0].message, "`root::greeting` is unused");
    }

    #[test]
    fn a_filed_function_is_silent() {
        let warnings = warned(
            "rec Thing { value: int };\nimpl Thing { fn make() -> Thing { return { value = 1 }; }; };\nfn main() {};",
        );
        assert!(warnings.is_empty(), "expected nothing, found {warnings:?}");
    }
}
