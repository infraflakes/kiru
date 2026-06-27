use crate::compiler::error::CompileError;
use crate::compiler::error::spanned_err;
use crate::dsl::Expr;
use crate::shell;
use std::collections::HashMap;

pub(crate) fn resolve_expr_merged(
    expr: &Expr,
    global_vars: &HashMap<String, String>,
    project_vars: &HashMap<String, String>,
    source_name: &str,
    source_text: &str,
) -> Result<String, CompileError> {
    let get_var = |name: &str| -> Option<&String> {
        project_vars.get(name).or_else(|| global_vars.get(name))
    };
    let err_for =
        |msg: String, o: usize, l: usize| spanned_err(msg, source_name, source_text, o, l);
    match expr {
        Expr::VarRef { name, offset, len } => {
            if let Some(val) = get_var(name) {
                return Ok(val.clone());
            }
            Err(err_for(
                format!("undefined variable: ${}", name),
                *offset,
                *len,
            ))
        }
        Expr::BacktickLit { parts, offset, len } => {
            let mut result = String::new();
            for part in parts {
                if part.is_var {
                    if let Some(val) = get_var(&part.value) {
                        result.push_str(val);
                    } else {
                        return Err(err_for(
                            format!("undefined variable: ${}", part.value),
                            *offset,
                            *len,
                        ));
                    }
                } else {
                    result.push_str(&part.value);
                }
            }
            Ok(result)
        }
    }
}

pub(crate) fn exec_shell_var(
    name: &str,
    resolved_command: &str,
    source_name: &str,
    source_text: &str,
    offset: usize,
    len: usize,
) -> Result<String, CompileError> {
    shell::exec_and_get_stdout(resolved_command, None, None).map_err(|e| {
        spanned_err(
            format!("shell var ${} failed: {}", name, e),
            source_name,
            source_text,
            offset,
            len,
        )
    })
}
