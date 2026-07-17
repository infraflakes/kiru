use super::*;
use crate::dsl::fnstmt::FnStmt;
use crate::dsl::lexer::Lexer;

pub(crate) fn parse_program(input: &str) -> Result<Program, Vec<ParseError>> {
    let lexer = Lexer::new(input.to_string());
    let mut parser = Parser::new(lexer);
    parser.parse()
}

pub(crate) fn count_fn_stmt_types(body: &[FnStmt]) -> Vec<&'static str> {
    body.iter()
        .map(|s| match s {
            FnStmt::Log(_) => "log",
            FnStmt::Exec(_) => "exec",
            FnStmt::Cd(_) => "cd",
            FnStmt::VarDecl(_) => "var",
            FnStmt::EnvBlock(_) => "env",
            FnStmt::Case(_) => "case",
        })
        .collect()
}

pub(crate) fn count_stmt_types(program: &Program) -> Vec<&'static str> {
    program
        .items
        .iter()
        .map(|s| match s {
            TopLevel::Stmt(Stmt::Var { .. }) => "var",
            TopLevel::Stmt(Stmt::Project { .. }) => "pr",
            TopLevel::Stmt(Stmt::Field { .. }) => "field",
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
            Stmt::Run { .. } => "run",
            Stmt::Project { .. } | Stmt::Field { .. } => "other",
        })
        .collect()
}
