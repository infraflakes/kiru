use crate::error::spanned_report_on;
use crate::exec::subprocess;
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
    let mut parser = crate::syntax::Parser::new(Lexer::new(source_text.clone()))
        .with_source_name(source_name.clone());
    let mut program = Program::new_with_source(source_name, source_text);
    while let Some(toplevel) = parser.parse_toplevel().map_err(|e| {
        CompileError::ParseReports(vec![miette::Report::new(e).with_source_code(
            miette::NamedSource::new(program.source_name.clone(), program.source_text.clone()),
        )])
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
    let shell = state.shell();
    let inlined = inline_dsl_template(
        path,
        &state.globals,
        &state.source_texts,
        &program.source_name,
    )?;
    let path_str = eval_path_template(&inlined, &shell);
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
    let report = spanned_report_on(
        format!("import target '{}' does not exist, skipping", path_str),
        &state.source_texts,
        &program.source_name,
        path.offset,
        path.len.max(1),
    );
    crate::error::print_diagnostic(&report);
    Ok(())
}

/// Build the ordered list of candidate paths for an import. Tries the literal
/// joined path first, then a basename fallback (so `(kiru/environment.kiru)`
/// resolves to `environment.kiru` in the same directory), then a `*.kiru`
/// directory glob when the path (without the trailing `.kiru`) is a directory.
fn resolve_import_candidates(base_dir: &Path, path_str: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let direct = base_dir.join(path_str);
    candidates.push(direct.clone());

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
/// path — this is the one place commands run at compile time, and the result is
/// used only to locate the file (it is never frozen into the IR).
pub(super) fn eval_path_template(tmpl: &Template, shell: &str) -> String {
    let mut out = String::new();
    for part in &tmpl.parts {
        match part {
            DslPart::Lit(s) => out.push_str(s),
            DslPart::Var(name) => out.push_str(name),
            DslPart::Cmd(inner) => {
                let cmd = eval_path_template(inner, shell);
                out.push_str(&run_capture(&cmd, shell));
            }
        }
    }
    out
}

/// Run `cmd` via `shell -c` and return its stdout (trimmed). Non-zero exit is
/// non-fatal: whatever stdout was produced is returned. Used only for resolving
/// import paths (see `eval_path_template`).
fn run_capture(cmd: &str, shell: &str) -> String {
    let mut captured = String::new();
    let _ = subprocess::run_subprocess(cmd, &[shell, "-c", cmd], None, None, None, &mut |line| {
        match line {
            subprocess::SubprocessLine::Stdout(text) => captured.push_str(&text),
            subprocess::SubprocessLine::Stderr(_) => {}
        }
    });
    captured.trim_end().to_string()
}

/// Render a template to a plain string for structural compile-time needs
/// (e.g. the `shell` value), concatenating literal parts and dropping command
/// output. Variable references are expected to be inlined already.
pub(super) fn render_literal(tmpl: &Template) -> String {
    let mut out = String::new();
    for part in &tmpl.parts {
        match part {
            DslPart::Lit(s) => out.push_str(s),
            DslPart::Var(name) => out.push_str(name),
            DslPart::Cmd(inner) => out.push_str(&render_literal(inner)),
        }
    }
    out
}
