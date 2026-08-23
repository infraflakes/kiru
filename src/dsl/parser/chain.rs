use super::*;

impl Parser {
    pub(crate) fn parse_chain(&mut self) -> Result<Vec<QualifiedFnRef>, ParseError> {
        let mut fn_names = Vec::new();
        fn_names.push(self.parse_fn_name_in_run()?);

        while self.current_token().ty == TokenType::Arrow {
            self.advance();
            fn_names.push(self.parse_fn_name_in_run()?);
        }

        self.expect_with_context(TokenType::Semicolon, "after run chain")?;
        Ok(fn_names)
    }

    fn parse_fn_name_in_run(&mut self) -> Result<QualifiedFnRef, ParseError> {
        let start_offset = self.current_token().offset;
        let (project, function, end_offset) = self.parse_qualified_ref(
            "expected project namespace in run block",
            "expected function name after `::` in run block",
            "run block reference must be namespaced as `namespace::function`",
        )?;
        Ok(QualifiedFnRef {
            project,
            function,
            offset: start_offset,
            len: end_offset - start_offset,
            source_name: self.source_name.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::dsl::parser::test_support::*;
    use crate::dsl::{Stmt, TopLevel};

    #[test]
    fn test_run_single_ref() {
        let input = "run b { p::build; }";
        let prog = parse_program(input).unwrap();
        match &prog.items[0] {
            TopLevel::Stmt(Stmt::Run { chains, .. }) => {
                assert_eq!(chains.len(), 1);
                let refs = &chains[0];
                assert_eq!(refs.len(), 1);
                assert_eq!(refs[0].project, "p");
                assert_eq!(refs[0].function, "build");
            }
            _ => panic!("expected RunDecl"),
        }
    }

    #[test]
    fn test_run_chained_refs() {
        let input = "run d { p::build => p::deploy => p::notify; }";
        let prog = parse_program(input).unwrap();
        match &prog.items[0] {
            TopLevel::Stmt(Stmt::Run { chains, .. }) => {
                assert_eq!(chains.len(), 1);
                let names: Vec<(&str, &str)> = chains[0]
                    .iter()
                    .map(|q| (q.project.as_str(), q.function.as_str()))
                    .collect();
                assert_eq!(
                    names,
                    vec![("p", "build"), ("p", "deploy"), ("p", "notify")]
                );
            }
            _ => panic!("expected RunDecl"),
        }
    }

    #[test]
    fn test_run_multiple_chains() {
        let input = "run all { p::build; p::test; p::deploy; }";
        let prog = parse_program(input).unwrap();
        match &prog.items[0] {
            TopLevel::Stmt(Stmt::Run { name, chains, .. }) => {
                assert_eq!(name, "all");
                assert_eq!(chains.len(), 3);
                for (i, expected_fn) in ["build", "test", "deploy"].iter().enumerate() {
                    let names: Vec<(&str, &str)> = chains[i]
                        .iter()
                        .map(|q| (q.project.as_str(), q.function.as_str()))
                        .collect();
                    assert_eq!(names, vec![("p", *expected_fn)], "chain {i}");
                }
            }
            _ => panic!("expected RunDecl"),
        }
    }

    #[test]
    fn test_run_chain_in_project() {
        let input = "pr p [url = `u`] { run local { p::build; } use build; }";
        let result = parse_program(input);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        let err_msg = errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            err_msg.contains("expected `var` or `use` in project body"),
            "got: {}",
            err_msg
        );
    }

    #[test]
    fn test_run_with_chain_and_recovery() {
        let result = parse_program("run r { p::a -> ; }");
        assert!(result.is_err());
    }

    #[test]
    fn test_run_body_must_end_with_semicolon() {
        let result = parse_program("run r { p::a }");
        assert!(result.is_err());
    }
}
