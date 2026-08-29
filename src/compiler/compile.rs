use crate::compiler::error::CompileError;
use crate::dsl::lexer::Lexer;
use crate::dsl::syntax::ArmPattern as DslArmPattern;
use crate::dsl::{Part as DslPart, Program, Stmt, Template, TopLevel};
use crate::error::spanned_report_on;
use crate::plan::{
    Arm, ArmPattern, Call, EnvPair, Instruction, Part, Plan, Project, Sync,
    Template as PlanTemplate,
};
use crate::subprocess;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Run the full compilation pipeline, always building the complete plan (the
/// runner/sync both need the resolved projects).
pub fn compile_and_resolve(entry_path: &Path, _force_cwd: bool) -> Result<Plan, CompileError> {
    let abs_entry = canonicalize_entry(entry_path)?;
    let mut state = CompilerState::new();
    compile_source_file(&abs_entry, &mut state)?;
    build_plan(state)
}

struct CompilerState {
    /// Static variables (top-level and `pr`-body), each already inlined to a
    /// template with no `@(var)` references. Commands inside them are preserved
    /// as `Cmd` parts — they are never executed or frozen at compile time.
    globals: BTreeMap<String, Template>,
    shell: Option<String>,
    syncs: BTreeMap<String, PendingSync>,
    projects: BTreeMap<String, PendingProject>,
    run_blocks: BTreeMap<String, Vec<Vec<Call>>>,
    source_texts: HashMap<String, String>,
    loaded_files: HashSet<PathBuf>,
    recursion_stack: HashSet<PathBuf>,
}

/// A repository/sync block being accumulated (fields only).
struct PendingSync {
    url: Option<Template>,
    dir: Option<Template>,
    branch: Option<Template>,
    strategy: Option<Template>,
}

/// A project block being accumulated: inlined static vars and lowered function
/// bodies. Function-local `bind` variables are inlined away during lowering, so
/// nothing static survives here either.
struct PendingProject {
    vars: BTreeMap<String, Template>,
    functions: BTreeMap<String, Vec<Instruction>>,
}

impl CompilerState {
    fn new() -> Self {
        Self {
            globals: BTreeMap::new(),
            shell: None,
            syncs: BTreeMap::new(),
            projects: BTreeMap::new(),
            run_blocks: BTreeMap::new(),
            source_texts: HashMap::new(),
            loaded_files: HashSet::new(),
            recursion_stack: HashSet::new(),
        }
    }

    fn shell(&self) -> String {
        self.shell.clone().unwrap_or_else(|| "sh".to_string())
    }

    fn spanned(
        &self,
        msg: impl Into<String>,
        source_name: &str,
        offset: usize,
        len: usize,
    ) -> CompileError {
        let report = spanned_report_on(msg.into(), &self.source_texts, source_name, offset, len);
        CompileError::ValidationReport(vec![report])
    }
}

/// Resolve a path to an absolute, canonical location.
pub(crate) fn canonicalize_entry(path: &Path) -> Result<PathBuf, CompileError> {
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(CompileError::Io)?
            .join(path)
    };
    std::fs::canonicalize(&abs_path).map_err(|e| {
        CompileError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to resolve {}: {}", abs_path.display(), e),
        ))
    })
}

