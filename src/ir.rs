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

impl Expr {
    pub fn span(&self) -> (usize, usize) {
        match self {
            Expr::BacktickLit { offset, len, .. } => (*offset, *len),
            Expr::VarRef { offset, len, .. } => (*offset, *len),
        }
    }
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

#[allow(clippy::enum_variant_names)]
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
        arms: Vec<CaseArm>,
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
