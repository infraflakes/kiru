use crate::compiler::error::{CompileError, spanned_err_named, spanned_err_on_field};
use crate::compiler::fnstmt::{ResolveFnCtx, resolve_fn_body_stmts};
use crate::compiler::scope::{BucketRegistry, Redeclaration};
use crate::compiler::types::{ProjectVarStmt, UnresolvedConfig, UnresolvedProject};
use crate::dsl::{CasePattern, Expr, InterpolationPart, Stmt, VarType};
use crate::error::SourceFile;
use crate::plan::{Plan, PlanCasePattern, PlanProject, SyncMode, parse_sync_mode};
use crate::shell;
use miette::miette;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Builds the "undefined variable" error for a `$name` reference absent
/// from `scope`. Centralizes the repeated `format!("undefined variable:
/// ${}", ..)` construction used by both expression and case-pattern
/// resolution (bare `VarRef` and interpolated backtick literals).
fn undefined_var_err(
    name: &str,
    offset: usize,
    len: usize,
    sources: &HashMap<String, String>,
    source_name: &str,
) -> CompileError {
    spanned_err_named(
        format!("undefined variable: ${}", name),
        sources,
        source_name,
        offset,
        len,
    )
}

/// Resolves the interpolation `parts` of a backtick literal (or case-
/// pattern literal) into a concrete string, substituting `$name` /
/// `${name}` references against `scope`. Shared by `resolve_expr` and
/// `resolve_case_pattern` so the substitution loop is defined once.
/// On an undefined reference, the error spans the whole literal
/// (`literal_offset`/`literal_len`), matching prior behavior.
fn resolve_interpolation_to_string(
    parts: &[InterpolationPart],
    scope: &BucketRegistry<String>,
    literal_offset: usize,
    literal_len: usize,
    sources: &HashMap<String, String>,
    source_name: &str,
    view: &CrossProjectView,
) -> Result<String, CompileError> {
    let mut result = String::new();
    for part in parts {
        if part.is_var {
            let val = match &part.namespace {
                Some(donor) => resolve_qualified(
                    donor,
                    &part.value,
                    view,
                    sources,
                    source_name,
                    literal_offset,
                    literal_len,
                )?,
                None => match scope.lookup(&part.value) {
                    Some(val) => val.clone(),
                    None => {
                        return Err(undefined_var_err(
                            &part.value,
                            literal_offset,
                            literal_len,
                            sources,
                            source_name,
                        ));
                    }
                },
            };
            result.push_str(&val);
        } else {
            result.push_str(&part.value);
        }
    }
    Ok(result)
}

/// Resolve an `Expr` to a concrete string using the bucket registry and the
/// cross-project variable view (for qualified `$proj::name` reads).
pub(crate) fn resolve_expr(
    expr: &Expr,
    scope: &BucketRegistry<String>,
    sources: &HashMap<String, String>,
    view: &CrossProjectView,
) -> Result<String, CompileError> {
    match expr {
        Expr::VarRef {
            namespace,
            name,
            offset,
            len,
            source_name,
        } => match namespace {
            Some(donor) => {
                resolve_qualified(donor, name, view, sources, source_name, *offset, *len)
            }
            None => match scope.lookup(name) {
                Some(val) => Ok(val.clone()),
                None => Err(undefined_var_err(name, *offset, *len, sources, source_name)),
            },
        },
        Expr::BacktickLit {
            parts,
            offset,
            len,
            source_name,
        } => {
            resolve_interpolation_to_string(parts, scope, *offset, *len, sources, source_name, view)
        }
    }
}

// ── Config-eval phase (quarantined `var shell` evaluation) ──────────────────
//
// All `var shell` execution is funnelled through `evaluate_config_shell` and
// runs in exactly one phase after validation. `compile_and_resolve` and
// `parse_projects_metadata` both drive this phase via `config_eval_top_level`.
// Results are memoized per (command, working_dir) so an identical command
// evaluates at most once per compile invocation.

/// Memo key for config-time shell evaluation: the resolved command text and
/// the working directory it ran in.
pub(crate) type ShellCache = std::collections::HashMap<(String, Option<String>), String>;