/// Inline every `@(var)` reference in `tmpl` against `scope`, replacing each
/// with the (already-inlined) template it names. Commands are preserved as
/// `Cmd` parts — they are never executed here. Returns the flattened list of
/// parts (a var that resolves to several parts is spliced in directly).
///
/// `stack` tracks the variable-resolution chain so a self- or mutually-referential
/// `var` is reported instead of looping forever.
fn inline_dsl_parts(
    tmpl: &Template,
    scope: &BTreeMap<String, Template>,
    sources: &HashMap<String, String>,
    source_name: &str,
    stack: &mut Vec<String>,
) -> Result<Vec<DslPart>, CompileError> {
    let mut out = Vec::new();
    for part in &tmpl.parts {
        match part {
            DslPart::Lit(s) => out.push(DslPart::Lit(s.clone())),
            DslPart::Var(name) => {
                if stack.contains(name) {
                    return Err(CompileError::ValidationReport(vec![spanned_report_on(
                        format!("circular variable reference: {}", name),
                        sources,
                        source_name,
                        tmpl.offset,
                        tmpl.len.max(1),
                    )]));
                }
                let var_tmpl = scope.get(name).ok_or_else(|| {
                    CompileError::ValidationReport(vec![spanned_report_on(
                        format!("undefined variable: {}", name),
                        sources,
                        source_name,
                        tmpl.offset,
                        tmpl.len.max(1),
                    )])
                })?;
                stack.push(name.clone());
                let inlined = inline_dsl_parts(var_tmpl, scope, sources, source_name, stack)?;
                stack.pop();
                out.extend(inlined);
            }
            DslPart::Cmd(inner) => {
                let inlined = inline_dsl_parts(inner, scope, sources, source_name, stack)?;
                out.push(DslPart::Cmd(Template {
                    parts: inlined,
                    offset: inner.offset,
                    len: inner.len,
                    source_name: inner.source_name.clone(),
                }));
            }
        }
    }
    Ok(out)
}

/// Inline `@(var)` references in `tmpl` against `scope`, returning a template with
/// no `Var` parts.
fn inline_dsl_template(
    tmpl: &Template,
    scope: &BTreeMap<String, Template>,
    sources: &HashMap<String, String>,
    source_name: &str,
) -> Result<Template, CompileError> {
    let parts = inline_dsl_parts(tmpl, scope, sources, source_name, &mut Vec::new())?;
    Ok(Template {
        parts,
        offset: tmpl.offset,
        len: tmpl.len,
        source_name: tmpl.source_name.clone(),
    })
}

/// Lower an already-inlined DSL `Template` (no `Var` parts) into a plan
/// `Template`. Commands survive as `Cmd` nodes for the runner to execute.
fn lower_template(tmpl: &Template) -> PlanTemplate {
    PlanTemplate {
        parts: tmpl
            .parts
            .iter()
            .map(|p| match p {
                DslPart::Lit(s) => Part::Lit(s.clone()),
                DslPart::Var(_) => {
                    unreachable!("variables are inlined away before lowering")
                }
                DslPart::Cmd(inner) => Part::Cmd(lower_template(inner)),
            })
            .collect(),
    }
}

