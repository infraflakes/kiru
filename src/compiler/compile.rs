use crate::compiler::error::CompileError;
use crate::compiler::merge;
use crate::compiler::types::Sanctuary;
use crate::compiler::validation;
use crate::dsl::Parser;
use crate::dsl::{Expr, Program, TopLevel};
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

fn resolve_import_path(path: &Expr) -> Result<String, CompileError> {
    match path {
        Expr::BacktickLit { parts, .. } => {
            let mut s = String::new();
            for part in parts {
                if part.is_var {
                    return Err(CompileError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!(
                            "variable interpolation in import path is not supported: ${{{}}}",
                            part.value
                        ),
                    )));
                }
                s.push_str(&part.value);
            }
            Ok(s)
        }
        Expr::VarRef { name, .. } => Err(CompileError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "variable reference in import path is not supported: ${}",
                name
            ),
        ))),
    }
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
    let mut parser = Parser::from_source(data);
    let mut program = Program::new();
    program.set_source(source_name, source_text);

    let mut results = Vec::new();
    let base_dir = canon_path.parent().unwrap_or_else(|| Path::new("."));

    while let Some(toplevel) = parser.parse_toplevel().map_err(|e| {
        CompileError::ParseReports(vec![miette::Report::new(e).with_source_code(
            miette::NamedSource::new(program.source_name.clone(), program.source_text.clone()),
        )])
    })? {
        match toplevel {
            TopLevel::Stmt(stmt) => program.stmts.push(stmt),
            TopLevel::Import(path) => {
                let rel_path = resolve_import_path(&path)?;
                let import_path = base_dir.join(rel_path.trim_start_matches('/'));
                match parse_recursive(&import_path, loaded_files, recursion_stack) {
                    Ok(imported) => results.extend(imported),
                    Err(e) => {
                        recursion_stack.remove(&canon_path);
                        return Err(e);
                    }
                }
            }
        }
    }

    recursion_stack.remove(&canon_path);
    loaded_files.insert(canon_path);
    if !program.stmts.is_empty() {
        results.push(program);
    }
    Ok(results)
}
