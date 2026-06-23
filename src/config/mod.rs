pub(crate) mod error;
pub(crate) mod merge;
pub(crate) mod types;
pub(crate) mod validation;

pub use error::ConfigError;
pub use types::{Config, Project};
pub use validation::is_sanctuary_disabled;
pub use validation::validate;

use crate::dsl::ast::{Program, Stmt};
use crate::dsl::lexer::Lexer;
use crate::dsl::parser::Parser;
use crate::ir::Expr;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub fn load(entry_path: &Path) -> Result<Config, ConfigError> {
    let abs_path = if entry_path.is_absolute() {
        entry_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(ConfigError::Io)?
            .join(entry_path)
    };

    let mut loaded_files = HashSet::new();
    let mut recursion_stack = HashSet::new();
    let programs = parse_recursive(&abs_path, &mut loaded_files, &mut recursion_stack)?;

    let config = merge::merge(programs)?;

    Ok(config)
}

pub fn default_config_path() -> PathBuf {
    if is_sanctuary_disabled()
        && let Ok(cwd) = std::env::current_dir()
    {
        let local = cwd.join(".kiru").join("main.kiru");
        if local.exists() {
            return local;
        }
    }
    if let Some(config_dir) = dirs::config_dir() {
        return config_dir.join("kiru").join("main.kiru");
    }
    PathBuf::from("main.kiru")
}

pub fn resolve_includes(cfg: &mut Config) -> Result<(), ConfigError> {
    validation::resolve_include(cfg, parse_recursive)
}

fn parse_recursive(
    file_path: &Path,
    loaded_files: &mut HashSet<PathBuf>,
    recursion_stack: &mut HashSet<PathBuf>,
) -> Result<Vec<Program>, ConfigError> {
    let abs_path = if file_path.is_absolute() {
        file_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(ConfigError::Io)?
            .join(file_path)
    };

    let canon_path = std::fs::canonicalize(&abs_path).map_err(|e| {
        ConfigError::Io(std::io::Error::new(
            e.kind(),
            format!("Failed to resolve {}: {}", abs_path.display(), e),
        ))
    })?;

    if recursion_stack.contains(&canon_path) {
        return Err(ConfigError::CircularImport(
            canon_path.display().to_string(),
        ));
    }

    if loaded_files.contains(&canon_path) {
        return Ok(Vec::new());
    }

    recursion_stack.insert(canon_path.clone());

    let data = std::fs::read_to_string(&canon_path).map_err(|e| {
        recursion_stack.remove(&canon_path);
        ConfigError::Io(std::io::Error::new(
            e.kind(),
            format!("Failed to read {}: {}", canon_path.display(), e),
        ))
    })?;

    let source_name = canon_path.display().to_string();
    let source_text = data.clone();
    let lexer = Lexer::new(data);
    let mut parser = Parser::new(lexer);
    let program = match parser.parse() {
        Ok(mut prog) => {
            prog.set_source(source_name, source_text);
            prog
        }
        Err(errors) => {
            recursion_stack.remove(&canon_path);
            let source = parser.into_source();
            let reports: Vec<miette::Report> = errors
                .into_iter()
                .map(|error| {
                    miette::Report::new(error).with_source_code(miette::NamedSource::new(
                        source_name.clone(),
                        source.clone(),
                    ))
                })
                .collect();
            return Err(ConfigError::ParseReports(reports));
        }
    };

    let mut results = Vec::new();

    let base_dir = canon_path.parent().unwrap_or_else(|| Path::new("."));

    for stmt in &program.stmts {
        if let Stmt::ImportDecl { path } = stmt {
            let rel_path = match path {
                Expr::BacktickLit { parts, .. } => {
                    let mut s = String::new();
                    for part in parts {
                        if part.is_var {
                            return Err(ConfigError::Validation(format!(
                                "variable interpolation in import path is not supported: ${{{}}}",
                                part.value
                            )));
                        }
                        s.push_str(&part.value);
                    }
                    s
                }
                Expr::VarRef { name, .. } => {
                    return Err(ConfigError::Validation(format!(
                        "variable reference in import path is not supported: ${}",
                        name
                    )));
                }
            };
            let import_abs = base_dir.join(&rel_path);
            match parse_recursive(&import_abs, loaded_files, recursion_stack) {
                Ok(imported) => results.extend(imported),
                Err(e) => {
                    recursion_stack.remove(&canon_path);
                    return Err(e);
                }
            }
        }
    }

    recursion_stack.remove(&canon_path);
    loaded_files.insert(canon_path);
    results.push(program);
    Ok(results)
}

#[cfg(test)]
mod tests;
