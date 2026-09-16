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
    /// One token of lookahead, used to disambiguate `project::fn` references in run
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

    /// Builds the error for an unexpected token. The found token carries the
    /// message (`unexpected `switch``) - what is grammatically expected in
    /// each position is the grammar's business, not a list of alternatives
    /// to re-enumerate on every rejection; the rendered source snippet shows
    /// where, the found token shows what.
    fn unexpected_token_error(&self) -> ParseError {
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
            TokenType::Import(Some(path)) => {
                let path = path.clone();
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
            TokenType::Project => self.parse_project_decl(),
            // A top-level `fn` is a global function: an AST template that
            // project functions splice in by calling it (`name();`). It is
            // never executable directly and never referenced from run blocks.
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
                Var | Project | Fn | Run => break,
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
            TokenType::Log(Some(template)) => {
                let template = template.clone();
                self.advance();
                self.expect_with_context(TokenType::Semicolon, "after `log`")?;
                Ok(FnStmt::Log(template))
            }
            TokenType::Cd(Some(template)) => {
                let template = template.clone();
                self.advance();
                self.expect_with_context(TokenType::Semicolon, "after `cd`")?;
                Ok(FnStmt::Cd(template))
            }
            TokenType::Var => {
                let (name, value) = self.parse_var_decl_common()?;
                Ok(FnStmt::Bind { name, value })
            }
            TokenType::EnvOpen => self.parse_env_block(),
            TokenType::Switch(Some(_)) => self.parse_switch_stmt(),
            TokenType::Call { .. } => self.parse_call_stmt(),
            TokenType::Template(_) => self.parse_run_shell_cmd_stmt(),
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
                      project p { fn b { log(x); }; };\n\
                      run s { p::b; };";
        let prog = parse_program(input).unwrap();
        assert_eq!(count_stmt_types(&prog), vec!["var", "project", "run"]);
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
    fn test_spaced_statement_form_is_rejected_generically() {
        // The keyword and its call parens must be adjacent; a spaced
        // statement arrives as a bare keyword and is rejected like any
        // other unexpected token, without enumerating the alternatives.
        let result =
            parse_program("project t { fn x { switch @(v) { case(v) { log(x); }; }; }; };");
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
    fn test_toplevel_fn_is_global_function() {
        let prog = parse_program("fn f { log(hi); };").unwrap();
        assert_eq!(count_stmt_types(&prog), vec!["fn"]);
    }

    #[test]
    fn test_underscore_is_an_ordinary_identifier() {
        // The wildcard arm is the bare `default` keyword now; `_` is a
        // plain identifier and a bare `_` statement is rejected generically.
        let result = parse_program("project t { fn test { log(hi); _; }; };");
        let errs = result.unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.to_string().contains("unexpected `_`")),
            "got: {:?}",
            errs
        );
    }

    #[test]
    fn test_default_is_a_bare_wildcard_arm() {
        // `default` takes no pattern, so it has no call parens; the
        // parenthesized spelling is rejected generically like any other
        // invalid arm.
        let prog = parse_program(
            "project t {\
              fn x {\
                switch(@(v)) {\
                  case(a) { log(a); };\
                  default { log(other); };\
                };\
              };\
            };",
        )
        .unwrap();
        match &prog.top_level_items[0] {
            crate::syntax::TopLevel::Stmt(crate::syntax::Stmt::Project { body, .. }) => {
                match &body[0] {
                    crate::syntax::Stmt::Fn { body: fn_body, .. } => {
                        assert!(
                            matches!(
                                &fn_body[0],
                                crate::syntax::FnStmt::Switch { arms, .. }
                                    if matches!(arms[1].pattern, crate::syntax::source::ArmPattern::Default)
                            ),
                            "got: {:?}",
                            fn_body[0]
                        );
                    }
                    other => panic!("expected fn, got {:?}", other),
                }
            }
            other => panic!("expected project, got {:?}", other),
        }

        let result = parse_program(
            "project t {\
              fn x {\
                switch(@(v)) {\
                  default() { log(other); };\
                };\
              };\
            };",
        );
        let errs = result.unwrap_err();
        // `default` takes no pattern, so `default()` commits to the wildcard
        // arm and the stray `(...)` reads as a missing `{`.
        assert!(
            errs.iter()
                .any(|e| { e.to_string().contains("expected `{` after switch pattern") }),
            "got: {:?}",
            errs
        );
    }
}
