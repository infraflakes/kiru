#[derive(Debug, Clone)]
pub enum Expr {
    BacktickLit {
        parts: Vec<TemplatePart>,
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

    pub fn resolve(
        &self,
        vars: &std::collections::HashMap<String, String>,
    ) -> Result<String, String> {
        match self {
            Expr::BacktickLit { parts, .. } => {
                let mut result = String::new();
                for part in parts {
                    if part.is_var {
                        match vars.get(&part.value) {
                            Some(value) => result.push_str(value),
                            None => return Err(format!("undefined variable: ${}", part.value)),
                        }
                    } else {
                        result.push_str(&part.value);
                    }
                }
                Ok(result)
            }
            Expr::VarRef { name, .. } => match vars.get(name) {
                Some(value) => Ok(value.clone()),
                None => Err(format!("undefined variable: ${}", name)),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct TemplatePart {
    pub is_var: bool,
    pub value: String,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone)]
pub enum Stmt {
    ShellDecl {
        value: String,
        offset: usize,
        len: usize,
    },
    SanctuaryDecl {
        value: Expr,
    },
    ImportDecl {
        path: Expr,
    },
    VarDecl {
        var_type: VarType,
        name: String,
        value: Expr,
        offset: usize,
        len: usize,
    },
    ProjectDecl {
        name: String,
        fields: Vec<ProjectField>,
        body: Vec<Stmt>,
        offset: usize,
        len: usize,
    },
    FnDecl {
        name: String,
        body: Vec<FnStmt>,
        offset: usize,
        len: usize,
    },
    RunDecl {
        name: String,
        chains: Vec<Vec<String>>,
        offset: usize,
        len: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum VarType {
    String,
    Shell,
}

#[derive(Debug, Clone)]
pub struct ProjectField {
    pub key: String,
    pub value: Expr,
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
        arg: Expr,
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
    Literal { parts: Vec<TemplatePart> },
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

#[derive(Debug, Clone)]
pub struct Program {
    pub stmts: Vec<Stmt>,
    pub source_name: String,
    pub source_text: String,
}

impl Program {
    pub fn new() -> Self {
        Self {
            stmts: Vec::new(),
            source_name: String::new(),
            source_text: String::new(),
        }
    }

    pub fn set_source(&mut self, name: String, text: String) {
        self.source_name = name;
        self.source_text = text;
    }
}
