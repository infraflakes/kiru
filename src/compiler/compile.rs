use crate::compiler::CompileError;
use crate::compiler::merge;
use crate::compiler::types::Sanctuary;
use crate::compiler::validation;
use crate::dsl::Expr;
use crate::dsl::ast::{Program, Stmt};
use crate::dsl::lexer::Lexer;
use crate::dsl::parser::Parser;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub fn compile(entry_path: &Path) -> Result<Sanctuary, CompileError> {
    let abs_path = if entry_path.is_absolute() {
        entry_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(CompileError::Io)?
            .join(entry_path)
    };

    let mut loaded_files = HashSet::new();
    let mut recursion_stack = HashSet::new();
    let programs = parse_recursive(&abs_path, &mut loaded_files, &mut recursion_stack)?;

    let config = merge::merge(programs)?;

    Ok(config)
}

pub fn resolve_includes(cfg: &mut Sanctuary) -> Result<(), CompileError> {
    validation::resolve_include(cfg, parse_recursive)
}

fn parse_recursive(
    file_path: &Path,
    loaded_files: &mut HashSet<PathBuf>,
    recursion_stack: &mut HashSet<PathBuf>,
) -> Result<Vec<Program>, CompileError> {
    let abs_path = if file_path.is_absolute() {
        file_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(CompileError::Io)?
            .join(file_path)
    };

    let canon_path = std::fs::canonicalize(&abs_path).map_err(|e| {
        CompileError::Io(std::io::Error::new(
            e.kind(),
            format!("Failed to resolve {}: {}", abs_path.display(), e),
        ))
    })?;

    if recursion_stack.contains(&canon_path) {
        return Err(CompileError::CircularImport(
            canon_path.display().to_string(),
        ));
    }

    if loaded_files.contains(&canon_path) {
        return Ok(Vec::new());
    }

    recursion_stack.insert(canon_path.clone());

    let data = std::fs::read_to_string(&canon_path).map_err(|e| {
        recursion_stack.remove(&canon_path);
        CompileError::Io(std::io::Error::new(
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
            return Err(CompileError::ParseReports(reports));
        }
    };

    let mut results = Vec::new();

    let base_dir = canon_path.parent().unwrap_or_else(|| Path::new("."));

    for stmt in &program.stmts {
        if let Stmt::Import { path } = stmt {
            let rel_path = match path {
                Expr::BacktickLit { parts, .. } => {
                    let mut s = String::new();
                    for part in parts {
                        if part.is_var {
                            return Err(CompileError::Validation(format!(
                                "variable interpolation in import path is not supported: ${{{}}}",
                                part.value
                            )));
                        }
                        s.push_str(&part.value);
                    }
                    s
                }
                Expr::VarRef { name, .. } => {
                    return Err(CompileError::Validation(format!(
                        "variable reference in import path is not supported: ${}",
                        name
                    )));
                }
            };
            let import_abs = base_dir.join(rel_path.trim_start_matches('/'));
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
