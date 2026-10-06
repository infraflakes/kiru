//! Turning source text into tokens.

use crate::syntax::Span;
use crate::types::Type;

use super::token::{Token, TokenKind};

/// A lexical error, positioned at the offending source range.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LexError {
    pub(crate) span: Span,
    pub(crate) message: String,
}

/// Lex a whole source file.
pub(crate) fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    Lexer::new(source).run()
}

struct Lexer<'a> {
    source: &'a str,
    chars: Vec<(usize, char)>,
    position: usize,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            chars: source.char_indices().collect(),
            position: 0,
        }
    }

    fn current(&self) -> Option<(usize, char)> {
        self.chars.get(self.position).copied()
    }

    fn peek(&self, ahead: usize) -> Option<(usize, char)> {
        self.chars.get(self.position + ahead).copied()
    }

    fn offset(&self) -> usize {
        self.current()
            .map(|(offset, _)| offset)
            .unwrap_or(self.source.len())
    }

    fn advance(&mut self) -> Option<(usize, char)> {
        let current = self.current();
        if current.is_some() {
            self.position += 1;
        }
        current
    }

    fn error(&self, span: Span, message: impl Into<String>) -> LexError {
        LexError {
            span,
            message: message.into(),
        }
    }

    fn run(mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        loop {
            let next = self.next_tokens()?;
            if next.is_empty() {
                tokens.push(Token {
                    kind: TokenKind::Eof,
                    span: Span::new(self.source.len(), self.source.len()),
                });
                return Ok(tokens);
            }
            tokens.extend(next);
        }
    }

    /// Lex the tokens at the cursor, skipping leading trivia. Usually one
    /// token, but a string with interpolation is several. Empty at the end of
    /// the source.
    fn next_tokens(&mut self) -> Result<Vec<Token>, LexError> {
        self.skip_trivia();
        let Some((start, current)) = self.current() else {
            return Ok(Vec::new());
        };

        if current == '"' {
            return self.text();
        }

        if is_identifier_start(current) {
            return Ok(vec![self.identifier()]);
        }

        let single_character_token = match current {
            '.' => Some(TokenKind::Dot),
            ',' => Some(TokenKind::Comma),
            ';' => Some(TokenKind::Semi),
            '+' => Some(TokenKind::Plus),
            '<' => Some(TokenKind::Less),
            '>' => Some(TokenKind::Greater),
            '{' => Some(TokenKind::LBrace),
            '}' => Some(TokenKind::RBrace),
            '(' => Some(TokenKind::LParen),
            ')' => Some(TokenKind::RParen),
            '[' => Some(TokenKind::LBracket),
            ']' => Some(TokenKind::RBracket),
            _ => None,
        };
        if let Some(kind) = single_character_token {
            self.advance();
            return Ok(vec![Token {
                kind,
                span: Span::new(start, self.offset()),
            }]);
        }

        if current == '=' {
            if let Some((_, '>')) = self.peek(1) {
                self.advance();
                self.advance();
                return Ok(vec![Token {
                    kind: TokenKind::FatArrow,
                    span: Span::new(start, self.offset()),
                }]);
            }
            self.advance();
            return Ok(vec![Token {
                kind: TokenKind::Equals,
                span: Span::new(start, self.offset()),
            }]);
        }

        if current == '-' {
            if let Some((_, '>')) = self.peek(1) {
                self.advance();
                self.advance();
                return Ok(vec![Token {
                    kind: TokenKind::Arrow,
                    span: Span::new(start, self.offset()),
                }]);
            }
            return Err(self.error(
                Span::new(start, start + current.len_utf8()),
                "expected `->`",
            ));
        }

        if current == ':' {
            if let Some((_, ':')) = self.peek(1) {
                self.advance();
                self.advance();
                return Ok(vec![Token {
                    kind: TokenKind::PathSep,
                    span: Span::new(start, self.offset()),
                }]);
            }
            return Err(self.error(
                Span::new(start, start + current.len_utf8()),
                "expected `::` in a path",
            ));
        }

        Err(self.error(
            Span::new(start, start + current.len_utf8()),
            format!("unexpected character `{current}`"),
        ))
    }

    fn skip_trivia(&mut self) {
        loop {
            match self.current() {
                Some((_, ' ' | '\t' | '\r' | '\n')) => {
                    self.advance();
                }
                Some((_, '#')) => {
                    while let Some((_, character)) = self.current() {
                        if character == '\n' {
                            break;
                        }
                        self.advance();
                    }
                }
                _ => return,
            }
        }
    }

    fn identifier(&mut self) -> Token {
        let (start, _) = self.current().expect("identifier starts at a character");
        while let Some((_, character)) = self.current() {
            if !is_identifier_continue(character) {
                break;
            }
            self.advance();
        }
        let text = &self.source[start..self.offset()];
        let kind = match text {
            "mod" => TokenKind::Mod,
            "import" => TokenKind::Import,
            "fn" => TokenKind::Fn,
            "let" => TokenKind::Let,
            "mut" => TokenKind::Mut,
            "match" => TokenKind::Match,
            "return" => TokenKind::Return,
            "panic" => TokenKind::Panic,
            "async" => TokenKind::Async,
            "wait" => TokenKind::Wait,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "break" => TokenKind::Break,
            _ => match Type::from_keyword(text) {
                Some(ty) => TokenKind::Type(ty),
                None => TokenKind::Ident(text.to_owned()),
            },
        };
        Token {
            kind,
            span: Span::new(start, self.offset()),
        }
    }

    /// Lex a string. A string with no interpolation is one `Text` token; a
    /// string with `@(…)` is a `Text` segment, then an `InterpolationStart`,
    /// the tokens of the expression, and an `InterpolationEnd`, repeated.
    fn text(&mut self) -> Result<Vec<Token>, LexError> {
        let (start, _) = self.current().expect("string starts at a quote");
        self.advance();
        let mut tokens = Vec::new();
        let mut value = String::new();
        let mut segment_start = self.offset();
        loop {
            let Some((position, character)) = self.current() else {
                return Err(self.error(Span::new(start, self.source.len()), "unterminated string"));
            };
            match character {
                '"' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Text(value),
                        span: Span::new(segment_start, self.offset()),
                    });
                    return Ok(tokens);
                }
                '@' if matches!(self.peek(1), Some((_, '('))) => {
                    tokens.push(Token {
                        kind: TokenKind::Text(std::mem::take(&mut value)),
                        span: Span::new(segment_start, self.offset()),
                    });
                    let interpolation_start = self.offset();
                    self.advance();
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::InterpolationStart,
                        span: Span::new(interpolation_start, self.offset()),
                    });
                    self.interpolation(&mut tokens)?;
                    segment_start = self.offset();
                }
                '\\' => {
                    let escape_start = position;
                    self.advance();
                    let Some((_, escape)) = self.current() else {
                        return Err(
                            self.error(Span::new(start, self.source.len()), "unterminated string")
                        );
                    };
                    let decoded = match escape {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        'e' => '\u{1b}',
                        '\\' => '\\',
                        '"' => '"',
                        '@' => '@',
                        _ => {
                            return Err(self.error(
                                Span::new(escape_start, self.offset() + escape.len_utf8()),
                                format!(
                                    "invalid escape `\\{escape}`; only \\n, \\t, \\r, \\e, \\\\, \\\", and \\@ are allowed"
                                ),
                            ));
                        }
                    };
                    value.push(decoded);
                    self.advance();
                }
                _ => {
                    value.push(character);
                    self.advance();
                }
            }
        }
    }

    /// Lex the tokens of an interpolation, up to its matching `)`, appending
    /// them and the closing `InterpolationEnd` to `tokens`.
    fn interpolation(&mut self, tokens: &mut Vec<Token>) -> Result<(), LexError> {
        let mut depth = 0usize;
        loop {
            let start = self.offset();
            let next = self.next_tokens()?;
            if next.is_empty() {
                return Err(self.error(
                    Span::new(start, self.source.len()),
                    "unterminated interpolation",
                ));
            }
            for token in next {
                match token.kind {
                    TokenKind::LParen => depth += 1,
                    TokenKind::RParen => {
                        if depth == 0 {
                            tokens.push(Token {
                                kind: TokenKind::InterpolationEnd,
                                span: token.span,
                            });
                            return Ok(());
                        }
                        depth -= 1;
                    }
                    _ => {}
                }
                tokens.push(token);
            }
        }
    }
}

