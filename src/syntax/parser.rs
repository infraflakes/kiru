use crate::diagnostics::Span;
use crate::syntax::ast::Call;
#[cfg(test)]
use crate::syntax::ast::Program;
use crate::syntax::error::ParseError;
use crate::syntax::lexer::Lexer;
use crate::syntax::token::{Token, TokenType, format_token, format_token_type, is_keyword_token};
use crate::syntax::{FnStmt, Stmt, Template, TopLevel};

mod body;
mod chain;
mod decl;
mod expr;
mod project;

#[cfg(test)]
mod test_support;

/// Recursive-descent parser for the kiru DSL. Wraps a `Lexer` and produces
/// a sequence of `TopLevel` items (statements and imports).
pub(crate) struct Parser {
    lexer: Lexer,
    current: Token,
    /// One token of lookahead, used to disambiguate `pr::fn` references in run
    /// blocks from a bare identifier in a function body.
    next: Token,
    source_len: usize,
    /// First lex error hit while filling the token windows. Lex errors are
    /// deferred to the next `parse_toplevel` call so the statement currently
    /// being parsed finishes normally before compilation aborts.
    pending_lex_error: Option<ParseError>,
}

impl Parser {
    /// Constructs a new Parser from the given Lexer, advancing to the first token.
    pub(crate) fn new(lexer: Lexer) -> Self {
        let source_len = lexer.source_len();
        let mut parser = Parser {
            lexer,
            current: Token::new(TokenType::Eof, source_len, 0),
            next: Token::new(TokenType::Eof, source_len, 0),
            source_len,
            pending_lex_error: None,
        };
        // Fill both lookahead slots (current + next) from the lexer.
        parser.fill_token_window();
        parser.fill_token_window();
        parser
    }

    /// Pulls one token from the lexer into `self.next`. A lex error is
    /// stashed (first one wins) and an EOF token takes its place, so the
    /// token window always holds usable tokens.
    fn pull_token(&mut self) {
        let token = match self.lexer.next_token() {
            Ok(token) => token,
            Err(e) => {
                if self.pending_lex_error.is_none() {
                    self.pending_lex_error = Some(e);
                }
                Token::new(TokenType::Eof, self.source_len, 0)
            }
        };
        self.next = token;
    }

    /// Refills the lookahead window: `current` becomes the old `next` and a
    /// fresh token is pulled from the lexer.
    fn fill_token_window(&mut self) {
        self.current = std::mem::replace(&mut self.next, Token::new(TokenType::Eof, 0, 0));
        self.pull_token();
    }

    /// Returns a reference to the current token.
    fn current_token(&self) -> &Token {
        &self.current
    }

    /// Advances to the next token from the lexer.
    fn advance(&mut self) {
        self.fill_token_window();
    }

