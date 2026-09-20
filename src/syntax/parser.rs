use crate::diagnostics::Span;
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

#[cfg(test)]
mod test_support;

/// Recursive-descent parser for the kiru DSL. Wraps a `Lexer` and produces
/// a sequence of `TopLevel` items (statements and imports).
pub(crate) struct Parser {
    lexer: Lexer,
    current: Token,
    source_len: usize,
    /// First lex error hit while advancing. Lex errors are deferred to the
    /// next `parse_toplevel` call so the statement currently being parsed
    /// finishes normally before compilation aborts.
    pending_lex_error: Option<ParseError>,
}

impl Parser {
    /// Constructs a new Parser from the given Lexer, advancing to the first token.
    pub(crate) fn new(lexer: Lexer) -> Self {
        let source_len = lexer.source_len();
        let mut parser = Parser {
            lexer,
            current: Token::new(TokenType::Eof, source_len, 0),
            source_len,
            pending_lex_error: None,
        };
        parser.advance();
        parser
    }

    /// Pulls the next token from the lexer into `current`. A lex error is
    /// stashed (first one wins) and an EOF token takes its place, so the
    /// parser always holds a usable token.
    fn pull_token(&mut self) {
        self.current = match self.lexer.next_token() {
            Ok(token) => token,
            Err(e) => {
                if self.pending_lex_error.is_none() {
                    self.pending_lex_error = Some(e);
                }
                Token::new(TokenType::Eof, self.source_len, 0)
            }
        };
    }

    /// Returns a reference to the current token.
    fn current_token(&self) -> &Token {
        &self.current
    }

