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
            FnStmt::Exec(_) => "exec",
            FnStmt::Bind { .. } => "var",
            FnStmt::Cd(_) => "cd",
            FnStmt::EnvBlock { .. } => "env",
            FnStmt::Switch { .. } => "switch",
            FnStmt::Call { .. } => "call",
            FnStmt::Async { .. } => "async",
            FnStmt::Project { .. } => "project",
        })
        .collect()
}

pub(crate) fn count_stmt_types(program: &Program) -> Vec<&'static str> {
    program
        .top_level_items
        .iter()
        .map(|s| match s {
            TopLevel::Stmt(Stmt::Var { .. }) => "var",
            TopLevel::Stmt(Stmt::Fn { .. }) => "fn",
            TopLevel::Stmt(Stmt::Run { .. }) => "run",
            TopLevel::Import(_) => "import",
        })
        .collect()
}