/// Fully-resolved variable state for one project, captured so that other
/// projects can inline its values via qualified `$proj::name` references at
/// compile time (read-only).
pub(crate) struct ResolvedProjectData {
    url: String,
    dir: String,
    sync: SyncMode,
    branch: Option<String>,
    /// The project's bucket registry: the shared global bucket plus this
    /// project's own project bucket. The case bucket is transient and is never
    /// stored here.
    registry: BucketRegistry<String>,
}

/// Map of every project that has been fully resolved so far, keyed by project
/// name. Used while resolving a later project to answer qualified references.
type CrossProjectView = HashMap<String, ResolvedProjectData>;

/// Resolve a qualified `$donor::name` reference against the donor project's
/// already-resolved fields and project bucket. Field names (`url`/`dir`/`sync`/
/// `branch`) resolve to the donor's field value; any other name resolves
/// against the donor's project bucket. An unknown donor or name is an
/// undefined-variable error.
fn resolve_qualified(
    donor: &str,
    name: &str,
    view: &CrossProjectView,
    sources: &HashMap<String, String>,
    source_name: &str,
    offset: usize,
    len: usize,
) -> Result<String, CompileError> {
    let data = match view.get(donor) {
        Some(d) => d,
        None => {
            return Err(undefined_var_err(
                &format!("{}::{}", donor, name),
                offset,
                len,
                sources,
                source_name,
            ));
        }
    };
    let field = match name {
        "url" => Some(data.url.clone()),
        "dir" => Some(data.dir.clone()),
        "branch" => data.branch.clone(),
        "sync" => Some(data.sync.to_string()),
        _ => None,
    };
    if let Some(value) = field {
        return Ok(value);
    }
    match data.registry.lookup(name) {
        Some(value) => Ok(value.clone()),
        None => Err(undefined_var_err(
            &format!("{}::{}", donor, name),
            offset,
            len,
            sources,
            source_name,
        )),
    }
}

/// Collect every donor project name referenced by qualified variable reads in
/// `proj` (project-body vars, fields, and function bodies).
fn collect_donor_projects(proj: &UnresolvedProject, donors: &mut Vec<String>) {
    let mut visit = |expr: &Expr| {
        expr.visit_vars(&mut |_: &str, ns: Option<&str>| {
            if let Some(donor) = ns {
                donors.push(donor.to_string());
            }
        });
    };
    for var_stmt in &proj.var_stmts {
        visit(&var_stmt.value);
    }
    for field in [
        proj.url.as_ref(),
        proj.dir.as_ref(),
        proj.sync.as_ref(),
        proj.branch.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        visit(field);
    }
    for body in proj.functions.values() {
        for stmt in body {
            stmt.visit_vars(&mut |_, ns| {
                if let Some(donor) = ns {
                    donors.push(donor.to_string());
                }
            });
        }
    }
}

/// Order project names so every donor project is resolved before the projects
/// that read from it. Errors on a reference to an unknown project or on a
/// cyclic dependency (which could never be resolved).
fn topo_order_projects(
    projects: &HashMap<String, UnresolvedProject>,
) -> Result<Vec<String>, CompileError> {
    use std::collections::VecDeque;

    let present: HashSet<&str> = projects.keys().map(String::as_str).collect();
    let mut donors: HashMap<String, Vec<String>> = HashMap::new();
    for (name, proj) in projects {
        let mut ds = Vec::new();
        collect_donor_projects(proj, &mut ds);
        for donor in &ds {
            if !present.contains(donor.as_str()) {
                return Err(CompileError::ValidationReport(vec![miette!(
                    "project {:?} references unknown project {:?}",
                    name,
                    donor
                )]));
            }
        }
        donors.insert(name.clone(), ds);
    }

    // Kahn's algorithm: an edge donor -> name means `name` depends on `donor`,
    // so `donor` must be emitted first.
    let mut indegree: HashMap<String, usize> = projects.keys().cloned().map(|k| (k, 0)).collect();
    let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
    for (name, ds) in &donors {
        for donor in ds {
            *indegree.get_mut(name).unwrap() += 1;
            dependents
                .entry(donor.clone())
                .or_default()
                .push(name.clone());
        }
    }

    let mut queue: VecDeque<String> = indegree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(k, _)| k.clone())
        .collect();
    let mut order = Vec::with_capacity(projects.len());
    while let Some(node) = queue.pop_front() {
        order.push(node.clone());
        if let Some(deps) = dependents.get(&node) {
            for dependent in deps {
                let deg = indegree.get_mut(dependent).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    queue.push_back(dependent.clone());
                }
            }
        }
    }

    if order.len() != projects.len() {
        let in_cycle: Vec<&str> = indegree
            .iter()
            .filter(|(_, deg)| **deg > 0)
            .map(|(k, _)| k.as_str())
            .collect();
        return Err(CompileError::ValidationReport(vec![miette!(
            "cyclic cross-project variable dependency involving: {:?}",
            in_cycle
        )]));
    }
    Ok(order)
}

