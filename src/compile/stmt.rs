//! Per-variant statement compilation helpers, extracted from `compile_stmt`.

use super::inline::inline_dsl_template;
use super::{CompileError, CompileState};

pub(super) fn compile_var_decl(
    name: &str,
    value: &crate::syntax::Template,
    offset: usize,
    len: usize,
    source_name: &str,
    state: &mut CompileState,
) -> Result<(), CompileError> {
    let inlined = inline_dsl_template(value, &state.globals, &state.source_texts, source_name)?;
    if state.globals.contains_key(name) {
        return Err(state.spanned(
            format!("variable `{}` is already defined", name),
            source_name,
            offset,
            len,
        ));
    }
    state.globals.insert(name.to_string(), inlined);
    Ok(())
}

pub(super) fn compile_run_decl(
    name: &str,
    body: &[crate::syntax::FnStmt],
    offset: usize,
    len: usize,
    source_name: &str,
    state: &mut CompileState,
) -> Result<(), CompileError> {
    if state.run_blocks.contains_key(name) {
        return Err(state.spanned(
            format!("duplicate run block: {}", name),
            source_name,
            offset,
            len,
        ));
    }
    state.run_blocks.insert(
        name.to_string(),
        super::PendingRunBlock {
            body: body.to_vec(),
            source_name: source_name.to_string(),
        },
    );
    Ok(())
}
