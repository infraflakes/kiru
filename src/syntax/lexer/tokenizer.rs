//! Tokenizer: consumes characters and produces [`Token`]s for the parser.
//! Handles template segment detection and keyword lookup. Any malformed
//! template is a lex error, never a token.

use super::Lexer;
use crate::syntax::error::ParseError;
use crate::syntax::source::{Part, Template};
use crate::syntax::token::{
    KeywordForm, Token, TokenType, fuse_call_arguments, is_identifier, keyword_shape, lookup_ident,
};

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

    /// Read an identifier or keyword. Reserved keywords become their bare
    /// token type; any `word` immediately followed by `(` is fused into a
    /// call-shaped token with the template (or, for `env`, the pair-list
    /// opening) as its payload. Whitespace between the word and `(` prevents
    /// fusion, so `log (x)` stays keyword + template and is rejected by the
    /// parser generically. The keyword's grammar shape comes from the
    /// `KEYWORDS` table; the bare-to-fused mapping lives in `fuse_call_arguments`.
    pub(super) fn read_ident(&mut self) -> Result<Token, ParseError> {
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

        let token_type = if self.ch == Some('(') {
            match (&token_type, keyword_shape(&token_type)) {
                // A user function call: `name(args)`.
                (TokenType::Ident(_), _) => {
                    let args = self.read_call_arguments()?;
                    TokenType::Call { name: ident, args }
                }
                // Call-form keywords: the argument list fuses into the token.
                (_, Some(KeywordForm::TemplateCall)) => {
                    let args = self.read_call_arguments()?;
                    fuse_call_arguments(token_type, args)
                }
                // `exec(` fuses the whole region as one raw command
                // template: `;` is literal shell text, only `@()`/`$()`
                // are substitutions.
                (_, Some(KeywordForm::Command)) => {
                    let args = self.read_command_argument()?;
                    fuse_call_arguments(token_type, args)
                }
                // `env(` opens the pair list; its pairs are ordinary tokens.
                (_, Some(KeywordForm::PairList)) => {
                    self.read_char(); // consume '('
                    TokenType::EnvOpen
                }
                // Declaration and bare keywords take a name or nothing, never
                // call parens: leave them bare and let the parser reject the
                // stray template generically.
                (_, Some(KeywordForm::Bare | KeywordForm::Declaration)) | (_, None) => token_type,
            }
        } else {
            token_type
        };

        Ok(Token::new(
            token_type,
            start_byte_offset,
            self.byte_offset - start_byte_offset,
        ))
    }

    /// Read `exec`'s payload: the whole `(...)` region as one raw command
    /// template (empty for `exec()`), so top-level `;` stays literal. Plain
    /// parentheses inside are data (see [`Self::read_template_parts_until`]),
    /// which is what makes subshells and grouped commands writable.
    fn read_command_argument(
        &mut self,
    ) -> Result<Vec<crate::syntax::source::Template>, ParseError> {
        let open_offset = self.byte_offset;
        self.read_char(); // consume '('
        if self.ch == Some(')') {
            self.read_char();
            return Ok(Vec::new());
        }
        let arg_offset = self.byte_offset;
        let (parts, _) = self
            .read_template_parts_until(false)
            .map_err(|msg| self.unexpected(msg, open_offset))?;
        Ok(vec![crate::syntax::source::Template {
            parts,
            offset: arg_offset,
            len: self.byte_offset - arg_offset,
        }])
    }

    /// Read a call's argument list: the `(...)` region split at top-level
    /// `;` into templates. Nested `@()`/`$()` parts and balanced plain
    /// parentheses stay atomic, so a `;` inside them is data; an empty
    /// region is zero arguments (`name()`). A trailing `;` before `)` does
    /// not create an empty argument.
    fn read_call_arguments(&mut self) -> Result<Vec<crate::syntax::source::Template>, ParseError> {
        let open_offset = self.byte_offset;
        self.read_char(); // consume '('
        if self.ch == Some(')') {
            self.read_char();
            return Ok(Vec::new());
        }

        let mut args = Vec::new();
        loop {
            let arg_offset = self.byte_offset;
            let (parts, ended_on_semicolon) = self
                .read_template_parts_until(true)
                .map_err(|msg| self.unexpected(msg, open_offset))?;
            args.push(crate::syntax::source::Template {
                parts: trim_argument_parts(parts),
                offset: arg_offset,
                len: self.byte_offset - arg_offset,
            });
            if !ended_on_semicolon {
                break;
            }
            // A trailing `;` before `)` ends the list without an empty arg.
            if self.ch == Some(')') {
                self.read_char();
                break;
            }
        }
        // `name(a;)` style trailing separators leave no empty tail argument.
        if args.last().is_some_and(|arg| arg.parts.is_empty()) {
            args.pop();
        }
        Ok(args)
    }

    /// Read a template expression starting at the current character. The current
    /// character must be `(` (general template), `$` followed by `(` (command
    /// substitution), or `@` followed by `(` (variable reference).
    pub(super) fn read_template_token(
        &mut self,
        start_byte_offset: usize,
    ) -> Result<Token, ParseError> {
        let start_offset = start_byte_offset;

        let parts = match self.ch {
            Some('$') => {
                // `$( cmd )` -> a single Cmd part wrapping the inner template.
                self.read_char(); // consume '$'
                self.read_char(); // consume '('
                match self.read_template_parts() {
                    Ok(inner) => {
                        if template_is_only_whitespace(&inner) {
                            return Err(self.unexpected(
                                "empty command substitution".to_string(),
                                start_offset,
                            ));
                        }
                        let len = self.byte_offset - start_offset;
                        vec![Part::Cmd(Template {
                            parts: inner,
                            offset: start_offset,
                            len,
                        })]
                    }
                    Err(msg) => return Err(self.unexpected(msg, start_offset)),
                }
            }
            Some('@') => {
                // `@( name )` -> a single Var part.
                self.read_char(); // consume '@'
                self.read_char(); // consume '('
                let name = self.read_ident_chars();
                if self.ch != Some(')') {
                    return Err(self
                        .unexpected("unterminated variable reference".to_string(), start_offset));
                }
                if name.is_empty() {
                    return Err(
                        self.unexpected("empty variable reference".to_string(), start_offset)
                    );
                }
                if !is_identifier(&name) {
                    return Err(self.unexpected(
                        format!("`{name}` is not a valid variable name"),
                        start_offset,
                    ));
                }
                self.read_char(); // consume ')'
                vec![Part::Var(name)]
            }
            Some('(') => {
                self.read_char(); // consume '('
                match self.read_template_parts() {
                    Ok(parts) => parts,
                    Err(msg) => return Err(self.unexpected(msg, start_offset)),
                }
            }
            _ => {
                return Err(self.unexpected(
                    "expected template starting with `(`".to_string(),
                    start_offset,
                ));
            }
        };

        let len = self.byte_offset - start_offset;
        Ok(Token::new(
            TokenType::Template(Template {
                parts,
                offset: start_offset,
                len,
            }),
            start_offset,
            len,
        ))
    }

    /// Read the body of a template until the matching top-level `)`. Inside, `@(`
    /// starts a `Var` part (its `)` is mandatory and its name must be a plain
    /// identifier) and `$(` starts a nested `Cmd` part (whose own body is read
    /// recursively and must not be empty). Every other character accumulates
    /// into a literal part, including plain parentheses.
    ///
    /// Plain `(` and `)` are data in matched pairs: `(` opens a depth level and
    /// `)` closes it, both staying in the literal text. Only the `)` that
    /// returns to depth zero ends the region, so `(a (b) c)` is one template
    /// and `exec(cd x && (make))` is one command. An unbalanced `)` cannot be
    /// data - closing the region is exactly what it does; produce one with
    /// `$()` when a command needs it literally. With `stop_on_semicolon`, a `;`
    /// at depth zero ends the parts instead (call argument lists); the caller
    /// learns which terminator was hit.
    ///
    /// Returns `Err(message)` when the template is malformed, so the caller can
    /// raise a lex error instead of producing a malformed `Template`.
    fn read_template_parts(&mut self) -> Result<Vec<Part>, String> {
        self.read_template_parts_until(false)
            .map(|(parts, _)| parts)
    }

    /// The shared part reader behind standalone templates and call argument
    /// lists (see [`Self::read_template_parts`]). Returns the parts and
    /// whether they ended on a top-level `;` instead of the closing `)`.
    fn read_template_parts_until(
        &mut self,
        stop_on_semicolon: bool,
    ) -> Result<(Vec<Part>, bool), String> {
        let mut parts: Vec<Part> = Vec::new();
        let mut lit = String::new();
        // Plain parentheses are data in matched pairs; only depth zero ends
        // the region.
        let mut paren_depth = 0usize;

        loop {
            match self.ch {
                None => {
                    // Unterminated template: signal the caller so it can error.
                    return Err("unterminated template".to_string());
                }
                Some(')') if paren_depth == 0 => {
                    self.read_char();
                    return Ok((finish_parts(parts, lit), false));
                }
                Some(')') => {
                    lit.push(')');
                    paren_depth -= 1;
                    self.read_char();
                }
                Some('(') => {
                    lit.push('(');
                    paren_depth += 1;
                    self.read_char();
                }
                Some(';') if stop_on_semicolon && paren_depth == 0 => {
                    self.read_char();
                    return Ok((finish_parts(parts, lit), true));
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
                    if !is_identifier(&name) {
                        return Err(format!("`{name}` is not a valid variable name"));
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
                    }));
                }
                Some(ch) => {
                    lit.push(ch);
                    self.read_char();
                }
            }
        }
    }

    /// Read the character span of a variable name (`[A-Za-z0-9_]`); whether
    /// it is a valid identifier is checked by the caller.
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

/// Fold a trailing literal into a part list (the readers accumulate literal
/// characters separately and flush them when a non-literal part or the end
/// of the region appears).
fn finish_parts(mut parts: Vec<Part>, lit: String) -> Vec<Part> {
    if !lit.is_empty() {
        parts.push(Part::Lit(lit));
    }
    parts
}

/// Trim whitespace surrounding an argument template: separators are written
/// with spaces for readability (`deploy(a; b)`), and those boundary spaces
/// are layout, not data.
fn trim_argument_parts(mut parts: Vec<Part>) -> Vec<Part> {
    if let Some(Part::Lit(first)) = parts.first_mut() {
        *first = first.trim_start().to_string();
    }
    if let Some(Part::Lit(last)) = parts.last_mut() {
        *last = last.trim_end().to_string();
    }
    parts.retain(|part| !matches!(part, Part::Lit(text) if text.is_empty()));
    parts
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
