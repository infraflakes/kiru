use super::*;
use crate::syntax::source::{Part, Template};

impl Parser {
    /// Parse a template expression. This is either a `( ... )` / `$( ... )` /
    /// `@( ... )` template token, or a bare identifier treated as a `@(name)`
    /// variable reference (used for `switch` conditions).
    pub(crate) fn parse_expr(&mut self) -> Result<Template, ParseError> {
        self.err_on_illegal_token()?;
        match &self.current_token().token_type {
            TokenType::Template(t) => {
                let t = t.clone();
                self.advance();
                Ok(t)
            }
            TokenType::Ident(name) => {
                let name = name.clone();
                self.advance();
                Ok(Template {
                    parts: vec![Part::Var(name)],
                    offset: self.current_token().offset,
                    len: 0,
                    source_name: self.source_name.clone(),
                })
            }
            _ => Err(self.unexpected_stmt_start_error("template or variable reference")),
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
