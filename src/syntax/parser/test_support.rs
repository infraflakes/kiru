use super::*;
use crate::syntax::fnstmt::FnStmt;
use crate::syntax::lexer::Lexer;

pub(crate) fn parse_program(input: &str) -> Result<Program, Vec<ParseError>> {
    let lexer = Lexer::new(input.to_string());
    let mut parser = Parser::new(lexer);
    parser.parse()
}

pub(crate) fn count_fn_stmt_types(body: &[FnStmt]) -> Vec<&'static str> {
    body.iter()
        .map(|s| match s {
            FnStmt::Log(_) => "log",
            FnStmt::Bind { target: None, .. } => "exec",
            FnStmt::Bind {
                target: Some(_), ..
            } => "var",
            FnStmt::Cd(_) => "cd",
            FnStmt::EnvBlock { .. } => "env",
            FnStmt::Switch { .. } => "switch",
        })
        .collect()
}

pub(crate) fn count_stmt_types(program: &Program) -> Vec<&'static str> {
    program
        .top_level_items
        .iter()
        .map(|s| match s {
            TopLevel::Stmt(Stmt::Var { .. }) => "var",
            TopLevel::Stmt(Stmt::Project { .. }) => "pr",
            TopLevel::Stmt(Stmt::Fn { .. }) => "fn",
            TopLevel::Stmt(Stmt::Run { .. }) => "run",
            TopLevel::Import(_) => "import",
        })
        .collect()
}

pub(crate) fn count_body_stmt_types(body: &[Stmt]) -> Vec<&'static str> {
    body.iter()
        .map(|s| match s {
            Stmt::Var { .. } => "var",
            Stmt::Fn { .. } => "fn",
            Stmt::Project { .. } => "other",
            _ => "other",
        })
        .collect()
}
