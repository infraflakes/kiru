/// A DSL expression: either a backtick-quoted string (possibly with variable interpolation)
/// or a variable reference ($name or ${name}).
#[derive(Debug, Clone)]
pub enum Expr {
    BacktickLit {
        parts: Vec<InterpolationPart>,
        offset: usize,
        len: usize,
    },
    VarRef {
        name: String,
        offset: usize,
        len: usize,
    },
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
    },
    VarRef {
        name: String,
        offset: usize,
        len: usize,
    },
    Default,
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
