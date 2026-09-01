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
pub(crate) mod test_support;

/// Recursive-descent parser for the kiru DSL. Wraps a `Lexer` and produces
/// a sequence of `TopLevel` items (statements and imports).
pub(crate) struct Parser {
    lexer: Lexer,
    current: Token,
    /// One token of lookahead, used to disambiguate `pr::fn` references in run
    /// blocks from a bare identifier in a function body.
    next: Token,
    source_len: usize,
    source_name: String,
}

impl Parser {
    /// Records the canonical path of the source file so every parsed node
    /// carries the name used to resolve its diagnostic span. The compiler sets
    /// this before parsing, tests that only inspect structure leave it empty.
    pub(crate) fn with_source_name(mut self, name: String) -> Self {
        self.source_name = name;
        self
    }

    /// Constructs a new Parser from the given Lexer, advancing to the first token.
    pub(crate) fn new(mut lexer: Lexer) -> Self {
        let source_len = lexer.source_len();
        let current = lexer.next_token();
        let next = lexer.next_token();
        Parser {
            lexer,
            current,
            next,
            source_len,
            source_name: String::new(),
        }
    }

    /// Returns a reference to the current token.
    fn current_token(&self) -> &Token {
        &self.current
    }

    /// Advances to the next token from the lexer.
    fn advance(&mut self) {
        self.current = std::mem::replace(&mut self.next, self.lexer.next_token());
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

    /// Returns an error if the current token is an illegal (lexer-error) token,
    /// otherwise `Ok(())`.
    fn err_on_illegal_token(&self) -> Result<(), ParseError> {
        if let TokenType::Illegal(msg) = &self.current_token().token_type {
            return Err(ParseError::new(self.eof_aware_span(), msg.clone()));
        }
        Ok(())
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

    /// Parses one top-level item, returning None on EOF.
    pub(crate) fn parse_toplevel(&mut self) -> Result<Option<TopLevel>, ParseError> {
        if self.current_token().token_type == TokenType::Eof {
            return Ok(None);
        }
        self.err_on_illegal_token()?;
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
        self.err_on_illegal_token()?;
        match self.current_token().token_type {
            TokenType::Var => self.parse_var_decl(),
            TokenType::Pr => self.parse_project_decl(),
            TokenType::Fn => self.parse_fn_decl(),
            TokenType::Run => self.parse_run_decl(),
            _ => Err(self.unexpected_stmt_start_error("var, pr, fn, or run")),
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
        self.err_on_illegal_token()?;
        match &self.current_token().token_type {
            TokenType::Log => self.parse_log_stmt(),
            TokenType::Cd => self.parse_cd_stmt(),
            TokenType::Var => self.parse_fn_var_decl(),
            TokenType::Env => self.parse_env_block(),
            TokenType::Switch | TokenType::Case => self.parse_switch_stmt(),
            TokenType::Template(_) => self.parse_exec_stmt(),
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
                      fn f { log (hi); }\n\
                      pr p { fn b { log (x); } }\n\
                      run s { p::b; }";
        let prog = parse_program(input).unwrap();
        assert_eq!(count_stmt_types(&prog), vec!["var", "fn", "pr", "run"]);
    }

    #[test]
    fn test_unexpected_token_at_top_level() {
        let result = parse_program("fooobar = (bar);");
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter()
                .any(|e| { e.to_string().contains("expected var, pr, fn, or run") })
        );
    }

    #[test]
    fn test_underscore_outside_case_pattern() {
        let result = parse_program("fn test { log (hi); _; }");
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
