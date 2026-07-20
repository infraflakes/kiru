//! # Namespaces — the single compile-time resolution map
//!
//! kiru is an IaC task runner, not a general-purpose language, and the runner
//! has no notion of runtime scope: `crate::plan` inlines every variable to a
//! `String` at compile time. Scoping therefore exists only to (1) detect
//! duplicate declarations and (2) resolve `namespace::name` references at
//! compile time.
//!
//! There is exactly one map. Namespaces are exactly:
//!
//! - **`global`** — variables declared at the top level (outside any `pr`).
//! - **`<project>`** — the variables declared in its body, any `fn` body, or any
//!   `env`/`case` block. All of those collapse into one project namespace. A
//!   project's `url`/`dir`/`sync`/`branch` metadata fields are runner-internal
//!   and are never referenceable.
//!
//! Resolution is a single lookup: `namespaces.get(ns, name)`. There is no
//! precedence chain, no shadow/ancestor rule, and no cross-bucket duplicate
//! logic. A redeclaration is an error *only* on an exact duplicate
//! `namespace::name`. See `plan.md` section 2 for the authoritative spec.

use crate::compiler::error::{CompileError, spanned_err_named};
use crate::dsl::Expr;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// The fully resolved state of one project: its four metadata fields plus the
/// variables declared into its namespace.
#[derive(Debug, Clone, Default)]
pub struct ProjectNamespace {
    pub vars: HashMap<String, String>,
}

/// The single compile-time resolution map. `get` is the one and only lookup
/// path used by every resolver and validator.
#[derive(Debug, Clone, Default)]
pub struct Namespaces {
    pub global: HashMap<String, String>,
    pub projects: HashMap<String, ProjectNamespace>,
    /// Names of variables declared inside function bodies, per project. A
    /// project's metadata fields (`url`/`dir`/`sync`/`branch`) may reference
    /// config variables (globals and project-body / donor variables) but never
    /// a function-body variable, so those names are tracked separately to
    /// reject such references.
    fn_body_var_names: HashMap<String, HashSet<String>>,
}

impl Namespaces {
    pub fn new() -> Self {
        Namespaces {
            global: HashMap::new(),
            projects: HashMap::new(),
            fn_body_var_names: HashMap::new(),
        }
    }

    /// Look up `ns::name` in exactly one place.
    ///
    /// - `ns == "global"` searches the global variable map.
    /// - otherwise the named project is looked up and `name` resolves to a
    ///   project variable. There is no fallback between namespaces. A project's
    ///   `url`/`dir`/`sync`/`branch` metadata fields are never referenceable.
    pub fn get(&self, ns: &str, name: &str) -> Option<&String> {
        if ns == "global" {
            return self.global.get(name);
        }
        self.projects.get(ns)?.vars.get(name)
    }

    /// Whether `ns` is a known namespace (`global` or a declared project).
    pub fn contains_ns(&self, ns: &str) -> bool {
        ns == "global" || self.projects.contains_key(ns)
    }

    /// Declare a top-level variable into the `global` namespace, erroring on an
    /// exact duplicate `global::name`.
    pub fn declare_global(
        &mut self,
        name: &str,
        value: String,
        source_name: &str,
        offset: usize,
        len: usize,
        sources: &HashMap<String, String>,
    ) -> Result<(), CompileError> {
        if self.global.contains_key(name) {
            return Err(redeclaration_err(
                "global",
                name,
                source_name,
                offset,
                len,
                sources,
            ));
        }
        self.global.insert(name.to_string(), value);
        Ok(())
    }

    /// Register a project namespace. Project bodies may be merged across files
    /// (several `pr name` blocks combine), so a second registration of the same
    /// name is idempotent rather than an error — only an exact duplicate
    /// `name::var` (handled by `declare_project_var`) is rejected.
    pub fn declare_project(
        &mut self,
        name: &str,
        source_name: &str,
        offset: usize,
        len: usize,
        sources: &HashMap<String, String>,
    ) -> Result<(), CompileError> {
        self.projects.entry(name.to_string()).or_default();
        let _ = (source_name, offset, len, sources);
        Ok(())
    }

    /// Declare a project variable into `ns`'s namespace, erroring on an exact
    /// duplicate `ns::name`.
    #[allow(clippy::too_many_arguments)]
    pub fn declare_project_var(
        &mut self,
        ns: &str,
        name: &str,
        value: String,
        source_name: &str,
        offset: usize,
        len: usize,
        sources: &HashMap<String, String>,
    ) -> Result<(), CompileError> {
        let entry = self.projects.entry(ns.to_string()).or_default();
        if entry.vars.contains_key(name) {
            return Err(redeclaration_err(
                ns,
                name,
                source_name,
                offset,
                len,
                sources,
            ));
        }
        entry.vars.insert(name.to_string(), value);
        Ok(())
    }

