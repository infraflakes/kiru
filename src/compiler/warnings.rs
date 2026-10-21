//! The warnings the checked program earns for unused syntax.
//!
//! A warning is a statement of a fact the compiler already holds: the
//! language is deliberately simple, so a warning says what is, not what to
//! do. The two facts are an unused `use` and an unused local binding. The
//! unused-`use` fact is recorded where resolutions read the deliveries and is
//! emitted beside those reads, in the resolver; the unused-binding fact is
//! the reference edges the check settled, and this pass reads them.

use crate::compiler::Diagnostic;
use crate::compiler::model::Program;
use crate::compiler::reads::collect_reads;

/// The warnings for the local bindings the program declares but never reads.
/// A read is an edge: a `Reference` node the program's bodies hold. Writing
/// the binding (`x = …`) and matching it (`Name.variant(x, …)`) write the
/// name without reading it, so a binding only assigned or only matched is
/// never read. The binding's own file is the file of the body it is written
/// in, which the walk carried.
pub(crate) fn unused_bindings(program: &Program) -> Vec<Diagnostic> {
    let reads = collect_reads(program);
    reads
        .bindings
        .iter()
        .filter(|bound| !reads.referenced.contains(&bound.declaration))
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
