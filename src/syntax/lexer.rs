use crate::diagnostics::Span;
use crate::syntax::error::ParseError;
use crate::syntax::token::{Token, TokenType};

mod tokenizer;

/// Character-level lexer that emits tokens from source text. Lexing is
/// strict: any character or template that cannot form a token is an error
/// carrying its span, never a token.
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
    fn single_char_token(&mut self, ty: TokenType, start_byte_offset: usize) -> Token {
        self.read_char();
        Token::new(ty, start_byte_offset, self.byte_offset - start_byte_offset)
    }

    /// Build the error for a character (or template) that cannot form a token.
    fn unexpected(&self, msg: String, start_byte_offset: usize) -> ParseError {
        ParseError::new(
            Span::new(
                start_byte_offset,
                (self.byte_offset - start_byte_offset).max(1),
            ),
            msg,
        )
    }

    /// Returns the next Token from the input, or a lex error with its span.
    /// Every error path consumes at least one character, so callers always
    /// make progress.
    pub(crate) fn next_token(&mut self) -> Result<Token, ParseError> {
        loop {
            self.skip_whitespace();
            if self.ch != Some('#') {
                break;
            }
            self.skip_comment();
        }

        let start_byte_offset = self.byte_offset;
        let ch = self.ch;

        match ch {
            None => Ok(Token::new(TokenType::Eof, start_byte_offset, 0)),
            Some('{') => Ok(self.single_char_token(TokenType::LBrace, start_byte_offset)),
            Some('}') => Ok(self.single_char_token(TokenType::RBrace, start_byte_offset)),
            Some('(') => self.read_template_token(start_byte_offset),
            Some(')') => Ok(self.single_char_token(TokenType::RParen, start_byte_offset)),
            Some(';') => Ok(self.single_char_token(TokenType::Semicolon, start_byte_offset)),
            Some('=') => Ok(self.single_char_token(TokenType::Assign, start_byte_offset)),
            // A bare `$()`/`@()` is not a value: every template is a
            // parenthesized region, with `$()` and `@()` as parts inside it.
            Some('$') if self.peek_next() == Some('(') => {
                self.read_char();
                Err(self.unexpected(
                    "`$(...)` is only valid inside `(...)`".to_string(),
                    start_byte_offset,
                ))
            }
            Some('@') if self.peek_next() == Some('(') => {
                self.read_char();
                Err(self.unexpected(
                    "`@(...)` is only valid inside `(...)`".to_string(),
                    start_byte_offset,
                ))
            }
            Some(':') => {
                self.read_char();
                Err(self.unexpected("unexpected character: :".to_string(), start_byte_offset))
            }
            Some(ch) if ch.is_alphabetic() || ch == '_' => self.read_ident(),
            Some(ch) => {
                self.read_char();
                Err(self.unexpected(format!("unexpected character: {ch}"), start_byte_offset))
            }
        }
    }
}

#[cfg(test)]
/// Drive the lexer to EOF, returning every token (including EOF) in order
/// alongside every lex error message.
fn drain_tokens(input: &str) -> (Vec<Token>, Vec<String>) {
    let mut lexer = Lexer::new(input.to_string());
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    loop {
        match lexer.next_token() {
            Ok(tok) => {
                let is_eof = matches!(tok.token_type, TokenType::Eof);
                tokens.push(tok);
                if is_eof {
                    break;
                }
            }
            Err(e) => errors.push(e.msg),
        }
    }
    (tokens, errors)
}

#[cfg(test)]
fn collect_tokens(input: &str) -> Vec<TokenType> {
    drain_tokens(input)
        .0
        .into_iter()
        .filter(|tok| !matches!(tok.token_type, TokenType::Eof))
        .map(|tok| tok.token_type)
        .collect()
}

