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
use crate::compiler::model::{DeclarationId, DeclarationKind, Program, join_path, namespace_path};

/// The warnings for the program declarations no reachable body reads. Only a
/// namespace-level function and a module value are named: a filed function
/// belongs to its type, a type, local, and native are reached as a whole or
/// not at all, and the embedded library's own declarations are never the
/// program's to prune.
pub(crate) fn unused_declarations(
    program: &Program,
    reachable: &HashSet<DeclarationId>,
) -> Vec<Diagnostic> {
    let mut warnings: Vec<Diagnostic> = program
        .declarations
        .iter()
        .enumerate()
        .filter(|(index, _)| !reachable.contains(&DeclarationId(*index)))
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
            let path = &program.files[file.0].path;
            if !is_program_source(path) {
                return None;
            }
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

/// Whether a file is the program's own. The embedded library's files name no
/// disk path, so a file with an absolute path is the program's and a warning
/// about the library is the compiler's, not the program's.
pub(crate) fn is_program_source(path: &std::path::Path) -> bool {
    path.is_absolute()
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
        let (program, entry) = checked_program_and_entry(source);
        let reachable = mark_reachable(&program, entry);
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
    fn the_librarys_files_are_not_the_programs() {
        assert!(is_program_source(std::path::Path::new(
            "/program/main.kiru"
        )));
        assert!(!is_program_source(std::path::Path::new(
            "<std>/dynamic/vec.kiru"
        )));
    }

    #[test]
    fn a_library_declaration_is_silent() {
        let (program, entry) = checked_program_and_entry("fn main() {};");
        let reachable = mark_reachable(&program, entry);
        let unreached_library_function =
            program
                .declarations
                .iter()
                .enumerate()
                .any(|(index, declaration)| {
                    declaration.kind == DeclarationKind::Function
                        && !reachable.contains(&DeclarationId(index))
                        && declaration.file.is_some_and(|file| {
                            program.files[file.0]
                                .path
                                .to_string_lossy()
                                .starts_with("<std>/")
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
