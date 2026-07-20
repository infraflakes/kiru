/// A DSL expression: either a backtick-quoted string (possibly with variable interpolation)
/// or a variable reference ($name or ${name}).
#[derive(Debug, Clone)]
pub enum Expr {
    BacktickLit {
        parts: Vec<InterpolationPart>,
        offset: usize,
        len: usize,
        /// Canonical path of the `.kiru` file this expression was parsed from.
        /// Carried on every node so diagnostics resolve against the correct
        /// source when a project body is merged across several files.
        source_name: String,
    },
    VarRef {
        name: String,
        /// Project namespace qualifier of this reference (e.g. `global` or a
        /// project name). Always present — references are always written
        /// `namespace::name` and never bare. Populated by the parser;
        /// resolution looks the name up in exactly this namespace.
        namespace: String,
        offset: usize,
        len: usize,
        /// Canonical path of the `.kiru` file this expression was parsed from.
        /// Carried on every node so diagnostics resolve against the correct
        /// source when a project body is merged across several files.
        source_name: String,
    },
}

impl Expr {
    /// Returns the source span `(offset, len)` for this expression.
    /// Both variants carry identical offset/len fields.
    pub fn offset_len(&self) -> (usize, usize) {
        match self {
            Expr::BacktickLit { offset, len, .. } => (*offset, *len),
            Expr::VarRef { offset, len, .. } => (*offset, *len),
        }
    }

    /// Returns the canonical path of the `.kiru` file this expression was
    /// parsed from. Carried on every node so diagnostics resolve against the
    /// correct source when a project body is merged across several files.
    pub fn source_name(&self) -> &str {
        match self {
            Expr::BacktickLit { source_name, .. } => source_name,
            Expr::VarRef { source_name, .. } => source_name,
        }
    }

    /// Invoke `f` with every variable this expression references, whether as a
    /// `$namespace::name` reference or an interpolation `${namespace::name}`
    /// inside a backtick literal. Both arguments are always present: `name`
    /// then `namespace` — there is no bare (unqualified) reference form.
    ///
    /// Defined once per node type so the var walk is centralized: adding an
    /// `Expr` variant requires extending only this method (and that variant's
    /// own resolve), not every call site that collects referenced variables.
    pub fn visit_vars(&self, f: &mut impl FnMut(&str, &str)) {
        match self {
            Expr::VarRef {
                namespace, name, ..
            } => f(name, namespace),
            Expr::BacktickLit { parts, .. } => {
                for part in parts {
                    if part.is_var {
                        f(&part.value, &part.namespace);
                    }
                }
            }
        }
    }

    /// Invoke `f` with a mutable handle to the namespace of every variable this
    /// expression references, plus the span `(offset, len, source_name)` that
    /// locates the reference. The mutable handle lets a normalization pass
    /// rewrite a namespace in place (e.g. the `self` alias into the enclosing
    /// scope name). Mirrors [`Expr::visit_vars`] so the namespace walk stays
    /// defined once per node kind.
    pub fn visit_namespaces_mut(&mut self, f: &mut impl FnMut(&mut String, usize, usize, &str)) {
        match self {
            Expr::VarRef {
                namespace,
                offset,
                len,
                source_name,
                ..
            } => f(namespace, *offset, *len, source_name),
            Expr::BacktickLit {
                parts,
                offset,
                len,
                source_name,
            } => {
                for part in parts {
                    if part.is_var {
                        f(&mut part.namespace, *offset, *len, source_name);
                    }
                }
            }
        }
    }
}

/// A segment of a backtick-quoted expression.
/// If `is_var` is true, `value` is a variable name to substitute; otherwise it is literal text.
#[derive(Debug, Clone)]
pub struct InterpolationPart {
    pub is_var: bool,
    /// Project namespace qualifier for `is_var` parts (e.g. the `nix` in
    /// `${nix::url}`). Always present — interpolation requires `namespace::name`.
    pub namespace: String,
    pub value: String,
}

/// The type of a variable declaration.
#[derive(Debug, Clone, PartialEq)]
pub enum VarType {
    /// `var` — plain string value.
    String,
    /// `var shell` — value is executed as a shell command at compile time.
    Shell,
}

/// A pattern arm inside a `match` expression.
#[derive(Debug, Clone)]
pub enum CasePattern {
    Literal {
        parts: Vec<InterpolationPart>,
        offset: usize,
        len: usize,
        /// Canonical path of the `.kiru` file this pattern was parsed from.
        source_name: String,
    },
    VarRef {
        name: String,
        /// Project namespace qualifier from a `ns::name` pattern (e.g.
        /// `nix::url`). Always present — references are always `namespace::name`.
        namespace: String,
        offset: usize,
        len: usize,
        /// Canonical path of the `.kiru` file this pattern was parsed from.
        source_name: String,
    },
    Default,
}

impl CasePattern {
    /// Returns the source span `(offset, len)` for this pattern. `Default`
    /// carries no span, so it returns `(0, 0)` — callers only use this for
    /// non-default patterns, where a variable reference is being reported.
    pub fn offset_len(&self) -> (usize, usize) {
        match self {
            CasePattern::Literal { offset, len, .. } => (*offset, *len),
            CasePattern::VarRef { offset, len, .. } => (*offset, *len),
            CasePattern::Default => (0, 0),
        }
    }

    /// Returns the canonical path of the `.kiru` file this pattern was parsed
    /// from. Used to resolve the diagnostic span against the correct source
    /// when a project body is merged across several files.
    pub fn source_name(&self) -> &str {
        match self {
            CasePattern::Literal { source_name, .. } => source_name,
            CasePattern::VarRef { source_name, .. } => source_name,
            CasePattern::Default => "",
        }
    }

    /// Invoke `f` with the name of every variable this pattern references,
    /// including `$namespace::name` and backtick interpolation
    /// `${namespace::name}`, and default (`_`) patterns (which reference no
    /// variables). Both arguments are always present (`name`, then
    /// `namespace`). Mirrors [`Expr::visit_vars`] so the var-walk API is
    /// uniform across node kinds.
    pub fn visit_vars(&self, f: &mut impl FnMut(&str, &str)) {
        match self {
            CasePattern::VarRef {
                namespace, name, ..
            } => f(name, namespace),
            CasePattern::Literal { parts, .. } => {
                for part in parts {
                    if part.is_var {
                        f(&part.value, &part.namespace);
                    }
                }
            }
            CasePattern::Default => {}
        }
    }

    /// Invoke `f` with a mutable handle to the namespace of every variable this
    /// pattern references, plus its span. Mirrors [`Expr::visit_namespaces_mut`]
    /// so a normalization pass can rewrite the `self` alias inside case patterns.
    pub fn visit_namespaces_mut(&mut self, f: &mut impl FnMut(&mut String, usize, usize, &str)) {
        match self {
            CasePattern::VarRef {
                namespace,
                offset,
                len,
                source_name,
                ..
            } => f(namespace, *offset, *len, source_name),
            CasePattern::Literal {
                parts,
                offset,
                len,
                source_name,
            } => {
                for part in parts {
                    if part.is_var {
                        f(&mut part.namespace, *offset, *len, source_name);
                    }
                }
            }
            CasePattern::Default => {}
        }
    }
}

/// A single arm of a `match` block: a pattern and its body statements.
#[derive(Debug, Clone)]
pub struct CaseArm {
    pub pattern: CasePattern,
    pub body: Vec<crate::dsl::fnstmt::FnStmt>,
}

/// A key-value pair for `env` blocks.
#[derive(Debug, Clone)]
pub struct EnvPair {
    pub key: String,
    pub value: Expr,
}
