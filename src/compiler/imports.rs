use crate::compiler::error::{CompileError, spanned_err};
use crate::dsl::Expr;
use std::collections::HashMap;

/// Resolve an import expression to an absolute file path.
/// Variable references in the import path are interpolated from the given scope.
pub(crate) fn resolve_import_path(
    expr: &Expr,
    scope: &HashMap<String, String>,
    source_name: &str,
    source_text: &str,
) -> Result<String, CompileError> {
    match expr {
        Expr::BacktickLit {
            parts, offset, len, ..
        } => {
            let mut path_builder = String::new();
            for part in parts {
                if part.is_var {
                    let resolved_value = scope.get(&part.value).ok_or_else(|| {
                        spanned_err(
                            format!("undefined variable in import path: ${{{}}}", part.value),
                            source_name,
                            source_text,
                            *offset,
                            *len,
                        )
                    })?;
                    path_builder.push_str(resolved_value);
                } else {
                    path_builder.push_str(&part.value);
                }
            }
            if path_builder.is_empty() {
                return Err(spanned_err(
                    "import path cannot be empty".to_string(),
                    source_name,
                    source_text,
                    *offset,
                    *len,
                ));
            }
            Ok(path_builder)
        }
        Expr::VarRef {
            name, offset, len, ..
        } => {
            let resolved_value = scope.get(name).ok_or_else(|| {
                spanned_err(
                    format!("undefined variable in import path: ${}", name),
                    source_name,
                    source_text,
                    *offset,
                    *len,
                )
            })?;
            Ok(resolved_value.clone())
        }
    }
}