fn is_identifier_start(character: char) -> bool {
    character == '_' || character.is_alphabetic()
}

fn is_identifier_continue(character: char) -> bool {
    character == '_' || character.is_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        lex(source)
            .expect("source lexes")
            .into_iter()
            .map(|token| token.kind)
            .collect()
    }

    #[test]
    fn lexes_keywords_identifiers_and_punctuation() {
        assert_eq!(
            kinds("fn build(repo<rec>) { return repo.dir; };"),
            vec![
                TokenKind::Fn,
                TokenKind::Ident("build".to_owned()),
                TokenKind::LParen,
                TokenKind::Ident("repo".to_owned()),
                TokenKind::Less,
                TokenKind::Type(Type::Record),
                TokenKind::Greater,
                TokenKind::RParen,
                TokenKind::LBrace,
                TokenKind::Return,
                TokenKind::Ident("repo".to_owned()),
                TokenKind::Dot,
                TokenKind::Ident("dir".to_owned()),
                TokenKind::Semi,
                TokenKind::RBrace,
                TokenKind::Semi,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_an_interpolated_string() {
        assert_eq!(
            kinds("\"a@(x)b\""),
            vec![
                TokenKind::Text("a".to_owned()),
                TokenKind::InterpolationStart,
                TokenKind::Ident("x".to_owned()),
                TokenKind::InterpolationEnd,
                TokenKind::Text("b".to_owned()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_a_let_binding_and_type_annotation() {
        assert_eq!(
            kinds("let x<txt> = \"a\";"),
            vec![
                TokenKind::Let,
                TokenKind::Ident("x".to_owned()),
                TokenKind::Less,
                TokenKind::Type(Type::Text),
                TokenKind::Greater,
                TokenKind::Equals,
                TokenKind::Text("a".to_owned()),
                TokenKind::Semi,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_a_match_arm_fat_arrow() {
        assert_eq!(
            kinds("match s { \"a\" => {}; };"),
            vec![
                TokenKind::Match,
                TokenKind::Ident("s".to_owned()),
                TokenKind::LBrace,
                TokenKind::Text("a".to_owned()),
                TokenKind::FatArrow,
                TokenKind::LBrace,
                TokenKind::RBrace,
                TokenKind::Semi,
                TokenKind::RBrace,
                TokenKind::Semi,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_a_return_arrow() {
        assert_eq!(
            kinds("fn f() -> txt { return \"x\"; };"),
            vec![
                TokenKind::Fn,
                TokenKind::Ident("f".to_owned()),
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::Arrow,
                TokenKind::Type(Type::Text),
                TokenKind::LBrace,
                TokenKind::Return,
                TokenKind::Text("x".to_owned()),
                TokenKind::Semi,
                TokenKind::RBrace,
                TokenKind::Semi,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_paths_and_comments() {
        assert_eq!(
            kinds("std::run(); # done"),
            vec![
                TokenKind::Ident("std".to_owned()),
                TokenKind::PathSep,
                TokenKind::Ident("run".to_owned()),
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::Semi,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_a_leading_path_separator() {
        assert_eq!(
            kinds("::std::print(\"x\");"),
            vec![
                TokenKind::PathSep,
                TokenKind::Ident("std".to_owned()),
                TokenKind::PathSep,
                TokenKind::Ident("print".to_owned()),
                TokenKind::LParen,
                TokenKind::Text("x".to_owned()),
                TokenKind::RParen,
                TokenKind::Semi,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn decodes_string_escapes() {
        assert_eq!(
            kinds(r#""a\n\t\r\e\\\"""#),
            vec![
                TokenKind::Text("a\n\t\r\u{1b}\\\"".to_owned()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn strings_span_lines() {
        assert_eq!(
            kinds("\"one\ntwo\""),
            vec![TokenKind::Text("one\ntwo".to_owned()), TokenKind::Eof]
        );
    }

    /// Run every rejection case against its expected message, collecting all
    /// mismatches so one run reports every case that failed.
    fn expect_rejections(cases: &[(&str, &str, &str)]) {
        let mut failures = Vec::new();
        for &(name, source, expected) in cases {
            let error = lex(source).expect_err("source is rejected");
            if error.message != expected {
                failures.push(format!(
                    "{name}: expected `{expected}`, found `{}`",
                    error.message
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "{} rejection case(s) failed:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn lexer_errors() {
        expect_rejections(&[
            (
                "invalid escape",
                r#""bad \q""#,
                "invalid escape `\\q`; only \\n, \\t, \\r, \\e, \\\\, \\\", and \\@ are allowed",
            ),
            ("unterminated string", "\"open", "unterminated string"),
            ("lone colon", "a : b", "expected `::` in a path"),
            ("lone dash", "a - b", "expected `->`"),
            ("unknown character", "a @ b", "unexpected character `@`"),
        ]);
    }
}