    /// Record that `name` is a function-body variable of project `ns`. Function
    /// bodies are the only place these names may be referenced; a project's
    /// metadata fields must never read them.
    pub fn declare_fn_body_var(&mut self, ns: &str, name: &str) {
        self.fn_body_var_names
            .entry(ns.to_string())
            .or_default()
            .insert(name.to_string());
    }

    /// Whether `name` is a function-body variable of project `ns` (and thus
    /// forbidden from being referenced by a metadata field expression).
    pub fn is_fn_body_var(&self, ns: &str, name: &str) -> bool {
        self.fn_body_var_names
            .get(ns)
            .is_some_and(|names| names.contains(name))
    }

    /// Set (overwrite) a project variable.
    pub fn set_project_var(&mut self, ns: &str, name: &str, value: String) {
        self.projects
            .entry(ns.to_string())
            .or_default()
            .vars
            .insert(name.to_string(), value);
    }
}

/// Build the "undefined variable" error for a `ns::name` reference absent from
/// the namespaces map. Centralizes the `format!("undefined variable: ...")`
/// construction used by expression and case-pattern resolution.
pub(crate) fn undefined_var_err(
    ns: &str,
    name: &str,
    offset: usize,
    len: usize,
    sources: &HashMap<String, String>,
    source_name: &str,
) -> CompileError {
    spanned_err_named(
        format!("undefined variable: {}::{}", ns, name),
        sources,
        source_name,
        offset,
        len,
    )
}

/// Build the "unknown project" error for a reference into a namespace that was
/// never declared.
pub(crate) fn unknown_namespace_err(
    ns: &str,
    offset: usize,
    len: usize,
    sources: &HashMap<String, String>,
    source_name: &str,
) -> CompileError {
    spanned_err_named(
        format!("unknown project namespace: {}", ns),
        sources,
        source_name,
        offset,
        len,
    )
}

/// Build a spanned redeclaration error for an exact `ns::name` duplicate.
fn redeclaration_err(
    ns: &str,
    name: &str,
    source_name: &str,
    offset: usize,
    len: usize,
    sources: &HashMap<String, String>,
) -> CompileError {
    spanned_err_named(
        format!("${}::{} is already defined", ns, name),
        sources,
        source_name,
        offset,
        len,
    )
}

/// Resolve the optional `Expr` field to a concrete string against `namespaces`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_optional_expr(
    expr: &Option<Expr>,
    namespaces: &Namespaces,
    sources: &HashMap<String, String>,
) -> Result<Option<String>, CompileError> {
    match expr {
        Some(e) => {
            let resolved = resolve_expr(e, namespaces, sources)?;
            if resolved.is_empty() {
                Ok(None)
            } else {
                Ok(Some(resolved))
            }
        }
        None => Ok(None),
    }
}

/// Resolve an `Expr` against the single namespaces map: a `VarRef` is one
/// lookup; a backtick literal concatenates its parts, substituting variable
/// parts via the same lookup. No `Some`/`None` fork — every reference carries
/// its namespace.
pub(crate) fn resolve_expr(
    expr: &Expr,
    namespaces: &Namespaces,
    sources: &HashMap<String, String>,
) -> Result<String, CompileError> {
    match expr {
        Expr::VarRef {
            namespace,
            name,
            offset,
            len,
            source_name,
        } => match namespaces.get(namespace, name) {
            Some(val) => Ok(val.clone()),
            None => {
                if namespaces.contains_ns(namespace) {
                    Err(undefined_var_err(
                        namespace,
                        name,
                        *offset,
                        *len,
                        sources,
                        source_name,
                    ))
                } else {
                    Err(unknown_namespace_err(
                        namespace,
                        *offset,
                        *len,
                        sources,
                        source_name,
                    ))
                }
            }
        },
        Expr::BacktickLit {
            parts,
            offset,
            len,
            source_name,
        } => {
            let mut result = String::new();
            for part in parts {
                if part.is_var {
                    match namespaces.get(&part.namespace, &part.value) {
                        Some(val) => result.push_str(val),
                        None => {
                            let err = if namespaces.contains_ns(&part.namespace) {
                                undefined_var_err(
                                    &part.namespace,
                                    &part.value,
                                    *offset,
                                    *len,
                                    sources,
                                    source_name,
                                )
                            } else {
                                unknown_namespace_err(
                                    &part.namespace,
                                    *offset,
                                    *len,
                                    sources,
                                    source_name,
                                )
                            };
                            return Err(err);
                        }
                    }
                } else {
                    result.push_str(&part.value);
                }
            }
            Ok(result)
        }
    }
}

