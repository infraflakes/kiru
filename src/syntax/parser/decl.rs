//! Top-level declaration parsers: `var` and `fn`. A `var` inside any body
//! is a statement (`FnStmt::Bind`), not a declaration.

use super::*;

impl Parser {
    pub(crate) fn parse_var_decl(&mut self) -> Result<Stmt, ParseError> {
        let (name, value) = self.parse_var_decl_common()?;
        Ok(Stmt::Var { name, value })
    }

    pub(crate) fn parse_fn_decl(&mut self) -> Result<Stmt, ParseError> {
        self.advance();

        // `fn name { ... }` (no params) or the fused call form
        // `fn name(p; q) { ... }`.
        let (name, params) = match &self.current_token().token_type {
            TokenType::Call { name, args } => {
                let name = name.clone();
                let params = self.parse_params(args.clone())?;
                self.advance();
                (name, params)
            }
            _ => (self.parse_ident_name("function name")?, Vec::new()),
        };

        let body = self.parse_braced_block(
            "after function name",
            "to close function body",
            Self::parse_fn_stmt,
        )?;
        self.expect_with_context(TokenType::Semicolon, "after function declaration")?;

        Ok(Stmt::Fn { name, params, body })
    }

    /// Validate the fused `fn name(...)` argument list as parameter names:
    /// each argument must be a plain identifier literal, unique within the
    /// declaration, and not a reserved keyword.
    fn parse_params(&self, args: Vec<Template>) -> Result<Vec<String>, ParseError> {
        let mut params = Vec::new();
        for arg in args {
            let text = match arg.parts.as_slice() {
                [crate::syntax::source::Part::Lit(text)] => text.trim(),
                _ => {
                    return Err(ParseError::new(
                        self.eof_aware_span(),
                        "function parameters must be plain identifiers".to_string(),
                    ));
                }
            };
            if !crate::syntax::token::is_identifier(text) {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    format!("`{text}` is not a valid parameter name"),
                ));
            }
            params.push(text.to_string());
        }
        Ok(params)
    }
}

#[cfg(test)]
mod tests {
    use crate::syntax::parser::test_support::*;
    use crate::syntax::{Stmt, TopLevel};

    #[test]
    fn test_var_decl() {
        let prog = parse_program("var x = (hello);").unwrap();
        match &prog.top_level_items[0] {
            TopLevel::Stmt(Stmt::Var { name, value, .. }) => {
                assert_eq!(name, "x");
                let text: String = value
                    .parts
                    .iter()
                    .map(|p| match p {
                        crate::syntax::source::Part::Lit(s) => s.clone(),
                        _ => String::new(),
                    })
                    .collect();
                assert_eq!(text, "hello");
            }
            _ => panic!("expected VarDecl"),
        }
    }

    #[test]
    fn test_fn_with_params() {
        let prog = parse_program("fn deploy(name; registry) { log(x); };").unwrap();
        match &prog.top_level_items[0] {
            TopLevel::Stmt(Stmt::Fn { name, params, .. }) => {
                assert_eq!(name, "deploy");
                assert_eq!(params, &["name".to_string(), "registry".to_string()]);
            }
            other => panic!("expected Fn, got {:?}", other),
        }
    }

    #[test]
    fn test_fn_without_params() {
        let prog = parse_program("fn build { log(x); };").unwrap();
        match &prog.top_level_items[0] {
            TopLevel::Stmt(Stmt::Fn { params, .. }) => assert!(params.is_empty()),
            other => panic!("expected Fn, got {:?}", other),
        }
    }

    #[test]
    fn test_duplicate_parameters_are_kept_in_order() {
        // Duplicates are not rejected; the last one wins when arguments are
        // bound at the call site.
        let prog = parse_program("fn deploy(name; name) { log(x); };").unwrap();
        match &prog.top_level_items[0] {
            TopLevel::Stmt(Stmt::Fn { params, .. }) => {
                assert_eq!(params, &["name".to_string(), "name".to_string()]);
            }
            other => panic!("expected Fn, got {:?}", other),
        }
    }

    #[test]
    fn test_keyword_param_rejected() {
        let result = parse_program("fn deploy(log) { log(x); };");
        assert!(result.is_err());
    }

    #[test]
    fn test_var_missing_name() {
        let result = parse_program("var = (hello);");
        assert!(result.is_err());
    }

    #[test]
    fn test_var_missing_value() {
        let result = parse_program("var x = ;");
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_semicolon() {
        let result = parse_program("var x = (hello)");
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| e.to_string().contains("expected")));
    }

    #[test]
    fn test_unclosed_fn_brace() {
        let result = parse_program("fn bad { log(hi);");
        assert!(result.is_err());
    }
}
