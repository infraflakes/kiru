use crate::compiler::error::CompileError;
use crate::dsl::Expr;
use std::collections::HashMap;

/// Resolve an import expression to an absolute file path.
/// Variable references in the import path are interpolated from the given scope.
pub(crate) fn resolve_import_path(
    expr: &Expr,
    scope: &HashMap<String, String>,
) -> Result<String, CompileError> {
    match expr {
        Expr::BacktickLit { parts, .. } => {
            let mut path_builder = String::new();
            for part in parts {
                if part.is_var {
                    let resolved_value = scope.get(&part.value).ok_or_else(|| {
                        CompileError::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            format!("undefined variable in import path: ${{{}}}", part.value),
                        ))
                    })?;
                    path_builder.push_str(resolved_value);
                } else {
                    path_builder.push_str(&part.value);
                }
            }
            if path_builder.is_empty() {
                return Err(CompileError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "import path cannot be empty".to_string(),
                )));
            }
            Ok(path_builder)
        }
        Expr::VarRef { name, .. } => {
            let resolved_value = scope.get(name).ok_or_else(|| {
                CompileError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("undefined variable in import path: ${}", name),
                ))
            })?;
            Ok(resolved_value.clone())
        }
    }
}
