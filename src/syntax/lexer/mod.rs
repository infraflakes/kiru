use crate::syntax::token::{Token, TokenType};

mod tokenizer;

/// Character-level lexer that emits tokens from source text.
#[derive(Debug)]
pub(crate) struct Lexer {
    /// Source characters as a Vec<char> for O(1) index access.
    pub(super) input: Vec<char>,
    /// Current byte index into `input` (points at the next character to read).
    pub(super) pos: usize,
    /// One past `pos`, used by `read_char` to advance after peeking.
    pub(super) read_pos: usize,
    /// The current character at `pos`, or `None` at end-of-input.
    pub(super) ch: Option<char>,
    /// Current 1-indexed line number (for diagnostics).
    pub(super) line: usize,
    /// Current 1-indexed column number within the line (for diagnostics).
    pub(super) col: usize,
    /// Byte offset of `pos` in the original source string, used for
    /// token span computation when characters are multi-byte.
    pub(super) byte_offset: usize,
}

impl Lexer {
    /// Constructs a new Lexer from the given input string.
    pub(crate) fn new(input: String) -> Self {
        let mut lexer = Self {
            input: input.chars().collect(),
            pos: 0,
            read_pos: 0,
            ch: None,
            line: 1,
            col: 0,
            byte_offset: 0,
        };
        lexer.read_char();
        lexer
    }

    /// Returns the source text length in bytes.
    pub(crate) fn source_len(&self) -> usize {
        self.input.iter().map(|ch| ch.len_utf8()).sum()
    }

    /// The character one position ahead, without consuming it.
    fn peek_next(&self) -> Option<char> {
        self.input.get(self.read_pos).copied()
    }

    /// Consume the current character and produce a single-character token.
    fn single_char_token(
        &mut self,
        ty: TokenType,
        start_line: usize,
        start_col: usize,
        start_byte_offset: usize,
    ) -> Token {
        self.read_char();
        Token::new(
            ty,
            start_line,
            start_col,
            start_byte_offset,
            self.byte_offset - start_byte_offset,
        )
    }

    /// Consume the current character plus the peeked one and produce a
    /// two-character token.
    fn two_char_token(
        &mut self,
        ty: TokenType,
        start_line: usize,
        start_col: usize,
        start_byte_offset: usize,
    ) -> Token {
        self.read_char();
        self.read_char();
        Token::new(
            ty,
            start_line,
            start_col,
            start_byte_offset,
            self.byte_offset - start_byte_offset,
        )
    }

    /// Returns the next Token from the input.
    pub(crate) fn next_token(&mut self) -> Token {
        loop {
            self.skip_whitespace();
            if self.ch != Some('#') {
                break;
            }
            self.skip_comment();
        }

        let start_line = self.line;
        let start_col = self.col;
        let start_byte_offset = self.byte_offset;
        let ch = self.ch;

        match ch {
            None => Token::new(TokenType::Eof, start_line, start_col, start_byte_offset, 0),
            Some('{') => {
                self.single_char_token(TokenType::LBrace, start_line, start_col, start_byte_offset)
            }
            Some('}') => {
                self.single_char_token(TokenType::RBrace, start_line, start_col, start_byte_offset)
            }
            Some('[') => self.single_char_token(
                TokenType::LBracket,
                start_line,
                start_col,
                start_byte_offset,
            ),
            Some(']') => self.single_char_token(
                TokenType::RBracket,
                start_line,
                start_col,
                start_byte_offset,
            ),
            Some('(') => self.read_template_token(start_line, start_col, start_byte_offset),
            Some(')') => {
                self.single_char_token(TokenType::RParen, start_line, start_col, start_byte_offset)
            }
            Some(';') => self.single_char_token(
                TokenType::Semicolon,
                start_line,
                start_col,
                start_byte_offset,
            ),
            Some('=') if self.peek_next() == Some('>') => self.two_char_token(
                TokenType::ChainArrow,
                start_line,
                start_col,
                start_byte_offset,
            ),
            Some('=') => {
                self.single_char_token(TokenType::Assign, start_line, start_col, start_byte_offset)
            }
            Some(':') if self.peek_next() == Some(':') => self.two_char_token(
                TokenType::NamespaceSep,
                start_line,
                start_col,
                start_byte_offset,
            ),
            Some('$') if self.peek_next() == Some('(') => {
                self.read_template_token(start_line, start_col, start_byte_offset)
            }
            Some('@') if self.peek_next() == Some('(') => {
                self.read_template_token(start_line, start_col, start_byte_offset)
            }
            Some(':') => self.single_char_token(
                TokenType::Illegal("unexpected character: :".to_string()),
                start_line,
                start_col,
                start_byte_offset,
            ),
            Some(ch) if ch.is_alphabetic() || ch == '_' => self.read_ident(),
            Some(ch) => self.single_char_token(
                TokenType::Illegal(format!("unexpected character: {}", ch)),
                start_line,
                start_col,
                start_byte_offset,
            ),
        }
    }
}

#[cfg(test)]
/// Drive the lexer to EOF, returning every token (including EOF) in order.
fn drain_tokens(input: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(input.to_string());
    let mut tokens = Vec::new();
    loop {
        let tok = lexer.next_token();
        let is_eof = matches!(tok.token_type, TokenType::Eof);
        tokens.push(tok);
        if is_eof {
            break;
        }
    }
    tokens
}

#[cfg(test)]
fn collect_tokens(input: &str) -> Vec<TokenType> {
    drain_tokens(input)
        .into_iter()
        .filter(|tok| !matches!(tok.token_type, TokenType::Eof | TokenType::Illegal(_)))
        .map(|tok| tok.token_type)
        .collect()
}

