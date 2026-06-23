use std::collections::HashMap;

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

    pub fn resolve(&self, vars: &HashMap<String, String>) -> Result<String, String> {
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

#[derive(Debug, Clone, PartialEq)]
pub enum VarType {
    String,
    Shell,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone)]
pub enum FnStmt {
    Log { value: Expr },
    Exec { value: Expr },
    Cd { arg: Expr },
    VarDecl { var_type: VarType, name: String, value: Expr },
    EnvBlock { pairs: Vec<EnvPair>, body: Vec<FnStmt> },
    Case { condition: Expr, arms: Vec<CaseArm> },
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
