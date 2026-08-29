use super::*;
use crate::dsl::token::format_token_type;

impl Parser {
    /// Parses `run name { pr::fn => pr::fn; pr::fn; }`.
    ///
    /// References are `project::function` calls. A `=>` appends the call to the
    /// current chain so it runs sequentially after the previous call in that
    /// chain; a `;` closes the current chain and opens a new one that runs
    /// concurrently with the others. A trailing `;` before `}` is optional.
    pub(crate) fn parse_run_decl(&mut self) -> Result<Stmt, ParseError> {
        let start_offset = self.current_token().offset;
        self.advance(); // skip `run`

        let name = self.parse_ident_name("run block name")?;
        self.expect_with_context(TokenType::LBrace, "after run block name")?;

        let mut chains: Vec<Vec<Call>> = Vec::new();
        let mut current_chain: Vec<Call> = Vec::new();
        while self.current_token().token_type != TokenType::RBrace {
            let call = self.parse_run_call()?;
            current_chain.push(call);
            match self.current_token().token_type.clone() {
                TokenType::ChainArrow => self.advance(),
                TokenType::Semicolon => {
                    self.advance();
                    chains.push(std::mem::take(&mut current_chain));
                }
                TokenType::RBrace => break,
                other => {
                    return Err(ParseError::new(
                        self.eof_aware_span(),
                        format!(
                            "expected `;`, `=>`, or `}}` after run reference, found {}",
                            format_token_type(&other)
                        ),
                    ));
                }
            }
        }
        // A dangling final chain (no trailing `;`) is still part of the block.
        if !current_chain.is_empty() {
            chains.push(current_chain);
        }

        let end_offset = self.current_token().offset + self.current_token().len;
        self.expect_with_context(TokenType::RBrace, "to close run block")?;
        // A trailing `;` after the block is optional, matching the other
        // top-level declarations (`sync`/`pr`/`var`/`fn`/`shell`).
        if self.current_token().token_type == TokenType::Semicolon {
            self.advance();
        }

        Ok(Stmt::Run {
            name,
            calls: chains,
            offset: start_offset,
            len: end_offset - start_offset,
        })
    }

    /// Parses a single `project::function` reference inside a run block. The
    /// separating `;`/`=>` is handled by the caller so stages can be grouped.
    fn parse_run_call(&mut self) -> Result<Call, ParseError> {
        let (project, function, _end) = self.parse_qualified_ref(
            "expected project namespace in run reference",
            "expected function name after `::` in run reference",
        )?;
        Ok(Call { project, function })
    }
}

#[cfg(test)]
mod tests {
    use crate::dsl::parser::test_support::*;
    use crate::dsl::{Stmt, TopLevel};
    use crate::plan::Call;

    #[test]
    fn test_run_single_ref() {
        let input = "run b { p::build; }";
        let prog = parse_program(input).unwrap();
        match &prog.top_level_items[0] {
            TopLevel::Stmt(Stmt::Run { calls, .. }) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].len(), 1);
                assert_eq!(
                    calls[0][0],
                    Call {
                        project: "p".into(),
                        function: "build".into()
                    }
                );
            }
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn test_run_multiple_semicolon_is_separate_chains() {
        let input = "run d { p::build; p::deploy; p::notify; }";
        let prog = parse_program(input).unwrap();
        match &prog.top_level_items[0] {
            TopLevel::Stmt(Stmt::Run { calls, .. }) => {
                assert_eq!(calls.len(), 3, "each `;` call is its own concurrent chain");
                assert_eq!(calls[0][0].project, "p");
                assert_eq!(calls[0][0].function, "build");
                assert_eq!(calls[2][0].function, "notify");
            }
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn test_run_arrow_is_sequential_chain() {
        let input = "run d { p::a => p::b => p::c; }";
        let prog = parse_program(input).unwrap();
        match &prog.top_level_items[0] {
            TopLevel::Stmt(Stmt::Run { calls, .. }) => {
                assert_eq!(calls.len(), 1, "`=>` joins one sequential chain");
                assert_eq!(calls[0].len(), 3);
                assert_eq!(calls[0][0].function, "a");
                assert_eq!(calls[0][1].function, "b");
                assert_eq!(calls[0][2].function, "c");
            }
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn test_run_mixed_chains() {
        let input = "run d { p::a; p::b => p::c; p::d }";
        let prog = parse_program(input).unwrap();
        match &prog.top_level_items[0] {
            TopLevel::Stmt(Stmt::Run { calls, .. }) => {
                assert_eq!(calls.len(), 3, "two `;` boundaries make three chains");
                assert_eq!(calls[0].len(), 1);
                assert_eq!(calls[1].len(), 2);
                assert_eq!(calls[1][0].function, "b");
                assert_eq!(calls[1][1].function, "c");
                assert_eq!(calls[2][0].function, "d");
            }
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn test_run_requires_separator_between_calls() {
        let result = parse_program("run r { p::a p::b }");
        assert!(result.is_err());
    }

    #[test]
    fn test_run_requires_namespace() {
        let result = parse_program("run r { build; }");
        assert!(result.is_err());
    }
}
