use super::*;

impl Parser {
    pub(crate) fn parse_chain(&mut self) -> Result<Vec<String>, ParseError> {
        let mut fns = Vec::new();
        fns.push(self.parse_fn_name_in_run()?);

        while self.current_token().ty == TokenType::Arrow {
            self.advance();
            fns.push(self.parse_fn_name_in_run()?);
        }

        self.expect_with_context(TokenType::Semicolon, "after run chain")?;
        Ok(fns)
    }

    fn parse_fn_name_in_run(&mut self) -> Result<String, ParseError> {
        match &self.current_token().ty {
            TokenType::Ident(n) => {
                let name = n.clone();
                self.advance();
                Ok(name)
            }
            _ => Err(ParseError::new(
                self.eof_aware_span(),
                format!(
                    "expected function name in run block, found {}",
                    format_token(self.current_token())
                ),
            )),
        }
    }
}
