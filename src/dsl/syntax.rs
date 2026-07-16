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
        offset: usize,
        len: usize,
        /// Canonical path of the `.kiru` file this expression was parsed from.
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
}

/// A segment of a backtick-quoted expression.
/// If `is_var` is true, `value` is a variable name to substitute; otherwise it is literal text.
#[derive(Debug, Clone)]
pub struct InterpolationPart {
    pub is_var: bool,
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

/// A statement that appears inside a function body.
#[derive(Debug, Clone)]
pub enum FnStmt {
    /// `log <expr>` — prints the evaluated expression at runtime.
    Log { value: Expr },
    /// `exec <expr>` — executes the evaluated expression as a shell command.
    Exec { value: Expr },
    /// `cd <expr>` — changes the working directory for subsequent stmts.
    Cd { value: Expr },
    /// `var` or `var shell` inside a function body.
    VarDecl {
        var_type: VarType,
        name: String,
        value: Expr,
    },
    /// `env { ... }` — sets environment variables for the enclosed block.
    EnvBlock {
        pairs: Vec<EnvPair>,
        body: Vec<FnStmt>,
    },
    /// `match <expr> { ... }` — conditional branching.
    Case {
        condition: Expr,
        scopes: Vec<CaseArm>,
    },
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
}

/// A single arm of a `match` block: a pattern and its body statements.
#[derive(Debug, Clone)]
pub struct CaseArm {
    pub pattern: CasePattern,
    pub body: Vec<FnStmt>,
}

/// A key-value pair for `env` blocks.
#[derive(Debug, Clone)]
pub struct EnvPair {
    pub key: String,
    pub value: Expr,
}
