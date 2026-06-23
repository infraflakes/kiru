use crate::ir::{Expr, FnStmt, VarType};

#[derive(Debug, Clone)]
pub enum Stmt {
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

#[derive(Debug, Clone)]
pub struct ProjectField {
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
