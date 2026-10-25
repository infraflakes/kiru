//! The reachability view: the graph keeps every declaration, and lowering
//! reads the set the entry reaches.

use std::collections::HashSet;

use crate::compiler::graph::{DeclarationId, Facts, Program};

/// Prove the reachable set is closed before lowering reads it. The graph is
/// not rewritten: every declaration keeps its id, and the set is the only
/// filter. A debug build also proves the relations at the pruned boundary.
pub(crate) fn verify_reachable(
    program: &Program,
    facts: &Facts,
    entry: DeclarationId,
    reachable: &HashSet<DeclarationId>,
) {
    #[cfg(debug_assertions)]
    {
        let violations = crate::compiler::verify_reachability(program, facts, entry, reachable);
        debug_assert!(
            violations.is_empty(),
            "the reachable set is not closed before lowering: {violations:?}"
        );
        crate::compiler::assert_relations(
            program,
            facts,
            crate::compiler::RelationBoundary::Pruned,
        );
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = (program, facts, entry, reachable);
    }
}
