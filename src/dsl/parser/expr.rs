use super::*;

impl Parser {
    pub(crate) fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        if let TokenType::Illegal(m) = &self.current_token().ty {
            let token = self.current_token().clone();
            return Err(ParseError::new(
                SourceSpan::new(token.offset.into(), token.len),
                m.clone(),
            ));
        }
        match &self.current_token().ty {
            TokenType::Backtick(_) => self.parse_backtick_expr(),
            TokenType::Dollar => {
                self.advance();

                let name = match &self.current_token().ty {
                    TokenType::Ident(n) => n.clone(),
                    ty if is_keyword_token(ty) => {
                        return Err(ParseError::new(
                            self.eof_aware_span(),
                            format!(
                                "expected identifier after `$`, found {} (reserved keyword)",
                                format_token(self.current_token())
                            ),
                        ));
                    }
                    _ => {
                        return Err(ParseError::new(
                            self.eof_aware_span(),
                            "expected identifier after `$`".to_string(),
                        ));
                    }
                };
                self.advance();

                Ok(Expr::VarRef { name })
            }
            _ => {
                let is_underscore = matches!(
                    &self.current_token().ty,
                    TokenType::Ident(s) if s == "_"
                );
                if is_underscore {
                    Err(ParseError::new(
                        self.eof_aware_span(),
                        "`_` is only valid as a case pattern".to_string(),
                    ))
                } else {
                    Err(ParseError::new(
                        self.eof_aware_span(),
                        format!(
                            "unexpected token in expression: {:?}",
                            self.current_token().ty
                        ),
                    ))
                }
            }
        }
    }

    pub(crate) fn parse_backtick_expr(&mut self) -> Result<Expr, ParseError> {
        let token = self.current_token().clone();
        let TokenType::Backtick(content) = &token.ty else {
            unreachable!("parse_backtick_expr called without Backtick token")
        };
        self.advance();
        let parts = parse_template_parts(content, token.offset)?;
        Ok(Expr::BacktickLit { parts })
    }

    pub(crate) fn parse_simple_backtick(&mut self) -> Result<String, ParseError> {
        let expr = self.parse_backtick_expr()?;
        if let Expr::BacktickLit { parts } = &expr {
            let concat: String = parts.iter().map(|p| p.value.as_str()).collect();
            Ok(concat)
        } else {
            Ok(String::new())
        }
    }
}

pub(crate) fn parse_template_parts(
    content: &str,
    offset: usize,
) -> Result<Vec<TemplatePart>, ParseError> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = content.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        if c == '$' && chars.peek().is_some_and(|(_, n)| n == &'{') {
            chars.next();

            if !current.is_empty() {
                parts.push(TemplatePart {
                    is_var: false,
                    value: current.clone(),
                });
                current.clear();
            }

            let mut var_name = String::new();
            loop {
                match chars.next() {
                    Some((_, '}')) => break,
                    Some((_, c)) => var_name.push(c),
                    None => {
                        return Err(ParseError::new(
                            SourceSpan::new((offset + i).into(), 3),
                            "unclosed variable interpolation".to_string(),
                        ));
                    }
                }
            }

            if var_name.is_empty() {
                return Err(ParseError::new(
                    SourceSpan::new((offset + i).into(), 3),
                    "empty variable name in template".to_string(),
                ));
            }

            parts.push(TemplatePart {
                is_var: true,
                value: var_name,
            });
        } else {
            current.push(c);
        }
    }

    if !current.is_empty() {
        parts.push(TemplatePart {
            is_var: false,
            value: current,
        });
    }

    Ok(parts)
}
