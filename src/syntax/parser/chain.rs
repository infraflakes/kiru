//! Run-block parser: `run name { ... };` - entry points whose bodies are
//! ordinary statement lists, exactly like function bodies. Concurrency is
//! expressed with the `async() { ... };` primitive, not by separators.

use super::*;

impl Parser {
    /// Parses `run name { statement; ... };`.
    ///
    /// A run body is a normal body: primitives, calls, and `async` blocks,
    /// executed top-down. `;` always means "then"; `async` starts its body
    /// concurrently and joins at the end of the enclosing body.
    pub(crate) fn parse_run_decl(&mut self) -> Result<Stmt, ParseError> {
        let start_offset = self.current_token().offset;
        self.advance(); // skip `run`

        let name = self.parse_ident_name("run block name")?;

        let body = self.parse_braced_block(
            "after run block name",
            "to close run body",
            Self::parse_fn_stmt,
        )?;

        let end_offset = self.current_token().offset + self.current_token().len;
        self.expect_with_context(TokenType::Semicolon, "after run declaration")?;

        Ok(Stmt::Run {
            name,
            body,
            offset: start_offset,
            len: end_offset - start_offset,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::syntax::fnstmt::FnStmt;
    use crate::syntax::parser::test_support::*;
    use crate::syntax::{Stmt, TopLevel};

    fn run_body(input: &str) -> Vec<FnStmt> {
        let prog = parse_program(input).unwrap();
        match &prog.top_level_items[0] {
            TopLevel::Stmt(Stmt::Run { body, .. }) => body.clone(),
            other => panic!("expected Run, got {:?}", other),
        }
    }

    #[test]
    fn test_run_body_is_sequential_statements() {
        let body = run_body("run d { a(); b(); };");
        assert_eq!(count_fn_stmt_types(&body), vec!["call", "call"]);
    }

    #[test]
    fn test_run_call_arguments() {
        let body = run_body("run d { deploy(app; $(git rev-parse HEAD)); };");
        match &body[0] {
            FnStmt::Call { name, args, .. } => {
                assert_eq!(name, "deploy");
                assert_eq!(args.len(), 2, "arguments are carried with the call");
            }
            other => panic!("expected Call, got {:?}", other),
        }
    }

    #[test]
    fn test_project_block_is_a_statement() {
        let body = run_body("run d { project(p) { build(); }; };");
        match &body[0] {
            FnStmt::Project { name, body } => {
                assert_eq!(name.literal_text(), "p");
                assert_eq!(count_fn_stmt_types(body), vec!["call"]);
            }
            other => panic!("expected Project, got {:?}", other),
        }
    }

    #[test]
    fn test_project_requires_a_name() {
        let result = parse_program("run d { project() { a(); }; };");
        let errs = result.unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.to_string().contains("`project` requires a project name")),
            "got: {:?}",
            errs
        );
    }

    #[test]
    fn test_async_block_is_a_statement() {
        let body = run_body("run d { async() { a(); b(); }; };");
        match &body[0] {
            FnStmt::Async { body } => {
                assert_eq!(count_fn_stmt_types(body), vec!["call", "call"]);
            }
            other => panic!("expected Async, got {:?}", other),
        }
    }

    #[test]
    fn test_async_rejects_arguments() {
        let result = parse_program("run d { async(x) { a(); }; };");
        assert!(result.is_err());
    }

    #[test]
    fn test_run_decl_requires_semicolon() {
        let result = parse_program("run r { a(); }");
        assert!(result.is_err());
    }
}
