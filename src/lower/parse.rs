//! Source file parsing and import resolution: reads `.kiru` files, resolves
//! import candidates (direct, basename, directory glob), and compiles them
//! into the lowering state.

use crate::diagnostics::{Diagnostic, Span};
use crate::syntax::lexer::Lexer;
use crate::syntax::{Part as DslPart, Program, Template};
use std::path::{Path, PathBuf};

use super::{CompileError, LoweringState, compile_source_file, inline::inline_dsl_template};

pub(super) fn parse_file(canon_path: &Path) -> Result<Program, CompileError> {
    let source_text = std::fs::read_to_string(canon_path).map_err(|e| {
        CompileError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to read {}: {}", canon_path.display(), e),
        ))
    })?;
    let source_name = canon_path.display().to_string();
    let mut parser = crate::syntax::Parser::new(Lexer::new(source_text.clone()));
    let mut program = Program::new_with_source(source_name, source_text);
    while let Some(toplevel) = parser.parse_toplevel().map_err(|e| {
        CompileError::diagnostic(Diagnostic::new(
            program.source_name.clone(),
            e.span,
            e.msg,
            program.source_text.clone(),
        ))
    })? {
        program.top_level_items.push(toplevel);
    }
    Ok(program)
}

pub(super) fn load_import(
    path: &Template,
    state: &mut LoweringState,
    program: &Program,
) -> Result<(), CompileError> {
    let inlined = inline_dsl_template(
        path,
        &state.globals,
        &state.source_texts,
        &program.source_name,
    )?;
    let path_str = eval_path_template(&inlined, state, &program.source_name)?;
    if path_str.is_empty() {
        return Err(state.spanned(
            "import path cannot be empty".to_string(),
            &program.source_name,
            path.offset,
            path.len.max(1),
        ));
    }

    let base_dir = Path::new(&program.source_name).parent().ok_or_else(|| {
        state.spanned(
            format!(
                "cannot determine base directory for import from '{}'",
                program.source_name
            ),
            &program.source_name,
            0,
            1,
        )
    })?;

    let candidates = resolve_import_candidates(base_dir, &path_str);
    for candidate in candidates {
        if candidate.exists() {
            compile_source_file(&candidate, state)?;
            return Ok(());
        }
    }

    // Missing import: non-fatal. Report and continue so `status` works even
    // when optional imports are absent.
    let diag = Diagnostic::new(
        program.source_name.to_string(),
        Span::new(path.offset, path.len.max(1)),
        format!("import target '{}' does not exist, skipping", path_str),
        state
            .source_texts
            .get(&program.source_name)
            .cloned()
            .unwrap_or_default(),
    );
    crate::diagnostics::print_diagnostic(&diag);
    Ok(())
}

/// Build the ordered list of candidate paths for an import. Tries the literal
/// joined path first, then a basename fallback (so `(kiru/environment.kiru)`
/// resolves to `environment.kiru` in the same directory), then a `*.kiru`
/// directory glob when the path (without the trailing `.kiru`) is a directory.
fn resolve_import_candidates(base_dir: &Path, path_str: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    candidates.push(base_dir.join(path_str));

    if let Some(filename) = Path::new(path_str).file_name() {
        candidates.push(base_dir.join(filename));
    }

    // Directory glob: `some/dir.kiru` -> `some/dir/*.kiru` if `some/dir` is a dir.
    if path_str.ends_with(".kiru") {
        let stripped = path_str.strip_suffix(".kiru").unwrap_or(path_str);
        let dir = base_dir.join(stripped);
        if dir.is_dir()
            && let Ok(entries) = std::fs::read_dir(&dir)
        {
            let mut kiru_files: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().map(|x| x == "kiru").unwrap_or(false))
                .collect();
            kiru_files.sort();
            candidates.extend(kiru_files);
        }
    }

    candidates
}

/// Resolve an import path at compile time. Imports are a structural file-system
/// operation, so any `$(command)` part here is executed to obtain a concrete
/// path -- this is the one place commands run at compile time, and the result is
/// used only to locate the file (it is never frozen into the IR).
///
/// Deliberately config-independent: compilation never reads `kiru.toml` (the IR
/// is config-free), so these commands always run under `sh` with no timeout.
/// Custom shell/timeout apply at run time only.
pub(super) fn eval_path_template(
    tmpl: &Template,
    state: &mut LoweringState,
    source_name: &str,
) -> Result<String, CompileError> {
    let mut out = String::new();
    for part in &tmpl.parts {
        match part {
            DslPart::Lit(s) => out.push_str(s),
            // Unreachable via the parser: `inline_dsl_template` resolves every
            // `Var` part before its result reaches this function. Kept as an
            // error rather than a silent fallback so the invariant is enforced.
            DslPart::Var(name) => {
                return Err(state.spanned(
                    format!("unexpected @({name}) in import path"),
                    source_name,
                    tmpl.offset,
                    tmpl.len.max(1),
                ));
            }
            DslPart::Cmd(inner) => {
                let cmd = eval_path_template(inner, state, source_name)?;
                out.push_str(&run_capture(&cmd));
            }
        }
    }
    Ok(out)
}

/// Run `cmd` via `sh -c` and return its stdout (trimmed). Non-zero exit is
/// non-fatal: whatever stdout was produced is returned. Used only for resolving
/// import paths (see `eval_path_template`).
fn run_capture(cmd: &str) -> String {
    crate::exec::subprocess::capture_shell(cmd, "sh", None, None, None).unwrap_or_default()
}
