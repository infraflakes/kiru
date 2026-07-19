#[cfg(test)]
use crate::dsl::Program;
use crate::dsl::ast::QualifiedFnRef;
use crate::dsl::error::ParseError;
use crate::dsl::lexer::Lexer;
use crate::dsl::token::{Token, TokenType, format_token, format_token_type, is_keyword_token};
use crate::dsl::{
    CaseArm, CasePattern, EnvPair, Expr, FnStmt, InterpolationPart, ProjectField, Stmt, TopLevel,
    VarType,
};
use miette::SourceSpan;

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
    source_len: usize,
    source_name: String,
}

impl Parser {
    /// Convenience constructor that creates a Parser from a source string directly.
    pub(crate) fn from_source(source: String) -> Self {
        Parser::new(Lexer::new(source))
    }

    /// Records the canonical path of the source file so every parsed node
    /// carries the name used to resolve its diagnostic span. The compiler sets
    /// this before parsing — tests that only inspect structure leave it empty.
    pub(crate) fn with_source_name(mut self, name: String) -> Self {
        self.source_name = name;
        self
    }

    /// Constructs a new Parser from the given Lexer, advancing to the first token.
    pub(crate) fn new(mut lexer: Lexer) -> Self {
        let source_len = lexer.source_len();
        let current = lexer.next_token();
        Parser {
            lexer,
            current,
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
        self.current = self.lexer.next_token();
    }

    /// Returns a SourceSpan that safely handles EOF by pointing at the last byte.
    fn eof_aware_span(&self) -> SourceSpan {
        let tok = &self.current;
        if tok.len == 0 && tok.offset >= self.source_len && self.source_len > 0 {
            let start = self.source_len.saturating_sub(1);
            return SourceSpan::new(start.into(), 1);
        }
        let len = if tok.len == 0 {
            1.min(self.source_len.saturating_sub(tok.offset))
        } else {
            tok.len
        };
        SourceSpan::new(tok.offset.into(), len)
    }

    /// Expects a specific token type and advances past it, returning an error with context on mismatch.
    fn expect_with_context(&mut self, ty: TokenType, context: &str) -> Result<(), ParseError> {
        if self.current_token().ty == ty {
            self.advance();
            Ok(())
        } else {
            let token = self.current_token().clone();
            let expected = format_token_type(&ty);
            let found = format_token(&token);
            Err(ParseError::new(
                self.eof_aware_span(),
                format!("expected {} {}, found {}", expected, context, found),
            ))
        }
    }

    /// Reads an identifier as a named declaration target (variable, function,
    /// project, run, or field name) and advances past it. Reserved keywords
    /// and non-identifier tokens are rejected with a context-aware message.
    fn parse_ident_name(
        &mut self,
        kind: &'static str,
        fallback: &'static str,
    ) -> Result<String, ParseError> {
        let name = match &self.current_token().ty {
            TokenType::Ident(name_str) => name_str.clone(),
            ty if is_keyword_token(ty) => {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    format!(
                        "expected {} name, found {} (reserved keyword)",
                        kind,
                        format_token(self.current_token())
                    ),
                ));
            }
            _ => {
                return Err(ParseError::new(self.eof_aware_span(), fallback.to_string()));
            }
        };
        self.advance();
        Ok(name)
    }

    /// Returns an error if the current token is an illegal (lexer-error) token,
    /// otherwise `Ok(())`. Centralizes the identical illegal-token guard
    /// repeated at every parse entry point.
    fn err_on_illegal_token(&self) -> Result<(), ParseError> {
        if let TokenType::Illegal(msg) = &self.current_token().ty {
            return Err(ParseError::new(self.eof_aware_span(), msg.clone()));
        }
        Ok(())
    }

    /// Builds the error for an unexpected statement-start token: `_` is only
    /// valid as a case pattern, otherwise the expected set is reported.
    fn unexpected_stmt_start_error(&self, expected: &'static str) -> ParseError {
        if matches!(&self.current_token().ty, TokenType::Ident(i) if i == "_") {
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

    /// Parses a `$name` (or `$ns::name`) variable reference after the caller has
    /// recorded the starting offset: advances past `$`, reads the identifier
    /// (rejecting keywords), and returns the optional namespace, the name, and
    /// its end offset. A `::` following the first identifier introduces a
    /// namespace qualifier; a second `::` is rejected. Shared by expression and
    /// case-pattern parsing.
    fn parse_dollar_var_name(
        &mut self,
        _start_offset: usize,
        expected: &'static str,
        fallback: &'static str,
    ) -> Result<(Option<String>, String, usize), ParseError> {
        self.advance();
        let first = match &self.current_token().ty {
            TokenType::Ident(name_str) => name_str.clone(),
            ty if is_keyword_token(ty) => {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    format!(
                        "{}, found {} (reserved keyword)",
                        expected,
                        format_token(self.current_token())
                    ),
                ));
            }
            _ => {
                return Err(ParseError::new(self.eof_aware_span(), fallback.to_string()));
            }
        };
        let first_end = self.current_token().offset + self.current_token().len;
        self.advance();

        if self.current_token().ty == TokenType::NamespaceSep {
            self.advance();
            let second = match &self.current_token().ty {
                TokenType::Ident(name_str) => name_str.clone(),
                ty if is_keyword_token(ty) => {
                    return Err(ParseError::new(
                        self.eof_aware_span(),
                        format!(
                            "{}, found {} (reserved keyword)",
                            expected,
                            format_token(self.current_token())
                        ),
                    ));
                }
                _ => {
                    return Err(ParseError::new(self.eof_aware_span(), fallback.to_string()));
                }
            };
            let second_end = self.current_token().offset + self.current_token().len;
            self.advance();
            if self.current_token().ty == TokenType::NamespaceSep {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    "nested namespace qualifier `::` is not allowed".to_string(),
                ));
            }
            Ok((Some(first), second, second_end))
        } else {
            Ok((None, first, first_end))
        }
    }

    /// Parses one top-level item, returning None on EOF.
    pub(crate) fn parse_toplevel(&mut self) -> Result<Option<TopLevel>, ParseError> {
        if self.current_token().ty == TokenType::Eof {
            return Ok(None);
        }
        self.err_on_illegal_token()?;
        match &self.current_token().ty {
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
        let mut program = Program::new();
        let mut errors = Vec::new();

        while self.current_token().ty != TokenType::Eof {
            match self.parse_toplevel() {
                Ok(Some(item)) => program.items.push(item),
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
        match self.current_token().ty {
            TokenType::Var => self.parse_var_decl(),
            TokenType::Pr => self.parse_project_decl(),
            TokenType::Fn => self.parse_fn_decl(),
            TokenType::Run => self.parse_run_decl(),
            _ => Err(self.unexpected_stmt_start_error("var, pr, fn, or run")),
        }
    }

    /// Dispatches to the correct statement parser for statements inside a project body.
    pub(crate) fn parse_project_body_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.err_on_illegal_token()?;
        match self.current_token().ty {
            TokenType::Var => self.parse_var_decl(),
            TokenType::Fn => self.parse_fn_decl(),
            TokenType::Run => self.parse_run_decl(),
            _ => Err(self.unexpected_stmt_start_error("var, fn, or run")),
        }
    }

    #[cfg(test)]
    fn skip_to_stmt_boundary(&mut self) {
        use TokenType::*;
        loop {
            match &self.current_token().ty {
                Eof => break,
                Semicolon | RBrace => {
                    self.advance();
                }
                Var | Pr | Fn | Run => break,
                _ => self.advance(),
            }
        }
    }

    /// Parses `var string/shell name = expr` and returns the type, name, and value.
    pub(crate) fn parse_var_decl_common(&mut self) -> Result<(VarType, String, Expr), ParseError> {
        self.advance();

        let var_type = match &self.current_token().ty {
            TokenType::StringKw => {
                self.advance();
                VarType::String
            }
            TokenType::Shell => {
                self.advance();
                VarType::Shell
            }
            _ => {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    "expected 'string' or 'shell'".to_string(),
                ));
            }
        };

        let name = self.parse_ident_name("variable", "expected variable name")?;

        self.expect_with_context(TokenType::Assign, "in variable declaration")?;

        let value = self.parse_expr()?;
        self.expect_with_context(TokenType::Semicolon, "after variable declaration")?;

        Ok((var_type, name, value))
    }

    /// Dispatches to the correct statement parser for statements inside a function body.
    pub(crate) fn parse_fn_stmt(&mut self) -> Result<FnStmt, ParseError> {
        self.err_on_illegal_token()?;
        match &self.current_token().ty {
            TokenType::Log => self.parse_log_stmt(),
            TokenType::Exec => self.parse_exec_stmt(),
            TokenType::Cd => self.parse_cd_stmt(),
            TokenType::Var => self.parse_fn_var_decl(),
            TokenType::Env => self.parse_env_block(),
            TokenType::Case => self.parse_case_stmt(),
            TokenType::Semicolon => Err(ParseError::new(
                self.eof_aware_span(),
                "unexpected `;` (empty statement)".to_string(),
            )),
            _ => Err(self.unexpected_stmt_start_error("log, exec, cd, var, env, or case")),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::dsl::parser::test_support::*;

    #[test]
    fn test_multiple_top_level_statements() {
        let input = "var string x = `hello`;\n\
                      pr p [url = `u` dir = `d`] { fn f { log `hi`; } run s { f; } }";
        let prog = parse_program(input).unwrap();
        assert_eq!(count_stmt_types(&prog), vec!["var", "pr"]);
    }

    #[test]
    fn test_unexpected_token_at_top_level() {
        let result = parse_program("fooobar = `bar`;");
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.to_string().contains("expected var, pr, fn, or run"))
        );
    }

    #[test]
    fn test_error_recovery_skips_bad_stmt() {
        let result = parse_program("var string x = `hello`;\nfn bad { unknown }");
        match result {
            Ok(prog) => {
                assert_eq!(prog.items.len(), 1);
            }
            Err(errs) => {
                assert!(errs.iter().any(|e| e.to_string().contains("expected log")));
            }
        }
    }

    #[test]
    fn test_underscore_outside_case_pattern() {
        let result = parse_program("pr p { fn test { log `_`; _; } }");
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