    /// Surfaces a deferred lex error. Every site that reports "end of file"
    /// must call this first: an EOF reached while a lex error is pending is
    /// the synthetic EOF substituted for the unreadable token, and the real
    /// error is the pending one.
    fn take_pending_lex_error(&mut self) -> Result<(), ParseError> {
        match self.pending_lex_error.take() {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    /// Returns a Span that safely handles EOF by pointing at the last byte.
    fn eof_aware_span(&self) -> Span {
        let tok = &self.current;
        if tok.len == 0 && tok.offset >= self.source_len && self.source_len > 0 {
            let start = self.source_len.saturating_sub(1);
            return Span::new(start, 1);
        }
        let len = if tok.len == 0 {
            1.min(self.source_len.saturating_sub(tok.offset))
        } else {
            tok.len
        };
        Span::new(tok.offset, len)
    }

    /// Expects a specific token type and advances past it, returning an error with context on mismatch.
    fn expect_with_context(&mut self, ty: TokenType, context: &str) -> Result<(), ParseError> {
        if self.current_token().token_type == ty {
            self.advance();
            Ok(())
        } else {
            let expected = format_token_type(&ty);
            let found = format_token(self.current_token());
            Err(ParseError::new(
                self.eof_aware_span(),
                format!("expected {} {}, found {}", expected, context, found),
            ))
        }
    }

    /// Reads an identifier as a named declaration target (variable, function,
    /// project, run, or field name) and advances past it. Reserved keywords
    /// and non-identifier tokens are rejected with a context-aware message.
    fn parse_ident_name(&mut self, expected: &'static str) -> Result<String, ParseError> {
        let name = match &self.current_token().token_type {
            TokenType::Ident(name_str) => name_str.clone(),
            ty if is_keyword_token(ty) => {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    format!(
                        "expected {}, found {} (reserved keyword)",
                        expected,
                        format_token(self.current_token())
                    ),
                ));
            }
            _ => {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    format!(
                        "expected {}, found {}",
                        expected,
                        format_token(self.current_token())
                    ),
                ));
            }
        };
        self.advance();
        Ok(name)
    }

    /// Reads one identifier part of a qualified `project::function` reference,
    /// rejecting reserved keywords, and advances past it.
    fn parse_ident_part(&mut self, expected: &'static str) -> Result<String, ParseError> {
        match &self.current_token().token_type {
            TokenType::Ident(part) => {
                let part = part.clone();
                self.advance();
                Ok(part)
            }
            ty if is_keyword_token(ty) => Err(ParseError::new(
                self.eof_aware_span(),
                format!(
                    "{}, found {} (reserved keyword)",
                    expected,
                    format_token(self.current_token())
                ),
            )),
            _ => Err(ParseError::new(self.eof_aware_span(), expected.to_string())),
        }
    }

    /// Parses a `project::function` reference at the current token. Both parts
    /// must be plain identifiers. Returns the project, the function, and the
    /// function's end offset (for span construction).
    fn parse_qualified_ref(
        &mut self,
        expected_project: &'static str,
        expected_function: &'static str,
    ) -> Result<(String, String, usize), ParseError> {
        let project = self.parse_ident_part(expected_project)?;
        if self.current_token().token_type != TokenType::NamespaceSep {
            return Err(ParseError::new(
                self.eof_aware_span(),
                "run reference must be `project::function`".to_string(),
            ));
        }
        self.advance();
        let function = self.parse_ident_part(expected_function)?;
        let function_end = self.current_token().offset + self.current_token().len;
        Ok((project, function, function_end))
    }

    /// Builds the error for an unexpected statement-start token.
    fn unexpected_stmt_start_error(&self, expected: &'static str) -> ParseError {
        if matches!(&self.current_token().token_type, TokenType::Ident(i) if i == "_") {
            return ParseError::new(
                self.eof_aware_span(),
                "`_` is only valid as a case pattern".to_string(),
            );
        }
        ParseError::new(
            self.eof_aware_span(),
            format!(
                "expected {}, found {}",
                expected,
                format_token(self.current_token())
            ),
        )
    }

    /// Parses one top-level item, returning None on EOF. A deferred lex
    /// error surfaces here before anything else parses.
    pub(crate) fn parse_toplevel(&mut self) -> Result<Option<TopLevel>, ParseError> {
        self.take_pending_lex_error()?;
        if self.current_token().token_type == TokenType::Eof {
            return Ok(None);
        }
        match &self.current_token().token_type {
            TokenType::Import => {
                self.advance();
                let path = self.parse_expr()?;
                self.expect_with_context(TokenType::Semicolon, "after import path")?;
                Ok(Some(TopLevel::Import(path)))
            }
            _ => self
                .parse_top_level_stmt()
                .map(|stmt| Some(TopLevel::Stmt(stmt))),
        }
    }

    #[cfg(test)]
    pub(crate) fn parse(&mut self) -> Result<Program, Vec<ParseError>> {
        let mut program = Program::new_with_source(String::new(), String::new());
        let mut errors = Vec::new();

        while self.current_token().token_type != TokenType::Eof {
            match self.parse_toplevel() {
                Ok(Some(item)) => program.top_level_items.push(item),
                Ok(None) => break,
                Err(e) => {
                    errors.push(e);
                    self.skip_to_stmt_boundary();
                }
            }
        }

        if errors.is_empty() {
            Ok(program)
        } else {
            Err(errors)
        }
    }

    /// Dispatches to the correct parser based on the current token for top-level statements.
    fn parse_top_level_stmt(&mut self) -> Result<Stmt, ParseError> {
        match self.current_token().token_type {
            TokenType::Var => self.parse_var_decl(),
            TokenType::Pr => self.parse_project_decl(),
            TokenType::Fn => {
                // Consume `fn` before erroring so error recovery always makes
                // progress past this token.
                let fn_span = self.eof_aware_span();
                self.advance();
                Err(ParseError::new(
                    fn_span,
                    "functions must be declared inside a `pr` block".to_string(),
                ))
            }
            TokenType::Run => self.parse_run_decl(),
            _ => Err(self.unexpected_stmt_start_error("var, pr, or run")),
        }
    }

    #[cfg(test)]
    fn skip_to_stmt_boundary(&mut self) {
        use TokenType::*;
        loop {
            match &self.current_token().token_type {
                Eof => break,
                Semicolon | RBrace => {
                    self.advance();
                }
                Var | Pr | Fn | Run => break,
                _ => self.advance(),
            }
        }
    }

    /// Parses `var name = expr;` (no type annotation). Returns the name and value.
    pub(crate) fn parse_var_decl_common(&mut self) -> Result<(String, Template), ParseError> {
        self.advance();

        let name = self.parse_ident_name("variable name")?;

        self.expect_with_context(TokenType::Assign, "in variable declaration")?;

        let value = self.parse_expr()?;
        self.expect_with_context(TokenType::Semicolon, "after variable declaration")?;

        Ok((name, value))
    }

    /// Dispatches to the correct statement parser for statements inside a function body.
    pub(crate) fn parse_fn_stmt(&mut self) -> Result<FnStmt, ParseError> {
        match &self.current_token().token_type {
            TokenType::Log => Ok(FnStmt::Log(self.parse_expr_stmt("after `log`")?)),
            TokenType::Cd => Ok(FnStmt::Cd(self.parse_expr_stmt("after `cd`")?)),
            TokenType::Var => {
                let (name, value) = self.parse_var_decl_common()?;
                Ok(FnStmt::Bind { name, value })
            }
            TokenType::Env => self.parse_env_block(),
            TokenType::Switch => self.parse_switch_stmt(),
            TokenType::Template(_) => self.parse_run_shell_cmd_stmt(),
            TokenType::Semicolon => Err(ParseError::new(
                self.eof_aware_span(),
                "unexpected `;` (empty statement)".to_string(),
            )),
            _ => Err(self.unexpected_stmt_start_error("log, cd, var, env, switch, or $(cmd)")),
        }
    }

    /// Parse a `{ ... }` block, invoking `parse_item` for each statement until
    /// the closing `}`. Shared by function bodies, `env` bodies, and `switch`
    /// arms so the open/close brace handling lives in one place.
    fn parse_braced_block<T>(
        &mut self,
        open_ctx: &str,
        close_ctx: &str,
        mut parse_item: impl FnMut(&mut Self) -> Result<T, ParseError>,
    ) -> Result<Vec<T>, ParseError> {
        self.expect_with_context(TokenType::LBrace, open_ctx)?;
        let mut items = Vec::new();
        while self.current_token().token_type != TokenType::RBrace {
            if self.current_token().token_type == TokenType::Eof {
                self.take_pending_lex_error()?;
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    format!("expected `}}` {}, found end of file", close_ctx),
                ));
            }
            items.push(parse_item(self)?);
        }
        self.expect_with_context(TokenType::RBrace, close_ctx)?;
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use crate::syntax::parser::test_support::*;

    #[test]
    fn test_multiple_top_level_statements() {
        let input = "var x = (hello);\n\
                      pr p { fn b { log (x); }; };\n\
                      run s { p::b; };";
        let prog = parse_program(input).unwrap();
        assert_eq!(count_stmt_types(&prog), vec!["var", "pr", "run"]);
    }

    #[test]
    fn test_unexpected_token_at_top_level() {
        let result = parse_program("fooobar = (bar);");
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter()
                .any(|e| { e.to_string().contains("expected var, pr, or run") })
        );
    }

    #[test]
    fn test_toplevel_fn_rejected() {
        let result = parse_program("fn f { log (hi); };");
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| e
                .to_string()
                .contains("functions must be declared inside a `pr` block")),
            "got: {:?}",
            errs
        );
    }

    #[test]
    fn test_underscore_outside_case_pattern() {
        let result = parse_program("pr t { fn test { log (hi); _; }; };");
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| e
                .to_string()
                .contains("`_` is only valid as a case pattern")),
            "got: {:?}",
            errs
        );
    }
}
