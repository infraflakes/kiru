use super::*;

impl Parser {
    pub(crate) fn parse_block_fn_name(&mut self) -> Result<String, ParseError> {
        let fn_name = match &self.current_token().ty {
            TokenType::Ident(n) => n.clone(),
            _ => {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    format!(
                        "expected function name in seq/par, found {}",
                        format_token(self.current_token())
                    ),
                ));
            }
        };
        self.advance();
        self.expect_with_context(TokenType::Semicolon, "after function name")?;
        Ok(fn_name)
    }
}