#[cfg(test)]
fn extract_errors(input: &str) -> Vec<String> {
    drain_tokens(input).1
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
            (";", TokenType::Semicolon),
            (")", TokenType::RParen),
        ];
        for (input, expected) in cases {
            let mut lexer = Lexer::new(input.to_string());
            assert_eq!(
                lexer.next_token().unwrap().token_type,
                expected,
                "input: {:?}",
                input
            );
        }
    }

    #[test]
    fn test_brackets_are_illegal() {
        // `[` / `]` belonged to the removed bracket-field syntax; they must
        // not lex as accepted tokens.
        let errors = extract_errors("project p [x] { };");
        assert_eq!(errors.len(), 2, "got {:?}", errors);
        assert!(errors.iter().all(|e| e.starts_with("unexpected character")));
    }

    #[test]
    fn test_keywords() {
        let tokens = collect_tokens("import var fn run env log cd switch case default async");
        assert_eq!(
            tokens,
            vec![
                TokenType::Import(None),
                TokenType::Var,
                TokenType::Fn,
                TokenType::Run,
                TokenType::Env,
                TokenType::Log(None),
                TokenType::Cd(None),
                TokenType::Switch(None),
                TokenType::Case(None),
                TokenType::Default,
                TokenType::Async(None),
            ]
        );
    }

    #[test]
    fn test_fused_call_tokens() {
        // `word(` adjacency fuses the template into one call-shaped token;
        // a space breaks the fusion and leaves keyword + template apart.
        let mut lexer = Lexer::new("log(hi);".to_string());
        assert_eq!(
            lexer.next_token().unwrap().token_type,
            TokenType::Log(Some(vec![crate::syntax::source::Template {
                parts: vec![crate::syntax::source::Part::Lit("hi".to_string())],
                offset: 4,
                len: 3,
            }]))
        );
        assert_eq!(lexer.next_token().unwrap().token_type, TokenType::Semicolon);

        let mut lexer = Lexer::new("name();".to_string());
        assert!(matches!(
            lexer.next_token().unwrap().token_type,
            TokenType::Call { ref name, .. } if name == "name"
        ));
    }

    #[test]
    fn test_identifiers() {
        let cases = vec!["todo", "port1", "idx_port", "url", "myVar", "x", "abc123"];
        for ident in cases {
            let mut lexer = Lexer::new(ident.to_string());
            assert_eq!(
                lexer.next_token().unwrap().token_type,
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
            ("($(echo hi))", "echo hi", false),
            ("(@(name))", "name", false),
        ];
        for (input, _, _) in cases {
            let mut lexer = Lexer::new(input.to_string());
            let tok = lexer.next_token().unwrap();
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
        // The name must be followed by `)`; a stray character says so, and
        // running out of input reports the unterminated reference.
        let cases = [
            ("(a @(b c)", "expected `)` after variable name"),
            ("($(echo @(x", "unterminated variable reference"),
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
        let cases = ["(@())", "(a @() b)"];
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
    fn test_empty_command_substitution_is_empty_data() {
        // `$()` runs nothing and substitutes nothing: empty is valid data.
        let cases = ["($())", "($(  ))", "(a $() b)", "($($( )))"];
        for input in cases {
            let errors = extract_errors(input);
            assert!(errors.is_empty(), "input {:?}: got {:?}", input, errors);
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
    fn test_nested_plain_parens_are_data() {
        // Plain `(`/`)` are literal characters in matched pairs; only the
        // depth-zero `)` ends the template.
        let tokens = collect_tokens("(a (b) c)");
        match &tokens[0] {
            TokenType::Template(template) => {
                assert_eq!(
                    template.parts,
                    vec![crate::syntax::source::Part::Lit("a (b) c".to_string())]
                );
            }
            other => panic!("expected template, got {:?}", other),
        }
    }

    #[test]
    fn test_balanced_parens_are_literal_command_text() {
        let tokens = collect_tokens("exec(cd x && (make))");
        match &tokens[0] {
            TokenType::Exec(Some(args)) => match args[0].parts.as_slice() {
                [crate::syntax::source::Part::Lit(command)] => {
                    assert_eq!(command, "cd x && (make)");
                }
                other => panic!("expected one literal command, got {:?}", other),
            },
            other => panic!("expected fused exec, got {:?}", other),
        }
    }

    #[test]
    fn test_parens_shield_semicolons_in_arguments() {
        // A `;` nested in plain parens is data, not an argument separator.
        let tokens = collect_tokens("name(a (x; y);b)");
        match &tokens[0] {
            TokenType::Call { args, .. } => {
                assert_eq!(args.len(), 2, "got {:?}", args);
                assert_eq!(args[0].literal_text(), "a (x; y)");
                assert_eq!(args[1].literal_text(), "b");
            }
            other => panic!("expected call, got {:?}", other),
        }
    }

    /// The hard rule: inside a string-literal paren, whitespace is data.
    #[test]
    fn test_argument_whitespace_is_data() {
        let tokens = collect_tokens("name( a ; b )");
        match &tokens[0] {
            TokenType::Call { args, .. } => {
                assert_eq!(args.len(), 2, "got {:?}", args);
                assert_eq!(args[0].literal_text(), " a ");
                assert_eq!(args[1].literal_text(), " b ");
            }
            other => panic!("expected call, got {:?}", other),
        }
    }

    /// `@()` holds an identifier, not data: whitespace around it is layout.
    #[test]
    fn test_variable_reference_allows_surrounding_whitespace() {
        let tokens = collect_tokens("(@( name ))");
        match &tokens[0] {
            TokenType::Template(template) => {
                assert_eq!(
                    template.parts,
                    vec![crate::syntax::source::Part::Var("name".to_string())]
                );
            }
            other => panic!("expected template, got {:?}", other),
        }
    }

    /// A bare `$()`/`@()` is not a value; every template is parenthesized.
    #[test]
    fn test_bare_substitutions_are_rejected() {
        for input in ["var x = $(cmd);", "var x = @(name);"] {
            let errors = extract_errors(input);
            let expected = if input.contains("$(") {
                "`$(...)` is only valid inside `(...)`"
            } else {
                "`@(...)` is only valid inside `(...)`"
            };
            assert!(
                errors.iter().any(|error| error == expected),
                "input {:?}: got {:?}",
                input,
                errors
            );
        }
    }

    #[test]
    fn test_unbalanced_open_paren_is_unterminated() {
        let errors = extract_errors("(a (b)");
        assert!(
            errors.iter().any(|e| e == "unterminated template"),
            "got {:?}",
            errors
        );
    }

    #[test]
    fn test_variable_names_must_be_identifiers() {
        for input in ["(@(1x))", "(a @(1x) b)", "($(echo @(2y)))"] {
            let errors = extract_errors(input);
            assert!(
                errors
                    .iter()
                    .any(|e| e == "`1x` is not a valid variable name"
                        || e == "`2y` is not a valid variable name"),
                "input {:?}: got {:?}",
                input,
                errors
            );
        }
    }

    #[test]
    fn test_complex_nested_template_still_valid() {
        // Nesting commands and references inside one template must keep working.
        let errors = extract_errors("($(echo @(name))suffix)");
        assert!(errors.is_empty(), "got {:?}", errors);
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