/// A top-level `var shell` deferred to the config-eval phase. We keep the
/// original `Expr` (not its pre-interpolated command) so nested shell vars
/// resolve against real outputs at eval time.
pub(crate) struct PendingShell {
    pub name: String,
    pub value: Expr,
    pub source_name: String,
    pub offset: usize,
    pub len: usize,
}

/// The single funnel for every `var shell` command. Memoizes by
/// (command, working_dir) and delegates to `shell::execute_shell_variable`,
/// which now propagates failures as compile errors.
pub(crate) fn evaluate_config_shell(
    name: &str,
    command: &str,
    working_dir: Option<&Path>,
    source: &SourceFile<'_>,
    offset: usize,
    len: usize,
    cache: &mut ShellCache,
) -> Result<String, CompileError> {
    let key = (
        command.to_string(),
        working_dir.map(|p| p.to_string_lossy().to_string()),
    );
    if let Some(cached) = cache.get(&key) {
        return Ok(cached.clone());
    }
    let result = shell::execute_shell_variable(name, command, working_dir, source, offset, len)?;
    cache.insert(key, result.clone());
    Ok(result)
}

/// Resolve a top-level `var` declaration during linear processing WITHOUT
/// running shell. `var string` is resolved and declared immediately;
/// `var shell` is resolved (its command interpolated against the in-progress
/// scope) and declared as a placeholder, and also returned as a `PendingShell`
/// for the post-validation config-eval phase to fill in with the real output.
pub(crate) fn collect_top_level_var(
    stmt: &Stmt,
    scope: &mut BucketRegistry<String>,
    sources: &HashMap<String, String>,
) -> Result<Option<PendingShell>, CompileError> {
    // Top-level vars resolve before any project, so no cross-project view is
    // available yet (a top-level `var` referencing a project's bucket would be
    // a layering violation and surfaces as an unknown-project error).
    let empty_view: CrossProjectView = HashMap::new();
    if let Stmt::Var {
        var_type,
        name,
        value,
        offset,
        len,
        ..
    } = stmt
    {
        let source_name = value.source_name().to_string();
        let resolved = resolve_expr(value, scope, sources, &empty_view)?;
        scope
            .declare_global(name.clone(), resolved)
            .map_err(|r| redeclaration_err(r, sources, &source_name, *offset, *len))?;
        if *var_type == VarType::Shell {
            Ok(Some(PendingShell {
                name: name.clone(),
                value: value.clone(),
                source_name,
                offset: *offset,
                len: *len,
            }))
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

/// Evaluate all deferred top-level `var shell` declarations after validation.
/// Re-resolves each command (so nested shell vars see real outputs) then swaps
/// the placeholder for the real shell output via `BucketRegistry::update`.
pub(crate) fn config_eval_top_level(
    pending: Vec<PendingShell>,
    scope: &mut BucketRegistry<String>,
    cache: &mut ShellCache,
    sources: &HashMap<String, String>,
) -> Result<(), CompileError> {
    for pending_var in pending {
        let source = SourceFile::from_registry(sources, &pending_var.source_name);
        let empty_view: CrossProjectView = HashMap::new();
        let command = resolve_expr(&pending_var.value, scope, sources, &empty_view)?;
        let output = evaluate_config_shell(
            &pending_var.name,
            &command,
            None,
            &source,
            pending_var.offset,
            pending_var.len,
            cache,
        )?;
        scope.update(&pending_var.name, output);
    }
    Ok(())
}

/// Resolve a case pattern against the bucket registry.
pub(crate) fn resolve_case_pattern(
    pattern: &CasePattern,
    scope: &BucketRegistry<String>,
    sources: &HashMap<String, String>,
    view: &CrossProjectView,
) -> Result<PlanCasePattern, CompileError> {
    match pattern {
        CasePattern::Literal {
            parts,
            offset,
            len,
            source_name,
        } => {
            let resolved = resolve_interpolation_to_string(
                parts,
                scope,
                *offset,
                *len,
                sources,
                source_name,
                view,
            )?;
            Ok(PlanCasePattern::Literal(resolved))
        }
        CasePattern::VarRef {
            namespace,
            name,
            offset,
            len,
            source_name,
        } => match namespace {
            Some(donor) => Ok(PlanCasePattern::Literal(resolve_qualified(
                donor,
                name,
                view,
                sources,
                source_name,
                *offset,
                *len,
            )?)),
            None => match scope.lookup(name) {
                Some(val) => Ok(PlanCasePattern::Literal(val.clone())),
                None => Err(undefined_var_err(name, *offset, *len, sources, source_name)),
            },
        },
        CasePattern::Default => Ok(PlanCasePattern::Default),
    }
}

/// Resolve and bind a `var` or `var shell` into a scope stack.
/// All duplicate detection flows through the bucket registry's `declare_*`
/// methods, which error only within a single bucket (no ancestor-chain shadow).
///
/// `working_dir` — the directory in which to execute `var shell` commands;
/// `None` means the current process directory.
/// Resolve a `var` / `var shell` declaration from individual fields (shared
/// implementation for both `resolve_var_stmt` and `resolve_project_var`).
#[allow(clippy::too_many_arguments)]
fn resolve_var_stmt_inner(
    var_type: &crate::dsl::VarType,
    name: &str,
    value: &Expr,
    offset: usize,
    len: usize,
    scope: &mut BucketRegistry<String>,
    working_dir: Option<&Path>,
    sources: &HashMap<String, String>,
    cache: &mut ShellCache,
    view: &CrossProjectView,
) -> Result<(), CompileError> {
    let source = SourceFile::from_registry(sources, value.source_name());
    let resolved = resolve_expr(value, scope, sources, view)?;
    let final_val = if *var_type == crate::dsl::VarType::Shell {
        evaluate_config_shell(name, &resolved, working_dir, &source, offset, len, cache)?
    } else {
        resolved
    };
    scope
        .declare_project(name.to_string(), final_val)
        .map_err(|r| redeclaration_err(r, sources, value.source_name(), offset, len))?;
    Ok(())
}

/// Resolve a `var` / `var shell` from a `ProjectVarStmt` (second pass in
/// `resolve_with_scopes`).
pub(crate) fn resolve_project_var(
    var: &ProjectVarStmt,
    scope: &mut BucketRegistry<String>,
    working_dir: Option<&Path>,
    sources: &HashMap<String, String>,
    cache: &mut ShellCache,
    view: &CrossProjectView,
) -> Result<(), CompileError> {
    resolve_var_stmt_inner(
        &var.var_type,
        &var.name,
        &var.value,
        var.offset,
        var.len,
        scope,
        working_dir,
        sources,
        cache,
        view,
    )
}

/// Build a spanned error from a `Redeclaration`, located on the node that
/// re-declares the name (resolved against the source-text registry by name).
pub(crate) fn redeclaration_err(
    r: Redeclaration,
    sources: &HashMap<String, String>,
    name: &str,
    offset: usize,
    len: usize,
) -> CompileError {
    let msg = format!("${} is already defined at {}", r.name, r.existing);
    spanned_err_named(msg, sources, name, offset, len)
}

/// Resolve an optional `Expr` field to a concrete string.
pub(crate) fn resolve_optional_expr(
    expr: &Option<Expr>,
    scope: &BucketRegistry<String>,
    sources: &HashMap<String, String>,
    view: &CrossProjectView,
) -> Result<Option<String>, CompileError> {
    match expr {
        Some(e) => {
            let resolved = resolve_expr(e, scope, sources, view)?;
            if resolved.is_empty() {
                Ok(None)
            } else {
                Ok(Some(resolved))
            }
        }
        None => Ok(None),
    }
}

/// Resolve a `dir` field, joining relative paths against the source file's
/// directory so that `dir = \`./foo\`` resolves relative to the `.kiru` file.
fn resolve_dir_field(
    unresolved: &UnresolvedProject,
    scope: &BucketRegistry<String>,
    sources: &HashMap<String, String>,
    view: &CrossProjectView,
) -> Result<String, CompileError> {
    let raw = resolve_optional_expr(&unresolved.dir, scope, sources, view)?.unwrap_or_default();
    if raw.is_empty() || Path::new(&raw).is_absolute() {
        return Ok(raw);
    }
    let dir_source_name = unresolved
        .dir
        .as_ref()
        .map(|e| e.source_name())
        .unwrap_or(unresolved.source_file.as_str());
    let base_dir = Path::new(dir_source_name).parent().ok_or_else(|| {
        spanned_err_on_field(
            "cannot determine base directory for dir".to_string(),
            sources,
            &unresolved.dir,
            &unresolved.source_file,
        )
    })?;
    Ok(base_dir.join(&raw).to_string_lossy().to_string())
}

/// Resolve an unresolved project's field expressions against a combined
/// scope that includes both global and project-level vars. Returns the
/// four resolved field values as a tuple `(url, dir, sync, branch)`.
pub(crate) fn resolve_project_fields(
    unresolved: &UnresolvedProject,
    scope: &BucketRegistry<String>,
    sources: &HashMap<String, String>,
    view: &CrossProjectView,
) -> Result<(String, String, SyncMode, Option<String>), CompileError> {
    let url = resolve_optional_expr(&unresolved.url, scope, sources, view)?.unwrap_or_default();
    let dir = resolve_dir_field(unresolved, scope, sources, view)?;
    let sync = match resolve_optional_expr(&unresolved.sync, scope, sources, view)? {
        Some(mode) => parse_sync_mode(&mode).map_err(|msg| {
            spanned_err_on_field(msg, sources, &unresolved.sync, &unresolved.source_file)
        })?,
        None => SyncMode::Clone,
    };
    let branch = resolve_optional_expr(&unresolved.branch, scope, sources, view)?;
    Ok((url, dir, sync, branch))
}

/// Resolve using pre-computed scopes.
///
/// Projects are resolved in dependency order (a project reading
/// `$donor::name` requires `donor` to be fully resolved first), so qualified
/// cross-project variable reads are inlined from the donor's already-resolved
/// buckets/fields. `force_cwd` mirrors the `KIRU_CWD` env var: when set,
/// project-body `var shell` commands run in the current directory instead of
/// the resolved project directory.
pub(crate) fn resolve_with_scopes(
    unresolved: UnresolvedConfig,
    global: BucketRegistry<String>,
    sources: &HashMap<String, String>,
    force_cwd: bool,
    shell_cache: &mut ShellCache,
) -> Result<Plan, CompileError> {
    let order = topo_order_projects(&unresolved.projects)?;

    let mut projects: HashMap<String, PlanProject> = HashMap::new();
    // Every project resolved so far, keyed by name, for cross-project reads.
    let mut resolved: CrossProjectView = HashMap::new();
    let mut seen_dirs: HashSet<String> = HashSet::new();

    for name in order {
        let unresolved_project = &unresolved.projects[&name];

        // 1. Project fields are resolved against the GLOBAL bucket only (plus
        //    any already-resolved donor project via the cross-project view).
        //    They may reference global vars (and earlier fields), never the
        //    project's own body vars — those are encapsulated by the project
        //    and resolved below.
        let (url, dir, sync, branch) =
            resolve_project_fields(unresolved_project, &global, sources, &resolved)?;

        if !dir.is_empty() && !seen_dirs.insert(dir.clone()) {
            return Err(CompileError::ValidationReport(vec![miette!(
                "project {:?}: duplicate directory {:?}",
                name,
                dir
            )]));
        }

        // 2. The project body runs in the resolved project directory (or the
        //    current directory when force_cwd is set / dir is empty).
        let effective_dir: Option<PathBuf> = if force_cwd || dir.is_empty() {
            None
        } else {
            Some(PathBuf::from(&dir))
        };
        let working_dir: Option<&Path> = effective_dir.as_deref();

        // 3. The project body and every function body resolve against the same
        //    project bucket. pr-body, fn-body, and env `var` declarations all
        //    land in this one bucket (fn-body `var` is project-global), and
        //    case arms open a transient per-arm bucket that shadows it.
        let mut project_reg = global.clone();

        // 4. Resolve body var statements once, in the project directory. Any
        //    qualified reference to a donor project reads its already-resolved
        //    bucket via `resolved`.
        for var_stmt in &unresolved_project.var_stmts {
            resolve_project_var(
                var_stmt,
                &mut project_reg,
                working_dir,
                sources,
                shell_cache,
                &resolved,
            )?;
        }

        // Record this project before resolving its function bodies so a
        // function may reference `$self::name` if needed.
        resolved.insert(
            name.clone(),
            ResolvedProjectData {
                url: url.clone(),
                dir: dir.clone(),
                sync: sync.clone(),
                branch: branch.clone(),
                registry: project_reg.clone(),
            },
        );

        // 5. Resolve each function body against the project bucket, with the
        //    full cross-project view available for qualified reads.
        let mut functions = HashMap::new();
        for (fn_name, body) in &unresolved_project.functions {
            let mut resolve_ctx = ResolveFnCtx {
                scope: &mut project_reg,
                working_dir,
                sources,
                shell_cache,
                view: &resolved,
            };
            let resolved_body = resolve_fn_body_stmts(body, &mut resolve_ctx)?;
            functions.insert(fn_name.clone(), resolved_body);
        }

        projects.insert(
            name,
            PlanProject {
                url,
                dir,
                sync,
                branch,
                functions,
                runs: unresolved_project.runs.clone(),
            },
        );
    }

    Ok(Plan { projects })
}

#[cfg(test)]
mod tests {
    use crate::compiler::error::CompileError;
    use crate::compiler::test_support::*;
    use crate::plan::PlanStmt;
    use miette::Report;

    #[test]
    fn test_variable_chain_resolution() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        var string a = `x`;\n\
        var string b = $a;\n\
        var string c = $b;\n\
        pr p [url = $c dir = `d`] { }
        ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.projects["p"].url, "x");
    }

    #[test]
    fn test_interpolation_in_backtick() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        var string name = `world`;\n\
        pr p [url = `http://${name}.com` dir = `d`] { }\
        ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.projects["p"].url, "http://world.com");
    }

    #[test]
    fn test_dir_field_resolves_relative_to_defining_file() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "pr x [url = `u`] { }\n\
             import `sub/build.kiru`;\n",
        );
        write_config(
            &dir.path().join("sub"),
            "build.kiru",
            "pr x [dir = `./overridden`] { }",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        // The `dir` value is defined in sub/build.kiru, so it must resolve
        // relative to that file's directory (sub/), not the first-merged
        // declaration's file (main.kiru at the project root).
        let expected = dir
            .path()
            .join("sub")
            .join("./overridden")
            .to_string_lossy()
            .to_string();
        assert_eq!(cfg.projects["x"].dir, expected);
    }

    #[test]
    fn test_project_field_with_var_ref() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        var string myurl = `http://example.com`;\n\
        pr x [url = $myurl dir = `d`] { }\
        ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.projects["x"].url, "http://example.com");
    }

    #[test]
    fn test_project_var_chain_resolution() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = `u`\n\
            dir = `d`\n\
        ] {\n\
            var string a = `hello`;\n\
            var string b = $a;\n\
        }\
        ",
        );
        // We can't check project vars directly on the resolved Config,
        // but the configuration should compile and resolve without errors.
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(
            cfg.projects["test"].dir,
            dir.path().join("d").to_string_lossy()
        );
    }

    #[test]
    fn test_duplicate_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr a [url = `ua` dir = `shared`] { }\n\
        pr b [url = `ub` dir = `shared`] { }\
        ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("duplicate directory"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_invalid_sync_value() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr p [url = `u` dir = `d` sync = `invalid`] { }\
        ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("sync"), "got: {}", err);
    }

    #[test]
    fn test_project_field_interpolation_cannot_reference_body_var() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = `http://example.com/${name}`\n\
            dir = $name\n\
        ] {\n\
            var string name = `myproject`;\n\
        }\
        ",
        );
        let result = compile_full(&dir.path().join("main.kiru"));
        assert!(result.is_err());
    }

    #[test]
    fn test_kiru_cwd_forces_current_dir_for_project_scope_var_shell() {
        let dir = tempfile::TempDir::new().unwrap();
        let subdir = dir.path().join("projectdir");
        std::fs::create_dir(&subdir).unwrap();
        let current_dir = std::env::current_dir().unwrap();

        write_config(
            dir.path(),
            "main.kiru",
            &format!(
                "\
        pr test [\n\
            url = `http://example.com`\n\
            dir = `{}`\n\
        ] {{\n\
            var shell cwd = `pwd`;\n\
            fn check {{\n\
                log $cwd;\n\
            }}\n\
        }}\n\
        ",
                subdir.to_string_lossy()
            ),
        );

        let cfg = compile_full_with_cwd(&dir.path().join("main.kiru"), true).unwrap();

        let proj = &cfg.projects["test"];
        let fn_body = &proj.functions["check"];
        assert_eq!(fn_body.len(), 1);
        let stmt = match &fn_body[0] {
            PlanStmt::Log(s) => s,
            other => panic!("expected Log statement, got {:?}", other),
        };
        let expected = current_dir.to_string_lossy().to_string();
        assert_eq!(*stmt.value, expected);
    }

    #[test]
    fn test_project_scope_var_shell_uses_project_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let subdir = dir.path().join("myproject");
        std::fs::create_dir(&subdir).unwrap();

        write_config(
            dir.path(),
            "main.kiru",
            &format!(
                "\
        pr test [\n\
            url = `http://example.com`\n\
            dir = `{}`\n\
        ] {{\n\
            var shell cwd = `pwd`;\n\
            fn check {{\n\
                log $cwd;\n\
            }}\n\
        }}\n\
        ",
                subdir.to_string_lossy()
            ),
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["test"];
        let fn_body = &proj.functions["check"];
        assert_eq!(fn_body.len(), 1);
        let stmt = match &fn_body[0] {
            PlanStmt::Log(s) => s,
            other => panic!("expected Log statement, got {:?}", other),
        };
        let expected = std::fs::canonicalize(&subdir)
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(*stmt.value, expected);
    }

    #[test]
    fn test_fn_scope_var_shell_uses_project_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let subdir = dir.path().join("myproject");
        std::fs::create_dir(&subdir).unwrap();

        write_config(
            dir.path(),
            "main.kiru",
            &format!(
                "\
        pr test [\n\
            url = `http://example.com`\n\
            dir = `{}`\n\
        ] {{\n\
            fn check {{\n\
                var shell cwd = `pwd`;\n\
                log $cwd;\n\
            }}\n\
        }}\n\
        ",
                subdir.to_string_lossy()
            ),
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["test"];
        let fn_body = &proj.functions["check"];
        assert_eq!(fn_body.len(), 1); // VarDecl consumed, only log emitted
        let stmt = match &fn_body[0] {
            PlanStmt::Log(s) => s,
            other => panic!("expected Log statement, got {:?}", other),
        };
        let expected = std::fs::canonicalize(&subdir)
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(*stmt.value, expected);
    }

    #[test]
    fn test_global_var_shell_uses_current_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        var shell msg = `echo hello-from-global`;\n\
        pr test [\n\
            url = $msg\n\
            dir = `d`\n\
        ] {\n\
            fn check { log $msg; }\n\
        }\
        ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["test"];
        assert_eq!(proj.url, "hello-from-global");
        let fn_body = &proj.functions["check"];
        let stmt = match &fn_body[0] {
            PlanStmt::Log(s) => s,
            other => panic!("expected Log statement, got {:?}", other),
        };
        assert_eq!(stmt.value, "hello-from-global");
    }

    #[test]
    fn test_field_cannot_reference_project_body_var() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = $x\n\
            dir = $x\n\
        ] {\n\
            var shell x = `echo workspace`;\n\
            fn check { log $x; }\n\
        }\
        ",
        );
        // Fields are resolved against the global scope only and may not reach
        // into the project body, so a field referencing a body var is an
        // undefined-variable error.  There is no cycle to deadlock on because
        // the project directory is computed before the body is ever resolved.
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("undefined variable"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_fn_body_redeclaration_reports_span_without_out_of_bounds() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
            pr test [ url = `u` dir = `d` ] {
                var string docker_bin = `x`;
                fn check {
                    var string docker_bin = `y`;
                }
            }
            ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        let report: &Report = match &err {
            CompileError::ValidationReport(reports) => &reports[0],
            other => panic!("expected ValidationReport, got {}", other),
        };
        // Render through the graphical handler — this is exactly where the
        // `[Failed to read contents for label <none> ... OutOfBounds]` artifact
        // used to leak when function-body spans pointed at an empty source.
        let _ = miette::set_hook(Box::new(|_| {
            Box::new(miette::MietteHandlerOpts::new().build())
        }));
        let rendered = format!("{:?}", report);
        assert!(
            rendered.contains("already defined at project"),
            "got: {}",
            rendered
        );
        assert!(
            !rendered.contains("OutOfBounds"),
            "diagnostic leaked an out-of-bounds artifact: {}",
            rendered
        );
        assert!(
            !rendered.contains("<none>"),
            "diagnostic used a default <none> source name: {}",
            rendered
        );
    }

    #[test]
    fn test_project_var_shell_runs_once() {
        let dir = tempfile::TempDir::new().unwrap();
        let subdir = dir.path().join("myproject");
        std::fs::create_dir(&subdir).unwrap();
        let marker = subdir.join("run_count.txt");

        write_config(
            dir.path(),
            "main.kiru",
            &format!(
                "\
        pr test [\n\
            url = `http://example.com`\n\
            dir = `{}`\n\
        ] {{\n\
            var shell x = `echo 1 >> {} && echo done`;\n\
            fn check {{\n\
                log $x;\n\
            }}\n\
        }}\n\
        ",
                subdir.to_string_lossy(),
                marker.to_string_lossy(),
            ),
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["test"];
        let fn_body = &proj.functions["check"];
        assert_eq!(fn_body.len(), 1);
        let stmt = match &fn_body[0] {
            PlanStmt::Log(s) => s,
            other => panic!("expected Log statement, got {:?}", other),
        };
        assert_eq!(stmt.value, "done");
        let count = std::fs::read_to_string(&marker).unwrap();
        assert_eq!(
            count.lines().count(),
            1,
            "var shell should execute exactly once, got {} lines",
            count.lines().count()
        );
    }

    #[test]
    fn test_cross_project_field_read_resolves_donor_first() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
            var string base = `http://base`;\n\
            pr a [url = $base dir = `da`] { }\n\
            pr b [url = $a::url dir = `db`] { }\
            ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        // `b` reads `a`'s `url` field, which is itself a global var reference.
        // Resolution must order `a` before `b` so the donor is available.
        assert_eq!(cfg.projects["b"].url, "http://base");
        assert_eq!(cfg.projects["a"].url, "http://base");
    }

    #[test]
    fn test_cross_project_var_read_from_donor_body() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
            pr a [url = `ua` dir = `da`] {\n\
                var string shared = `VALUE`;\n\
            }\n\
            pr b [url = $a::shared dir = `db`] { }\
            ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        // `b` reads `shared` from `a`'s project bucket (not a field).
        assert_eq!(cfg.projects["b"].url, "VALUE");
    }

    #[test]
    fn test_unknown_cross_project_reference_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
            pr b [url = $nope::url dir = `db`] { }\
            ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("unknown project"), "got: {}", err);
    }

    #[test]
    fn test_cyclic_cross_project_dependency_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
            pr a [url = $b::url dir = `da`] { }\n\
            pr b [url = $a::url dir = `db`] { }\
            ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("cyclic"), "got: {}", err);
    }
}