/// Resolve a `dir` field, joining relative paths against the source file's
/// directory so that `dir = \`./foo\`` resolves relative to the `.kiru` file.
pub(crate) fn resolve_dir_field(
    unresolved: &crate::compiler::types::UnresolvedProject,
    namespaces: &Namespaces,
    sources: &HashMap<String, String>,
) -> Result<String, CompileError> {
    let raw = resolve_optional_expr(&unresolved.dir, namespaces, sources)?.unwrap_or_default();
    if raw.is_empty() || Path::new(&raw).is_absolute() {
        return Ok(raw);
    }
    let dir_source_name = unresolved
        .dir
        .as_ref()
        .map(|e| e.source_name())
        .unwrap_or(unresolved.source_file.as_str());
    let base_dir = Path::new(dir_source_name).parent().ok_or_else(|| {
        crate::compiler::error::spanned_err_on_field(
            "cannot determine base directory for dir".to_string(),
            sources,
            &unresolved.dir,
            &unresolved.source_file,
        )
    })?;
    Ok(base_dir.join(&raw).to_string_lossy().to_string())
}

/// Resolve a case pattern's literal/var-ref against the namespaces map.
pub(crate) fn resolve_case_pattern(
    pattern: &crate::dsl::CasePattern,
    namespaces: &Namespaces,
    sources: &HashMap<String, String>,
) -> Result<crate::plan::PlanCasePattern, CompileError> {
    match pattern {
        crate::dsl::CasePattern::Literal {
            parts,
            offset,
            len,
            source_name,
        } => {
            let mut result = String::new();
            for part in parts {
                if part.is_var {
                    match namespaces.get(&part.namespace, &part.value) {
                        Some(val) => result.push_str(val),
                        None => {
                            return Err(if namespaces.contains_ns(&part.namespace) {
                                undefined_var_err(
                                    &part.namespace,
                                    &part.value,
                                    *offset,
                                    *len,
                                    sources,
                                    source_name,
                                )
                            } else {
                                unknown_namespace_err(
                                    &part.namespace,
                                    *offset,
                                    *len,
                                    sources,
                                    source_name,
                                )
                            });
                        }
                    }
                } else {
                    result.push_str(&part.value);
                }
            }
            Ok(crate::plan::PlanCasePattern::Literal(result))
        }
        crate::dsl::CasePattern::VarRef {
            namespace,
            name,
            offset,
            len,
            source_name,
        } => match namespaces.get(namespace, name) {
            Some(val) => Ok(crate::plan::PlanCasePattern::Literal(val.clone())),
            None => Err(if namespaces.contains_ns(namespace) {
                undefined_var_err(namespace, name, *offset, *len, sources, source_name)
            } else {
                unknown_namespace_err(namespace, *offset, *len, sources, source_name)
            }),
        },
        crate::dsl::CasePattern::Default => Ok(crate::plan::PlanCasePattern::Default),
    }
}

/// Collect every donor project name referenced by qualified variable reads in
/// `proj` (project-body vars, fields, and function bodies). A donor is any
/// namespace that is a declared project and is not `global` and not the project
/// itself.
pub(crate) fn collect_donor_projects(
    proj_name: &str,
    proj: &crate::compiler::types::UnresolvedProject,
    all_projects: &HashMap<String, crate::compiler::types::UnresolvedProject>,
    donors: &mut Vec<String>,
) {
    let visit_expr = |expr: &Expr, donors: &mut Vec<String>| {
        expr.visit_vars(&mut |_: &str, ns: &str| {
            if ns != "global" && ns != proj_name && all_projects.contains_key(ns) {
                donors.push(ns.to_string());
            }
        });
    };
    for var_stmt in &proj.var_stmts {
        visit_expr(&var_stmt.value, donors);
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
        visit_expr(field, donors);
    }
    for body in proj.functions.values() {
        for stmt in body {
            stmt.visit_vars(&mut |_, ns| {
                if ns != "global" && ns != proj_name && all_projects.contains_key(ns) {
                    donors.push(ns.to_string());
                }
            });
        }
    }
}

/// Order project names so every donor project is resolved before the projects
/// that read from it. Errors on a reference to an unknown project or on a
/// cyclic dependency (which could never be resolved).
pub(crate) fn topo_order_projects(
    projects: &HashMap<String, crate::compiler::types::UnresolvedProject>,
) -> Result<Vec<String>, CompileError> {
    use std::collections::VecDeque;

    let present: std::collections::HashSet<&str> = projects.keys().map(String::as_str).collect();
    let mut donors: HashMap<String, Vec<String>> = HashMap::new();
    for (name, proj) in projects {
        let mut ds = Vec::new();
        collect_donor_projects(name, proj, projects, &mut ds);
        for donor in &ds {
            if !present.contains(donor.as_str()) {
                return Err(CompileError::ValidationReport(vec![miette::miette!(
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
        return Err(CompileError::ValidationReport(vec![miette::miette!(
            "cyclic cross-project variable dependency involving: {:?}",
            in_cycle
        )]));
    }
    Ok(order)
}
