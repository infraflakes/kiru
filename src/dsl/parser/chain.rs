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
        let project = match &self.current_token().ty {
            TokenType::Ident(ident) => ident.clone(),
            _ => {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    format!(
                        "expected project namespace in run block, found {}",
                        format_token(self.current_token())
                    ),
                ));
            }
        };
        self.advance();
        if self.current_token().ty != TokenType::NamespaceSep {
            return Err(ParseError::new(
                self.eof_aware_span(),
                "run block reference must be namespaced as `namespace::function`".to_string(),
            ));
        }
        self.advance();
        let function = match &self.current_token().ty {
            TokenType::Ident(ident) => ident.clone(),
            _ => {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    format!(
                        "expected function name after `::` in run block, found {}",
                        format_token(self.current_token())
                    ),
                ));
            }
        };
        self.advance();
        Ok(QualifiedFnRef { project, function })
    }
}

#[cfg(test)]
mod tests {
    use crate::dsl::ast::QualifiedFnRef;
    use crate::dsl::parser::test_support::*;
    use crate::dsl::{Stmt, TopLevel};

    #[test]
    fn test_run_single_ref() {
        let input = "run b { p::build; }";
        let prog = parse_program(input).unwrap();
        match &prog.items[0] {
            TopLevel::Stmt(Stmt::Run { chains, .. }) => {
                assert_eq!(chains.len(), 1);
                assert_eq!(
                    chains[0],
                    vec![QualifiedFnRef {
                        project: "p".to_string(),
                        function: "build".to_string()
                    }]
                );
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
                assert_eq!(
                    chains[0],
                    vec![
                        QualifiedFnRef {
                            project: "p".to_string(),
                            function: "build".to_string()
                        },
                        QualifiedFnRef {
                            project: "p".to_string(),
                            function: "deploy".to_string()
                        },
                        QualifiedFnRef {
                            project: "p".to_string(),
                            function: "notify".to_string()
                        }
                    ]
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
                assert_eq!(
                    chains[0],
                    vec![QualifiedFnRef {
                        project: "p".to_string(),
                        function: "build".to_string()
                    }]
                );
                assert_eq!(
                    chains[1],
                    vec![QualifiedFnRef {
                        project: "p".to_string(),
                        function: "test".to_string()
                    }]
                );
                assert_eq!(
                    chains[2],
                    vec![QualifiedFnRef {
                        project: "p".to_string(),
                        function: "deploy".to_string()
                    }]
                );
            }
            _ => panic!("expected RunDecl"),
        }
    }

    #[test]
    fn test_run_chain_in_project() {
        let input = "pr p [url = `u`] { run local { p::build; } fn build { exec `make`; } }";
        let result = parse_program(input);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        let err_msg = errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            err_msg.contains("expected fn or var in project body"),
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
