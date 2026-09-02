//! Tokenizer: consumes characters and produces [`Token`]s for the parser.
//! Handles string escapes, template segment detection, and keyword lookup.

use super::Lexer;
use crate::syntax::source::{Part, Template};
use crate::syntax::token::{Token, TokenType, lookup_ident};

impl Lexer {
    pub(super) fn read_char(&mut self) {
        if let Some(ch) = self.ch {
            self.byte_offset += ch.len_utf8();
        }
        self.ch = if self.read_pos < self.input.len() {
            Some(self.input[self.read_pos])
        } else {
            None
        };
        self.pos = self.read_pos;
        self.read_pos += 1;

        if self.ch == Some('\n') {
            self.line += 1;
            self.col = 0;
        } else {
            self.col += 1;
        }
    }

    pub(super) fn skip_whitespace(&mut self) {
        while let Some(ch) = self.ch {
            if !ch.is_whitespace() {
                break;
            }
            self.read_char();
        }
    }

    pub(super) fn skip_comment(&mut self) {
        while self.ch != Some('\n') && self.ch.is_some() {
            self.read_char();
        }
    }

    pub(super) fn read_ident(&mut self) -> Token {
        let start_line = self.line;
        let start_col = self.col;
        let start_pos = self.pos;
        let start_byte_offset = self.byte_offset;

        while let Some(ch) = self.ch {
            if ch.is_alphanumeric() || ch == '_' {
                self.read_char();
            } else {
                break;
            }
        }

        let ident: String = self.input[start_pos..self.pos].iter().collect();
        let token_type = lookup_ident(&ident);
        Token::new(
            token_type,
            start_line,
            start_col,
            start_byte_offset,
            self.byte_offset - start_byte_offset,
        )
    }

    /// Read a template expression starting at the current character. The current
    /// character must be `(` (general template), `$` followed by `(` (command
    /// substitution), or `@` followed by `(` (variable reference).
    pub(super) fn read_template_token(
        &mut self,
        start_line: usize,
        start_col: usize,
        start_byte_offset: usize,
    ) -> Token {
        let start_offset = start_byte_offset;

        let mut parts = match self.ch {
            Some('$') => {
                // `$( cmd )` -> a single Cmd part wrapping the inner template.
                self.read_char(); // consume '$'
                self.read_char(); // consume '('
                match self.read_template_parts() {
                    Ok(inner) => {
                        if template_is_only_whitespace(&inner) {
                            return self.illegal(
                                "empty command substitution",
                                start_line,
                                start_col,
                                start_offset,
                            );
                        }
                        let len = self.byte_offset - start_offset;
                        vec![Part::Cmd(Template {
                            parts: inner,
                            offset: start_offset,
                            len,
                            source_name: String::new(),
                        })]
                    }
                    Err(msg) => {
                        return self.illegal(&msg, start_line, start_col, start_offset);
                    }
                }
            }
            Some('@') => {
                // `@( name )` -> a single Var part.
                self.read_char(); // consume '@'
                self.read_char(); // consume '('
                let name = self.read_ident_chars();
                if self.ch != Some(')') {
                    return self.illegal(
                        "unterminated variable reference",
                        start_line,
                        start_col,
                        start_offset,
                    );
                }
                if name.is_empty() {
                    return self.illegal(
                        "empty variable reference",
                        start_line,
                        start_col,
                        start_offset,
                    );
                }
                self.read_char(); // consume ')'
                vec![Part::Var(name)]
            }
            Some('(') => {
                self.read_char(); // consume '('
                match self.read_template_parts() {
                    Ok(parts) => parts,
                    Err(msg) => {
                        return self.illegal(&msg, start_line, start_col, start_offset);
                    }
                }
            }
            _ => {
                return self.illegal(
                    "expected template starting with `(`",
                    start_line,
                    start_col,
                    start_offset,
                );
            }
        };

        let len = self.byte_offset - start_offset;
        // Coalesce a single trailing/leading empty literal into nothing.
        if parts.len() == 2
            && let (Some(Part::Lit(a)), Some(Part::Lit(b))) = (parts.first(), parts.last())
            && a.is_empty()
            && b.is_empty()
        {
            parts = Vec::new();
        }
        Token::new(
            TokenType::Template(Template {
                parts,
                offset: start_offset,
                len,
                source_name: String::new(),
            }),
            start_line,
            start_col,
            start_offset,
            len,
        )
    }

