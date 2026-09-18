//! Template expression parser: `(literal)`, `$(cmd)`, `@(var)`, and
//! bare identifiers resolved into `Template` nodes.

use super::*;
use crate::syntax::source::Template;

impl Parser {
    /// Parse a template expression: a `( ... )` / `$( ... )` / `@( ... )`
    /// template token. Variables are always referenced as `@(name)`; there
    /// is no bare-identifier shorthand.
    pub(crate) fn parse_expr(&mut self) -> Result<Template, ParseError> {
        match &self.current_token().token_type {
            TokenType::Template(t) => {
                let t = t.clone();
                self.advance();
                Ok(t)
            }
            _ => Err(ParseError::new(
                self.eof_aware_span(),
                format!(
                    "expected a template, found {}",
                    format_token(self.current_token())
                ),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::syntax::parser::test_support::parse_program;
    use crate::syntax::source::Part;

    #[test]
    fn test_parse_literal_template() {
        let prog = parse_program("var x = (hello);").unwrap();
        match &prog.top_level_items[0] {
            crate::syntax::TopLevel::Stmt(crate::syntax::Stmt::Var { value, .. }) => {
                assert_eq!(value.parts.len(), 1);
                assert!(matches!(&value.parts[0], Part::Lit(s) if s == "hello"));
            }
            _ => panic!("expected var"),
        }
    }

    #[test]
    fn test_parse_var_ref_template() {
        let prog = parse_program("var x = @(name);").unwrap();
        match &prog.top_level_items[0] {
            crate::syntax::TopLevel::Stmt(crate::syntax::Stmt::Var { value, .. }) => {
                assert!(matches!(&value.parts[0], Part::Var(n) if n == "name"));
            }
            _ => panic!("expected var"),
        }
    }
}
