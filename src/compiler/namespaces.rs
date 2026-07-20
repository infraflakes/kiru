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
//! Resolution is a single lookup: `namespaces.lookup_var(ns, name)`. There is no
//! precedence chain, no shadow/ancestor rule, and no cross-bucket duplicate
//! logic. A redeclaration is an error *only* on an exact duplicate
//! `namespace::name`. See `plan.md` section 2 for the authoritative spec.

use crate::compiler::error::{CompileError, spanned_err_named};
use crate::dsl::Expr;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// The single compile-time resolution map. `lookup_var` is the one and only
/// lookup path used by every resolver and validator.
#[derive(Debug, Clone, Default)]
pub struct Namespaces {
    pub global: HashMap<String, String>,
    pub projects: HashMap<String, HashMap<String, String>>,
    /// Names of variables declared inside function bodies, per project. A
    /// project's metadata fields (`url`/`dir`/`sync`/`branch`) may reference
    /// config variables (globals and this project's own body variables) but
    /// never a function-body variable, so those names are tracked separately to
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

    /// Look up `ns::name` and return its resolved value.
    ///
    /// - `ns == "global"` searches the global variable map.
    /// - otherwise the named project is looked up and `name` resolves to a
    ///   project variable. There is no fallback between namespaces. A project's
    ///   `url`/`dir`/`sync`/`branch` metadata fields are never referenceable.
    pub fn lookup_var(&self, ns: &str, name: &str) -> Option<&String> {
        if ns == "global" {
            return self.global.get(name);
        }
        self.projects.get(ns)?.get(name)
    }

    /// Whether `name` is a project variable of `ns` (declared in its body or
    /// injected by a function binding). Function-body locals live in their own
    /// per-function map and are not consulted here.
    pub fn project_var_exists(&self, ns: &str, name: &str) -> bool {
        self.projects
            .get(ns)
            .is_some_and(|entry| entry.contains_key(name))
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
    pub fn declare_project(&mut self, name: &str) -> Result<(), CompileError> {
        self.projects.entry(name.to_string()).or_default();
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
        if entry.contains_key(name) {
            return Err(redeclaration_err(
                ns,
                name,
                source_name,
                offset,
                len,
                sources,
            ));
        }
        entry.insert(name.to_string(), value);
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
        } => {
            let resolved = namespaces.lookup_var(namespace, name);
            match resolved {
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
            }
        }
        Expr::BacktickLit {
            parts,
            offset,
            len,
            source_name,
        } => {
            let mut result = String::new();
            for part in parts {
                if part.is_var {
                    let val = namespaces.lookup_var(&part.namespace, &part.value);
                    match val {
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
                    let val = namespaces.lookup_var(&part.namespace, &part.value);
                    match val {
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
        } => match namespaces.lookup_var(namespace, name) {
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