    /// Advances to the next token from the lexer.
    fn advance(&mut self) {
        self.pull_token();
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
        } else if self.current_token().token_type == TokenType::Eof {
            // The EOF may be synthetic, standing in for an unreadable token.
            self.take_pending_lex_error()?;
            Err(ParseError::new(
                self.eof_aware_span(),
                format!(
                    "expected {} {}, found {}",
                    format_token_type(&ty),
                    context,
                    format_token(self.current_token())
                ),
            ))
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
    /// or run) and advances past it. Reserved keywords and non-identifier
    /// tokens are rejected with a context-aware message.
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

    /// Builds the error for an unexpected token. The found token carries the
    /// message (`unexpected `switch``) - what is grammatically expected in
    /// each position is the grammar's business, not a list of alternatives
    /// to re-enumerate on every rejection; the rendered source snippet shows
    /// where, the found token shows what.
    fn unexpected_token_error(&mut self) -> ParseError {
        // A synthetic EOF stands in for an unreadable token: report the real
        // lex error instead of "unexpected end of file".
        if self.current_token().token_type == TokenType::Eof
            && let Err(error) = self.take_pending_lex_error()
        {
            return error;
        }
        ParseError::new(
            self.eof_aware_span(),
            format!("unexpected {}", format_token(self.current_token())),
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
            TokenType::Import(Some(args)) => {
                let path = self.single_argument(args.clone(), "import")?;
                self.advance();
                self.expect_with_context(TokenType::Semicolon, "after import path")?;
                Ok(Some(TopLevel::Import(path)))
            }
            // A bare `import` (spaced path) is not a valid statement start,
            // like every other bare call-form keyword: rejected generically.
            _ => self
                .parse_top_level_stmt()
                .map(|stmt| Some(TopLevel::Stmt(stmt))),
        }
    }

    /// Extract the single argument a one-argument builtin (`log`, `cd`,
    /// `switch`, `case`, `import`) expects. `()` is the empty template for
    /// these builtins (`case()` matches the empty string), while any other
    /// count is a parse error naming the builtin.
    fn single_argument(
        &self,
        mut args: Vec<crate::syntax::source::Template>,
        keyword: &str,
    ) -> Result<crate::syntax::source::Template, ParseError> {
        match args.len() {
            0 => Ok(crate::syntax::source::Template {
                parts: Vec::new(),
                offset: self.current_token().offset,
                len: self.current_token().len,
            }),
            1 => Ok(args.remove(0)),
            _ => Err(ParseError::new(
                self.eof_aware_span(),
                format!("`{keyword}` takes exactly one argument"),
            )),
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
            // Functions are named bundles, declared before use and
            // importable across files; a later declaration of the same name
            // wins from that point on.
            TokenType::Fn => self.parse_fn_decl(),
            TokenType::Run => self.parse_run_decl(),
            _ => Err(self.unexpected_token_error()),
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
                Var | Fn | Run => break,
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
    /// Every statement is call-shaped: the keyword and its call parens must
    /// be adjacent (the lexer fuses them), so spaced `log(x)` and friends
    /// arrive as bare keywords and are rejected here through the generic
    /// unexpected-statement error.
    pub(crate) fn parse_fn_stmt(&mut self) -> Result<FnStmt, ParseError> {
        match &self.current_token().token_type {
            TokenType::Log(Some(args)) => {
                let message = self.single_argument(args.clone(), "log")?;
                self.advance();
                self.expect_with_context(TokenType::Semicolon, "after `log`")?;
                Ok(FnStmt::Log(message))
            }
            TokenType::Cd(Some(args)) => {
                let path = self.single_argument(args.clone(), "cd")?;
                self.advance();
                self.expect_with_context(TokenType::Semicolon, "after `cd`")?;
                Ok(FnStmt::Cd(path))
            }
            TokenType::Var => {
                let (name, value) = self.parse_var_decl_common()?;
                Ok(FnStmt::Bind { name, value })
            }
            TokenType::EnvOpen => self.parse_env_block(),
            TokenType::Switch(Some(_)) => self.parse_switch_stmt(),
            TokenType::Async(Some(_)) => self.parse_async_block(),
            TokenType::Exec(Some(_)) => self.parse_exec_stmt(),
            TokenType::Project(Some(_)) => self.parse_project_block(),
            TokenType::Call { .. } => self.parse_call_stmt(),
            TokenType::Semicolon => Err(ParseError::new(
                self.eof_aware_span(),
                "unexpected `;` (empty statement)".to_string(),
            )),
            _ => Err(self.unexpected_token_error()),
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
                      fn b { log(x); };\n\
                      run s { b(); };";
        let prog = parse_program(input).unwrap();
        assert_eq!(count_stmt_types(&prog), vec!["var", "fn", "run"]);
    }

    #[test]
    fn test_unexpected_token_at_top_level() {
        let result = parse_program("fooobar = (bar);");
        assert!(result.is_err());
        let errs = result.unwrap_err();
        // The rejection is generic: what was found, not a list of
        // alternatives.
        assert!(
            errs.iter()
                .any(|e| e.to_string().contains("unexpected `fooobar`"))
        );
    }

    #[test]
    fn test_toplevel_fn_is_a_function() {
        let prog = parse_program("fn f { log(hi); };").unwrap();
        assert_eq!(count_stmt_types(&prog), vec!["fn"]);
    }

    #[test]
    fn test_underscore_is_an_ordinary_identifier() {
        // `_` is a plain identifier; a bare `_` statement is rejected
        // generically.
        let result = parse_program("fn test { log(hi); _; };");
        let errs = result.unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.to_string().contains("unexpected `_`")),
            "got: {:?}",
            errs
        );
    }

    #[test]
    fn test_spaced_statement_form_is_rejected_generically() {
        // The keyword and its call parens must be adjacent; a spaced
        // statement arrives as a bare keyword and is rejected like any
        // other unexpected token, without enumerating the alternatives.
        let result = parse_program("fn x { switch @(v) { case(v) { log(x); }; }; };");
        let errs = result.unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.to_string().contains("unexpected `switch`")),
            "got: {:?}",
            errs
        );

        let result = parse_program("import (./shared.kiru);");
        let errs = result.unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.to_string().contains("unexpected `import`")),
            "got: {:?}",
            errs
        );
    }

    #[test]
    fn test_default_is_a_bare_wildcard_arm() {
        // `default` takes no pattern, so it has no call parens; the
        // parenthesized spelling is rejected like any other invalid arm.
        let prog = parse_program(
            "fn x {\
                switch(@(v)) {\
                  case(a) { log(a); };\
                  default { log(other); };\
                };\
            };",
        )
        .unwrap();
        match &prog.top_level_items[0] {
            crate::syntax::TopLevel::Stmt(crate::syntax::Stmt::Fn { body, .. }) => {
                assert!(
                    matches!(
                        &body[0],
                        crate::syntax::FnStmt::Switch { arms, .. }
                            if matches!(arms[1].pattern, crate::syntax::source::ArmPattern::Default)
                    ),
                    "got: {:?}",
                    body[0]
                );
            }
            other => panic!("expected fn, got {:?}", other),
        }

        let result = parse_program(
            "fn x {\
                switch(@(v)) {\
                  default() { log(other); };\
                };\
            };",
        );
        let errs = result.unwrap_err();
        assert!(
            errs.iter()
                .any(|e| { e.to_string().contains("expected `{` after switch pattern") }),
            "got: {:?}",
            errs
        );
    }
}
