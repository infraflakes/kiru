//! IR builder: consumes the accumulated `CompileState` and produces the
//! final [`Ir`] by lowering every run body with the universal body compiler.
//! Run bodies are no different from function bodies: every statement becomes
//! a display row, calls are flattened, and blocks indent their bodies.

use crate::ir::Ir;
use std::collections::BTreeMap;

use super::inline::{FnResolver, compile_fn_stmts};
use super::{CompileError, CompileState, PendingRunBlock};

pub(super) fn build_ir(state: CompileState) -> Result<Ir, CompileError> {
    let CompileState {
        globals,
        functions,
        run_blocks,
        source_texts,
        loaded_files: _,
        recursion_stack: _,
    } = state;

    let resolver = FnResolver {
        globals: &globals,
        functions: &functions,
    };

    let mut runs = BTreeMap::new();
    for (run_name, pending_run) in run_blocks {
        let PendingRunBlock { body, source_name } = pending_run;
        let mut scope = globals.clone();
        let mut row = 0;
        let mut cycle_stack = Vec::new();
        let instructions: Vec<crate::ir::Instruction> = compile_fn_stmts(
            &body,
            &mut scope,
            &source_texts,
            &source_name,
            &resolver,
            &mut cycle_stack,
            &mut row,
        )?;
        runs.insert(run_name, instructions);
    }

    Ok(Ir { runs })
}
