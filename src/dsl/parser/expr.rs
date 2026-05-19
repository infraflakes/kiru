use super::*;

impl Parser {
    pub(crate) fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        match &self.current_token().ty {
            TokenType::Backtick(_) => self.parse_backtick_expr(),
            TokenType::Dollar => {
                self.advance();

                let name = match &self.current_token().ty {
                    TokenType::Ident(n) => n.clone(),
                    _ => {
                        return Err(ParseError::new(
                            miette::SourceSpan::new(
                                self.current_token().offset.into(),
                                self.current_token().len,
                            ),
                            "expected identifier after `$`".to_string(),
                        ));
                    }
                };
                self.advance();

                Ok(Expr::VarRef { name })
            }
            _ => Err(ParseError::new(
                miette::SourceSpan::new(
                    self.current_token().offset.into(),
                    self.current_token().len,
                ),
                format!(
                    "unexpected token in expression: {:?}",
                    self.current_token().ty
                ),
            )),
        }
    }

    pub(crate) fn parse_backtick_expr(&mut self) -> Result<Expr, ParseError> {
        let token = self.current_token().clone();

        match &token.ty {
            TokenType::Backtick(content) => {
                self.advance();

                let parts = parse_template_parts(content, token.offset)?;

                Ok(Expr::BacktickLit { parts })
            }
            _ => unreachable!(),
        }
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

#[cfg(test)]
mod expr_tests {
    use super::*;

    #[test]
    fn test_basic_template_part() {
        let parts = parse_template_parts("hello", 0).unwrap();
        assert_eq!(parts.len(), 1);
        assert!(!parts[0].is_var);
        assert_eq!(parts[0].value, "hello");
    }

    #[test]
    fn test_template_with_var() {
        let parts = parse_template_parts("hello ${name} world", 0).unwrap();
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
        let result = parse_template_parts("hello ${}", 0);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("empty variable name")
        );
    }
}