#[cfg(test)]
fn extract_errors(input: &str) -> Vec<String> {
    drain_tokens(input)
        .into_iter()
        .filter_map(|tok| match tok.token_type {
            TokenType::Illegal(msg) => Some(msg),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_tokens() {
        let cases = vec![
            ("=", TokenType::Assign),
            ("{", TokenType::LBrace),
            ("}", TokenType::RBrace),
            ("[", TokenType::LBracket),
            ("]", TokenType::RBracket),
            (";", TokenType::Semicolon),
            ("=>", TokenType::ChainArrow),
        ];
        for (input, expected) in cases {
            let mut lexer = Lexer::new(input.to_string());
            assert_eq!(
                lexer.next_token().token_type,
                expected,
                "input: {:?}",
                input
            );
        }
    }

    #[test]
    fn test_keywords() {
        let tokens = collect_tokens("import var pr fn run env log cd switch case");
        assert_eq!(
            tokens,
            vec![
                TokenType::Import,
                TokenType::Var,
                TokenType::Pr,
                TokenType::Fn,
                TokenType::Run,
                TokenType::Env,
                TokenType::Log,
                TokenType::Cd,
                TokenType::Switch,
                TokenType::Case,
            ]
        );
    }

    #[test]
    fn test_identifiers() {
        let cases = vec!["todo", "port1", "idx_port", "url", "myVar", "x", "abc123"];
        for ident in cases {
            let mut lexer = Lexer::new(ident.to_string());
            assert_eq!(
                lexer.next_token().token_type,
                TokenType::Ident(ident.to_string()),
                "ident: {:?}",
                ident
            );
        }
    }

    #[test]
    fn test_template_literals() {
        let cases = vec![
            ("(hello)", "hello", false),
            ("()", "", false),
            ("(a @(b) c)", "a  c", false),
            ("$(echo hi)", "echo hi", false),
            ("@(name)", "name", false),
        ];
        for (input, _, _) in cases {
            let mut lexer = Lexer::new(input.to_string());
            let tok = lexer.next_token();
            assert!(
                matches!(&tok.token_type, TokenType::Template(_)),
                "input {:?} should be a template, got {:?}",
                input,
                tok.token_type
            );
        }
    }

    #[test]
    fn test_template_unterminated() {
        let errors = extract_errors("(unterminated");
        assert!(errors.iter().any(|e| e == "unterminated template"));
    }

    #[test]
    fn test_nested_var_reference_requires_closing_paren() {
        // Top-level `@(` was already strict; the same must hold inside templates.
        let cases = [
            ("(a @(b c)", "unterminated variable reference"),
            ("$(echo @(x", "unterminated variable reference"),
        ];
        for (input, expected) in cases {
            let errors = extract_errors(input);
            assert!(
                errors.iter().any(|e| e == expected),
                "input {:?}: expected {:?}, got {:?}",
                input,
                expected,
                errors
            );
        }
    }

    #[test]
    fn test_empty_var_reference_rejected() {
        let cases = ["@()", "(a @() b)"];
        for input in cases {
            let errors = extract_errors(input);
            assert!(
                errors.iter().any(|e| e == "empty variable reference"),
                "input {:?}: got {:?}",
                input,
                errors
            );
        }
    }

    #[test]
    fn test_empty_command_substitution_rejected() {
        let cases = ["$()", "$(  )", "(a $() b)", "$($( ))"];
        for input in cases {
            let errors = extract_errors(input);
            assert!(
                errors.iter().any(|e| e == "empty command substitution"),
                "input {:?}: got {:?}",
                input,
                errors
            );
        }
    }

    #[test]
    fn test_empty_literal_template_still_valid() {
        // `()` is the empty-string literal (used by `case ()` patterns) and
        // must keep parsing as a template, not an error.
        let errors = extract_errors("()");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_complex_nested_template_still_valid() {
        // Nesting commands and references inside one template must keep working.
        let errors = extract_errors("($(echo @(name))suffix)");
        assert!(errors.is_empty(), "got {:?}", errors);
    }

    #[test]
    fn test_namespace_sep() {
        let mut lexer = Lexer::new("a::b".to_string());
        assert_eq!(
            lexer.next_token().token_type,
            TokenType::Ident("a".to_string())
        );
        assert_eq!(lexer.next_token().token_type, TokenType::NamespaceSep);
        assert_eq!(
            lexer.next_token().token_type,
            TokenType::Ident("b".to_string())
        );
    }

    #[test]
    fn test_comments() {
        let tokens = collect_tokens("# comment\nvar x = (hello);");
        assert_eq!(
            tokens,
            vec![
                TokenType::Var,
                TokenType::Ident("x".to_string()),
                TokenType::Assign,
                TokenType::Template(crate::syntax::source::Template {
                    parts: vec![crate::syntax::source::Part::Lit("hello".to_string())],
                    offset: 18,
                    len: 7,
                    source_name: String::new(),
                }),
                TokenType::Semicolon,
            ]
        );
    }

    #[test]
    fn test_empty_input() {
        let tokens = collect_tokens("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_error_cases() {
        let cases = vec![
            ("bare:", "unexpected character: :"),
            ("@", "unexpected character: @"),
        ];
        for (input, expected_err) in cases {
            let errors = extract_errors(input);
            assert!(
                errors.iter().any(|e| e == expected_err),
                "input {:?}: expected error {:?}, got {:?}",
                input,
                expected_err,
                errors
            );
        }
    }
}
