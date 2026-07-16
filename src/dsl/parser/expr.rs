use super::*;

impl Parser {
    pub(crate) fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        if let TokenType::Illegal(msg) = &self.current_token().ty {
            let token = self.current_token().clone();
            return Err(ParseError::new(
                SourceSpan::new(token.offset.into(), token.len),
                msg.clone(),
            ));
        }
        match &self.current_token().ty {
            TokenType::Backtick(_) => self.parse_backtick_expr(),
            TokenType::Dollar => {
                let start_offset = self.current_token().offset;
                self.advance();

                let name = match &self.current_token().ty {
                    TokenType::Ident(name_str) => name_str.clone(),
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
                let name_end = self.current_token().offset + self.current_token().len;
                self.advance();

                Ok(Expr::VarRef {
                    name,
                    offset: start_offset,
                    len: name_end - start_offset,
                })
            }
            _ => {
                let is_underscore = matches!(
                    &self.current_token().ty,
                    TokenType::Ident(ident) if ident == "_"
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
                            "expected backtick string or variable reference, found {}",
                            format_token(self.current_token())
                        ),
                    ))
                }
            }
        }
    }

    pub(crate) fn parse_backtick_expr(&mut self) -> Result<Expr, ParseError> {
        let token = self.current_token().clone();
        let TokenType::Backtick(content) = &token.ty else {
            return Err(ParseError::new(
                self.eof_aware_span(),
                format!("expected backtick string, found {}", format_token(&token)),
            ));
        };
        let offset = token.offset;
        let len = token.len;
        self.advance();
        let parts = parse_interpolation_parts(content, token.offset)?;
        Ok(Expr::BacktickLit { parts, offset, len })
    }
}

pub(crate) fn parse_interpolation_parts(
    content: &str,
    offset: usize,
) -> Result<Vec<InterpolationPart>, ParseError> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = content.char_indices().peekable();

    while let Some((idx, ch)) = chars.next() {
        if ch == '$' && chars.peek().is_some_and(|(_, next_char)| next_char == &'{') {
            chars.next();

            if !current.is_empty() {
                parts.push(InterpolationPart {
                    is_var: false,
                    value: current.clone(),
                });
                current.clear();
            }

            let mut var_name = String::new();
            loop {
                match chars.next() {
                    Some((_, '}')) => break,
                    Some((_, ch)) => var_name.push(ch),
                    None => {
                        return Err(ParseError::new(
                            SourceSpan::new((offset + 1 + idx).into(), 3),
                            "unclosed variable interpolation".to_string(),
                        ));
                    }
                }
            }

            if var_name.is_empty() {
                return Err(ParseError::new(
                    SourceSpan::new((offset + 1 + idx).into(), 3),
                    "empty variable name in template".to_string(),
                ));
            }

            parts.push(InterpolationPart {
                is_var: true,
                value: var_name,
            });
        } else {
            current.push(ch);
        }
    }

    if !current.is_empty() {
        parts.push(InterpolationPart {
            is_var: false,
            value: current,
        });
    }

    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::parse_interpolation_parts;

    #[test]
    fn test_basic_template_part() {
        let parts = parse_interpolation_parts("hello", 0).unwrap();
        assert_eq!(parts.len(), 1);
        assert!(!parts[0].is_var);
        assert_eq!(parts[0].value, "hello");
    }

    #[test]
    fn test_template_with_var() {
        let parts = parse_interpolation_parts("hello ${name} world", 0).unwrap();
        assert_eq!(parts.len(), 3);
        assert!(!parts[0].is_var);
        assert_eq!(parts[0].value, "hello ");
        assert!(parts[1].is_var);
        assert_eq!(parts[1].value, "name");
        assert!(!parts[2].is_var);
        assert_eq!(parts[2].value, " world");
    }

    #[test]
    fn test_template_empty_var_name() {
        let result = parse_interpolation_parts("hello ${}", 0);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("empty variable name")
        );
    }
}