/// Render a template to a plain string for structural compile-time needs
/// (e.g. the `shell` value), concatenating literal parts and dropping command
/// output. Variable references are expected to be inlined already.
fn render_literal(tmpl: &Template) -> String {
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

/// Resolve an import path at compile time. Imports are a structural file-system
/// operation, so any `$(command)` part here is executed to obtain a concrete
/// path — this is the one place commands run at compile time, and the result is
/// used only to locate the file (it is never frozen into the plan).
fn eval_path_template(tmpl: &Template, shell: &str) -> String {
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

/// Lower a function body into plan `Instruction`s, inlining every `@(var)`
/// reference (against `static_scope` plus function-local `bind`s) as it goes.
///
/// Function-local `bind x = T` adds `x -> T` to the local scope so later
/// references resolve to `T`; the `bind` itself becomes a plain command
/// execution (no `target`) since the variable is fully inlined at compile time.
/// Nested `env`/`switch` bodies get a *copy* of the local scope so their binds
/// do not leak into the surrounding body.
fn lower_function_body(
    stmts: &[crate::dsl::FnStmt],
    static_scope: &BTreeMap<String, Template>,
    sources: &HashMap<String, String>,
    source_name: &str,
) -> Result<Vec<Instruction>, CompileError> {
    let mut local_scope = static_scope.clone();
    lower_fn_stmts(stmts, &mut local_scope, sources, source_name)
}

fn lower_fn_stmts(
    stmts: &[crate::dsl::FnStmt],
    scope: &mut BTreeMap<String, Template>,
    sources: &HashMap<String, String>,
    source_name: &str,
) -> Result<Vec<Instruction>, CompileError> {
    let mut out = Vec::new();
    for stmt in stmts {
        match stmt {
            crate::dsl::FnStmt::Log(t) => {
                out.push(Instruction::Log(lower_template(&inline_dsl_template(
                    t,
                    scope,
                    sources,
                    source_name,
                )?)));
            }
            crate::dsl::FnStmt::Bind { target, value } => {
                let inlined = inline_dsl_template(value, scope, sources, source_name)?;
                if let Some(name) = target {
                    scope.insert(name.clone(), inlined.clone());
                }
                out.push(Instruction::Bind {
                    value: lower_template(&inlined),
                });
            }
            crate::dsl::FnStmt::Cd(t) => {
                out.push(Instruction::Cd(lower_template(&inline_dsl_template(
                    t,
                    scope,
                    sources,
                    source_name,
                )?)));
            }
            crate::dsl::FnStmt::EnvBlock { pairs, body } => {
                let pairs = pairs
                    .iter()
                    .map(|p| -> Result<EnvPair, CompileError> {
                        Ok(EnvPair {
                            key: p.key.clone(),
                            value: lower_template(&inline_dsl_template(
                                &p.value,
                                scope,
                                sources,
                                source_name,
                            )?),
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let mut inner = scope.clone();
                let body = lower_fn_stmts(body, &mut inner, sources, source_name)?;
                out.push(Instruction::Env { pairs, body });
            }
            crate::dsl::FnStmt::Switch { subject, arms } => {
                let subject =
                    lower_template(&inline_dsl_template(subject, scope, sources, source_name)?);
                let mut arms_out = Vec::new();
                for arm in arms {
                    let pattern = match &arm.pattern {
                        DslArmPattern::Lit(s) => ArmPattern::Lit(s.clone()),
                        DslArmPattern::Default => ArmPattern::Default,
                    };
                    let mut inner = scope.clone();
                    let body = lower_fn_stmts(&arm.body, &mut inner, sources, source_name)?;
                    arms_out.push(Arm { pattern, body });
                }
                out.push(Instruction::Switch {
                    subject,
                    arms: arms_out,
                });
            }
        }
    }
    Ok(out)
}

fn parse_file(canon_path: &Path) -> Result<Program, CompileError> {
    let source_text = std::fs::read_to_string(canon_path).map_err(|e| {
        CompileError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to read {}: {}", canon_path.display(), e),
        ))
    })?;
    let source_name = canon_path.display().to_string();
    let mut parser = crate::dsl::Parser::new(Lexer::new(source_text.clone()))
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

thread_local! {
    static PARSED_CACHE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn compile_source_file(file_path: &Path, state: &mut CompilerState) -> Result<(), CompileError> {
    let canon_path = std::fs::canonicalize(file_path).map_err(|e| {
        CompileError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to resolve {}: {}", file_path.display(), e),
        ))
    })?;
    if state.recursion_stack.contains(&canon_path) {
        return Err(state.spanned(
            format!("circular import: {}", canon_path.display()),
            &canon_path.display().to_string(),
            0,
            1,
        ));
    }
    if state.loaded_files.contains(&canon_path) {
        return Ok(());
    }
    state.recursion_stack.insert(canon_path.clone());
    let program = parse_file(&canon_path)?;
    let result = compile_program(&program, state);
    state.recursion_stack.remove(&canon_path);
    result
}

fn compile_program(program: &Program, state: &mut CompilerState) -> Result<(), CompileError> {
    state
        .source_texts
        .insert(program.source_name.clone(), program.source_text.clone());
    for item in &program.top_level_items {
        match item {
            TopLevel::Stmt(stmt) => compile_stmt(stmt, state, program)?,
            TopLevel::Import(path) => {
                load_import(path, state, program)?;
            }
        }
    }
    Ok(())
}

fn compile_stmt(
    stmt: &Stmt,
    state: &mut CompilerState,
    program: &Program,
) -> Result<(), CompileError> {
    match stmt {
        Stmt::Shell {
            value,
            offset,
            len,
            source_name,
        } => {
            let inlined =
                inline_dsl_template(value, &state.globals, &state.source_texts, source_name)?;
            let resolved = render_literal(&inlined);
            if state.shell.is_some() {
                return Err(state.spanned(
                    "duplicate shell declaration".to_string(),
                    source_name,
                    *offset,
                    *len,
                ));
            }
            state.shell = Some(resolved);
            Ok(())
        }
        Stmt::Var {
            name,
            value,
            offset,
            len,
        } => {
            let inlined = inline_dsl_template(
                value,
                &state.globals,
                &state.source_texts,
                &program.source_name,
            )?;
            if state.globals.contains_key(name) {
                return Err(state.spanned(
                    format!("variable `{}` is already defined", name),
                    &program.source_name,
                    *offset,
                    *len,
                ));
            }
            state.globals.insert(name.clone(), inlined);
            Ok(())
        }
        Stmt::Fn {
            name,
            body,
            offset,
            len,
        } => {
            // Top-level function: attached to no project. Kept so it can be
            // referenced, but `kiru` runs functions inside `pr` blocks.
            let _ = (name, body, offset, len);
            Ok(())
        }
        Stmt::Project { name, fields, body } => compile_project(name, fields, body, state, program),
        Stmt::Run {
            name,
            calls,
            offset,
            len,
        } => {
            if state.run_blocks.contains_key(name) {
                return Err(state.spanned(
                    format!("duplicate run block: {}", name),
                    &program.source_name,
                    *offset,
                    *len,
                ));
            }
            state.run_blocks.insert(name.clone(), calls.clone());
            Ok(())
        }
        Stmt::Field { .. } => Ok(()),
    }
}

fn compile_project(
    name: &str,
    fields: &[Stmt],
    body: &[Stmt],
    state: &mut CompilerState,
    program: &Program,
) -> Result<(), CompileError> {
    // Sync fields (if any) accumulate into the syncs map.
    if !fields.is_empty() {
        let pending = state
            .syncs
            .entry(name.to_string())
            .or_insert_with(|| PendingSync {
                url: None,
                dir: None,
                branch: None,
                strategy: None,
            });
        for field in fields {
            if let Stmt::Field {
                key,
                value,
                offset,
                len,
            } = field
            {
                let resolved = inline_dsl_template(
                    value,
                    &state.globals,
                    &state.source_texts,
                    &program.source_name,
                )?;
                match key {
                    crate::dsl::ProjectField::Url => {
                        if pending.url.is_some() {
                            return Err(state.spanned(
                                "duplicate field 'url'".to_string(),
                                &program.source_name,
                                *offset,
                                *len,
                            ));
                        }
                        pending.url = Some(resolved);
                    }
                    crate::dsl::ProjectField::Dir => {
                        if pending.dir.is_some() {
                            return Err(state.spanned(
                                "duplicate field 'dir'".to_string(),
                                &program.source_name,
                                *offset,
                                *len,
                            ));
                        }
                        pending.dir = Some(resolved);
                    }
                    crate::dsl::ProjectField::Branch => {
                        if pending.branch.is_some() {
                            return Err(state.spanned(
                                "duplicate field 'branch'".to_string(),
                                &program.source_name,
                                *offset,
                                *len,
                            ));
                        }
                        pending.branch = Some(resolved);
                    }
                    crate::dsl::ProjectField::Sync => {
                        if pending.strategy.is_some() {
                            return Err(state.spanned(
                                "duplicate field 'sync'".to_string(),
                                &program.source_name,
                                *offset,
                                *len,
                            ));
                        }
                        pending.strategy = Some(resolved);
                    }
                }
            }
        }
    }

    // Project body: `var` (frozen), `fn` (lowered to instructions).
    let pending = state
        .projects
        .entry(name.to_string())
        .or_insert_with(|| PendingProject {
            vars: BTreeMap::new(),
            functions: BTreeMap::new(),
        });

    // Scope for resolving this project's vars: globals + already-defined vars.
    let mut scope = state.globals.clone();
    for (k, v) in &pending.vars {
        scope.insert(k.clone(), v.clone());
    }

    for stmt in body {
        match stmt {
            Stmt::Var {
                name: var_name,
                value,
                offset,
                len,
            } => {
                let resolved =
                    inline_dsl_template(value, &scope, &state.source_texts, &program.source_name)?;
                if pending.vars.contains_key(var_name) {
                    return Err(state.spanned(
                        format!(
                            "variable `{}` is already defined in project `{}`",
                            var_name, name
                        ),
                        &program.source_name,
                        *offset,
                        *len,
                    ));
                }
                pending.vars.insert(var_name.clone(), resolved.clone());
                scope.insert(var_name.clone(), resolved);
            }
            Stmt::Fn {
                name: fn_name,
                body: fn_body,
                offset,
                len,
            } => {
                if pending.functions.contains_key(fn_name) {
                    return Err(state.spanned(
                        format!("duplicate function `{}` in project `{}`", fn_name, name),
                        &program.source_name,
                        *offset,
                        *len,
                    ));
                }
                let lowered = lower_function_body(
                    fn_body,
                    &scope,
                    &state.source_texts,
                    &program.source_name,
                )?;
                pending.functions.insert(fn_name.clone(), lowered);
            }
            _ => {}
        }
    }

    Ok(())
}

fn load_import(
    path: &Template,
    state: &mut CompilerState,
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

fn build_plan(state: CompilerState) -> Result<Plan, CompileError> {
    let shell = state.shell.clone().unwrap_or_else(|| "sh".to_string());

    let mut syncs = BTreeMap::new();
    for (name, s) in &state.syncs {
        syncs.insert(
            name.clone(),
            Sync {
                url: lower_template(s.url.as_ref().unwrap_or(&Template::default())),
                dir: lower_template(s.dir.as_ref().unwrap_or(&Template::default())),
                branch: lower_template(s.branch.as_ref().unwrap_or(&Template::default())),
                strategy: lower_template(s.strategy.as_ref().unwrap_or(&Template::lit("clone"))),
            },
        );
    }

    let mut projects = BTreeMap::new();
    // Every name that appears in either map gets a project entry.
    let mut names: Vec<String> = state.projects.keys().cloned().collect();
    for name in state.syncs.keys() {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }
    for name in names {
        let functions = match state.projects.get(&name) {
            Some(pending) => pending.functions.clone(),
            None => BTreeMap::new(),
        };
        projects.insert(name, Project { functions });
    }

    // Validate run-block references against the merged projects.
    for (run_name, stages) in &state.run_blocks {
        for stage in stages {
            for call in stage {
                match projects.get(&call.project) {
                    Some(project) => {
                        if !project.functions.contains_key(&call.function) {
                            return Err(CompileError::ValidationReport(vec![miette::miette!(
                                "run `{}`: function `{}` not found in project `{}`",
                                run_name,
                                call.function,
                                call.project
                            )]));
                        }
                    }
                    None => {
                        return Err(CompileError::ValidationReport(vec![miette::miette!(
                            "run `{}`: unknown project `{}`",
                            run_name,
                            call.project
                        )]));
                    }
                }
            }
        }
    }

    Ok(Plan {
        shell,
        syncs,
        projects,
        run_blocks: state.run_blocks,
    })
}