    fn illegal(
        &mut self,
        msg: &str,
        _start_line: usize,
        _start_col: usize,
        start_offset: usize,
    ) -> Token {
        Token::new(
            TokenType::Illegal(msg.to_string()),
            self.line,
            self.col,
            start_offset,
            (self.byte_offset - start_offset).max(1),
        )
    }

    /// Read the body of a template until the matching top-level `)`. Inside, `@(`
    /// starts a `Var` part (its `)` is mandatory and its name must not be empty)
    /// and `$(` starts a nested `Cmd` part (whose own body is read recursively
    /// and must not be empty). All other characters accumulate into a literal
    /// part.
    ///
    /// Returns `Err(message)` when the template is malformed, so the caller can
    /// emit an `Illegal` token carrying that message instead of a malformed
    /// `Template`.
    fn read_template_parts(&mut self) -> Result<Vec<Part>, String> {
        let mut parts: Vec<Part> = Vec::new();
        let mut lit = String::new();

        loop {
            match self.ch {
                None => {
                    // Unterminated template: signal the caller so it can error.
                    return Err("unterminated template".to_string());
                }
                Some(')') => {
                    self.read_char();
                    break;
                }
                Some('@') if self.peek_next() == Some('(') => {
                    if !lit.is_empty() {
                        parts.push(Part::Lit(std::mem::take(&mut lit)));
                    }
                    self.read_char(); // '@'
                    self.read_char(); // '('
                    let name = self.read_ident_chars();
                    if self.ch != Some(')') {
                        return Err("unterminated variable reference".to_string());
                    }
                    if name.is_empty() {
                        return Err("empty variable reference".to_string());
                    }
                    self.read_char(); // ')'
                    parts.push(Part::Var(name));
                }
                Some('$') if self.peek_next() == Some('(') => {
                    if !lit.is_empty() {
                        parts.push(Part::Lit(std::mem::take(&mut lit)));
                    }
                    let cmd_offset = self.pos;
                    self.read_char(); // '$'
                    self.read_char(); // '('
                    let inner = self.read_template_parts()?;
                    if template_is_only_whitespace(&inner) {
                        return Err("empty command substitution".to_string());
                    }
                    parts.push(Part::Cmd(Template {
                        parts: inner,
                        offset: cmd_offset,
                        len: self.pos - cmd_offset,
                        source_name: String::new(),
                    }));
                }
                Some(ch) => {
                    lit.push(ch);
                    self.read_char();
                }
            }
        }

        if !lit.is_empty() {
            parts.push(Part::Lit(lit));
        }
        Ok(parts)
    }

    /// Read an identifier's worth of characters (`[A-Za-z0-9_]`).
    fn read_ident_chars(&mut self) -> String {
        let mut name = String::new();
        while let Some(ch) = self.ch {
            if ch.is_alphanumeric() || ch == '_' {
                name.push(ch);
                self.read_char();
            } else {
                break;
            }
        }
        name
    }
}

/// A command substitution is empty when it contains no variable or nested
/// command parts and its literal text is only whitespace; running it would
/// be a silent no-op. A general `()` template is deliberately exempt: it is
/// the empty-string literal (used by `case ()` patterns).
fn template_is_only_whitespace(parts: &[Part]) -> bool {
    parts
        .iter()
        .all(|p| matches!(p, Part::Lit(s) if s.trim().is_empty()))
}
