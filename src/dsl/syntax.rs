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

#[derive(Debug, Clone)]
pub struct InterpolationPart {
    pub is_var: bool,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VarType {
    String,
    Shell,
}

#[derive(Debug, Clone)]
pub enum FnStmt {
    Log {
        value: Expr,
    },
    Exec {
        value: Expr,
    },
    Cd {
        value: Expr,
    },
    VarDecl {
        var_type: VarType,
        name: String,
        value: Expr,
    },
    EnvBlock {
        pairs: Vec<EnvPair>,
        body: Vec<FnStmt>,
    },
    Case {
        condition: Expr,
        scopes: Vec<CaseArm>,
    },
}

#[derive(Debug, Clone)]
pub enum CasePattern {
    Literal { parts: Vec<InterpolationPart> },
    VarRef { name: String },
    Default,
}

#[derive(Debug, Clone)]
pub struct CaseArm {
    pub pattern: CasePattern,
    pub body: Vec<FnStmt>,
}

#[derive(Debug, Clone)]
pub struct EnvPair {
    pub key: String,
    pub value: Expr,
}
