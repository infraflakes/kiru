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
use crate::dsl::{Expr, VarType};
use crate::error::{SourceFile, spanned_report};
use crate::shell::execute_shell_variable;
use std::collections::HashMap;
use std::path::Path;

/// The single compile-time resolution map. `lookup_var` is the one and only
/// lookup path used by every resolver and validator.
#[derive(Debug, Clone, Default)]
pub(crate) struct Namespaces {
    pub(crate) global: HashMap<String, String>,
    pub(crate) projects: HashMap<String, HashMap<String, String>>,
}

impl Namespaces {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Insert `name => value` into `map` if absent, erroring on an exact
    /// duplicate `ns::name`. The single duplicate-declaration guard shared by
    /// the global and project declaration paths.
    #[allow(clippy::too_many_arguments)]
    fn declare_unique(
        map: &mut HashMap<String, String>,
        ns: &str,
        name: &str,
        value: String,
        source_name: &str,
        offset: usize,
        len: usize,
        sources: &HashMap<String, String>,
    ) -> Result<(), CompileError> {
        if map.contains_key(name) {
            return Err(redeclaration_err(
                ns,
                name,
                source_name,
                offset,
                len,
                sources,
            ));
        }
        map.insert(name.to_string(), value);
        Ok(())
    }

    /// Look up `ns::name` and return its resolved value.
    ///
    /// - `ns == "global"` searches the global variable map.
    /// - otherwise the named project is looked up and `name` resolves to a
    ///   project variable. There is no fallback between namespaces. A project's
    ///   `url`/`dir`/`sync`/`branch` metadata fields are never referenceable.
    pub(crate) fn lookup_var(&self, ns: &str, name: &str) -> Option<&String> {
        if ns == "global" {
            return self.global.get(name);
        }
        self.projects.get(ns)?.get(name)
    }

    /// Whether `name` is a project variable of `ns` (declared in its body or
    /// injected by a function binding). Function-body locals live in their own
    /// per-function map and are not consulted here.
    pub(crate) fn project_var_exists(&self, ns: &str, name: &str) -> bool {
        self.projects
            .get(ns)
            .is_some_and(|entry| entry.contains_key(name))
    }

    /// Whether `ns` is a known namespace (`global` or a declared project).
    pub(crate) fn contains_ns(&self, ns: &str) -> bool {
        ns == "global" || self.projects.contains_key(ns)
    }

    /// Declare a top-level variable into the `global` namespace, erroring on an
    /// exact duplicate `global::name`.
    pub(crate) fn declare_global(
        &mut self,
        name: &str,
        value: String,
        source_name: &str,
        offset: usize,
        len: usize,
        sources: &HashMap<String, String>,
    ) -> Result<(), CompileError> {
        Self::declare_unique(
            &mut self.global,
            "global",
            name,
            value,
            source_name,
            offset,
            len,
            sources,
        )
    }

    /// Register a project namespace. Project bodies may be merged across files
    /// (several `pr name` blocks combine), so a second registration of the same
    /// name is idempotent rather than an error — only an exact duplicate
    /// `name::var` (handled by `declare_project_var`) is rejected.
    pub(crate) fn declare_project(&mut self, name: &str) -> Result<(), CompileError> {
        self.projects.entry(name.to_string()).or_default();
        Ok(())
    }

    /// Declare a project variable into `ns`'s namespace, erroring on an exact
    /// duplicate `ns::name`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn declare_project_var(
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
        Self::declare_unique(entry, ns, name, value, source_name, offset, len, sources)
    }

    /// Set (overwrite) a project variable.
    pub(crate) fn set_project_var(&mut self, ns: &str, name: &str, value: String) {
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

/// Look up `ns::name` and produce the matching resolution error: an undefined
/// variable when the namespace exists, an unknown namespace when it does not.
/// The single implementation of the lookup + error fork shared by variable
/// references, interpolation parts, and case-pattern references.
pub(crate) fn lookup_var_or_err(
    namespaces: &Namespaces,
    ns: &str,
    name: &str,
    offset: usize,
    len: usize,
    sources: &HashMap<String, String>,
    source_name: &str,
) -> Result<String, CompileError> {
    match namespaces.lookup_var(ns, name) {
        Some(val) => Ok(val.clone()),
        None => Err(if namespaces.contains_ns(ns) {
            undefined_var_err(ns, name, offset, len, sources, source_name)
        } else {
            unknown_namespace_err(ns, offset, len, sources, source_name)
        }),
    }
}

/// Resolve a variable's declared value to its final string: plain `var`
/// values are kept as-is, `var shell` values are executed as a shell command
/// at compile time. The single implementation of the `var shell` fork shared
/// by global, project, and function-local variable declarations.
pub(crate) fn resolve_var_value(
    var_type: &VarType,
    name: &str,
    resolved_value: String,
    working_dir: Option<&Path>,
    source: &SourceFile,
    offset: usize,
    len: usize,
) -> Result<String, CompileError> {
    if *var_type == VarType::Shell {
        execute_shell_variable(&resolved_value, working_dir).map_err(|e| {
            CompileError::ValidationReport(vec![spanned_report(
                format!("shell var ${} failed: {}", name, e),
                source,
                offset,
                len,
            )])
        })
    } else {
        Ok(resolved_value)
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
        } => lookup_var_or_err(
            namespaces,
            namespace,
            name,
            *offset,
            *len,
            sources,
            source_name,
        ),
        Expr::BacktickLit {
            parts,
            offset,
            len,
            source_name,
        } => {
            let mut result = String::new();
            for part in parts {
                if part.is_var {
                    result.push_str(&lookup_var_or_err(
                        namespaces,
                        &part.namespace,
                        &part.value,
                        *offset,
                        *len,
                        sources,
                        source_name,
                    )?);
                } else {
                    result.push_str(&part.value);
                }
            }
            Ok(result)
        }
    }
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
                    result.push_str(&lookup_var_or_err(
                        namespaces,
                        &part.namespace,
                        &part.value,
                        *offset,
                        *len,
                        sources,
                        source_name,
                    )?);
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
        } => Ok(crate::plan::PlanCasePattern::Literal(lookup_var_or_err(
            namespaces,
            namespace,
            name,
            *offset,
            *len,
            sources,
            source_name,
        )?)),
        crate::dsl::CasePattern::Default => Ok(crate::plan::PlanCasePattern::Default),
    }
}
