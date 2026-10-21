//! Turning source text into tokens.

use std::str::CharIndices;

use crate::syntax::Span;

use super::token::{Token, TokenKind, WILDCARD};

/// A lexical error, positioned at the offending source range.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LexError {
    pub(crate) span: Span,
    pub(crate) message: String,
}

/// Lex a whole source file.
pub(crate) fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let mut lexer = Lexer::new(source);
    let mut tokens = Vec::new();
    lexer.run(&mut tokens)?;
    Ok(tokens)
}

struct Lexer<'a> {
    source: &'a str,
    /// A cursor over the source's characters, walked in place: no character
    /// table is materialized. The cursor sits on the next unread character
    /// and the one at the cursor is cached, so the reads are plain field
    /// reads and a lookahead copies the cursor over the borrowed slice.
    cursor: CharIndices<'a>,
    at: Option<(usize, char)>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        let mut cursor = source.char_indices();
        let at = cursor.next();
        Self { source, cursor, at }
    }

    fn current(&self) -> Option<(usize, char)> {
        self.at
    }

    /// The character `ahead` positions past the cursor. The cursor's clone
    /// walks over the same borrowed slice, so this allocates nothing.
    fn peek(&self, ahead: usize) -> Option<(usize, char)> {
        match ahead {
            0 => self.at,
            ahead => self.cursor.clone().nth(ahead - 1),
        }
    }

    fn offset(&self) -> usize {
        self.current()
            .map(|(offset, _)| offset)
            .unwrap_or(self.source.len())
    }

    fn advance(&mut self) -> Option<(usize, char)> {
        let current = self.at;
        self.at = self.cursor.next();
        current
    }

    fn error(&self, span: Span, message: impl Into<String>) -> LexError {
        LexError {
            span,
            message: message.into(),
        }
    }

    /// Append every token of the source to `tokens`, ending with `Eof`. The
    /// buffer is the caller's, so one token allocates no vector of its own.
    fn run(&mut self, tokens: &mut Vec<Token>) -> Result<(), LexError> {
        loop {
            let before = tokens.len();
            self.tokens_at_cursor(tokens)?;
            if tokens.len() == before {
                tokens.push(Token {
                    kind: TokenKind::Eof,
                    span: Span::new(self.source.len(), self.source.len()),
                });
                return Ok(());
            }
        }
    }

    /// Append the tokens at the cursor, skipping leading trivia. Usually one
    /// token, but a string with interpolation is several. Appends nothing at
    /// the end of the source.
    fn tokens_at_cursor(&mut self, tokens: &mut Vec<Token>) -> Result<(), LexError> {
        self.skip_trivia();
        let Some((start, current)) = self.current() else {
            return Ok(());
        };

        if current == '"' {
            return self.text(tokens);
        }

        if current.is_ascii_digit() {
            tokens.push(self.number()?);
            return Ok(());
        }

        if is_identifier_start(current) {
            tokens.push(self.identifier());
            return Ok(());
        }

        // A two-character operator comes first, so `==` never lexes as two `=`.
        let next = self.peek(1).map(|(_, next)| next);
        let two_character_token = match (current, next) {
            ('=', Some('>')) => Some(TokenKind::FatArrow),
            ('=', Some('=')) => Some(TokenKind::EqEq),
            ('!', Some('=')) => Some(TokenKind::NotEq),
            ('<', Some('=')) => Some(TokenKind::LessEq),
            ('>', Some('=')) => Some(TokenKind::GreaterEq),
            ('&', Some('&')) => Some(TokenKind::AndAnd),
            ('|', Some('|')) => Some(TokenKind::OrOr),
            ('-', Some('>')) => Some(TokenKind::Arrow),
            (':', Some(':')) => Some(TokenKind::PathSep),
            _ => None,
        };
        if let Some(kind) = two_character_token {
            self.advance();
            self.advance();
            tokens.push(Token {
                kind,
                span: Span::new(start, self.offset()),
            });
            return Ok(());
        }

        let single_character_token = match current {
            '.' => Some(TokenKind::Dot),
            ',' => Some(TokenKind::Comma),
            ';' => Some(TokenKind::Semi),
            '+' => Some(TokenKind::Plus),
            '-' => Some(TokenKind::Minus),
            '*' => Some(TokenKind::Star),
            '/' => Some(TokenKind::Slash),
            '%' => Some(TokenKind::Percent),
            '!' => Some(TokenKind::Bang),
            '<' => Some(TokenKind::Less),
            '>' => Some(TokenKind::Greater),
            '=' => Some(TokenKind::Equals),
            ':' => Some(TokenKind::Colon),
            '{' => Some(TokenKind::LBrace),
            '}' => Some(TokenKind::RBrace),
            '(' => Some(TokenKind::LParen),
            ')' => Some(TokenKind::RParen),
            _ => None,
        };
        if let Some(kind) = single_character_token {
            self.advance();
            tokens.push(Token {
                kind,
                span: Span::new(start, self.offset()),
            });
            return Ok(());
        }

        // A lone `&` or `|` is a prefix of a two-character operator.
        if let '&' | '|' = current {
            let message = match current {
                '&' => "expected `&&`".to_owned(),
                _ => "expected `||`".to_owned(),
            };
            return Err(self.error(Span::new(start, start + current.len_utf8()), message));
        }

        Err(self.error(
            Span::new(start, start + current.len_utf8()),
            format!("unexpected character `{current}`"),
        ))
    }

    /// Lex an integer literal. The sign is a separate unary operator, so only
    /// digits are read here.
    fn number(&mut self) -> Result<Token, LexError> {
        let (start, _) = self.current().expect("a number starts at a digit");
        while let Some((_, character)) = self.current() {
            if !character.is_ascii_digit() {
                break;
            }
            self.advance();
        }
        let span = Span::new(start, self.offset());
        let text = &self.source[start..self.offset()];
        let value = text
            .parse::<i64>()
            .map_err(|_| self.error(span, format!("integer `{text}` is too large")))?;
        Ok(Token {
            kind: TokenKind::Int(value),
            span,
        })
    }

    /// Skip whitespace and comments: a comment is trivia the parser never
    /// sees.
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
            WILDCARD => TokenKind::Wildcard,
            "use" => TokenKind::Use,
            "pub" => TokenKind::Pub,
            "fn" => TokenKind::Fn,
            "impl" => TokenKind::Impl,
            "enum" => TokenKind::Enum,
            "let" => TokenKind::Let,
            "rec" => TokenKind::Rec,
            "match" => TokenKind::Match,
            "return" => TokenKind::Return,
            "panic" => TokenKind::Panic,
            "async" => TokenKind::Async,
            "wait" => TokenKind::Wait,
            "for" => TokenKind::For,
            "break" => TokenKind::Break,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            _ => TokenKind::Ident,
        };
        Token {
            kind,
            span: Span::new(start, self.offset()),
        }
    }

    /// Lex a string into `tokens`. A string with no interpolation is one
    /// `Text` token; a string with `@(…)` is a `Text` segment, then an
    /// `InterpolationStart`, the tokens of the expression, and an
    /// `InterpolationEnd`, repeated.
    fn text(&mut self, tokens: &mut Vec<Token>) -> Result<(), LexError> {
        let (start, _) = self.current().expect("string starts at a quote");
        self.advance();
        let mut value = String::new();
        // The first segment's span opens at the quote, so a literal's written
        // span covers its delimiters and a later segment's opening
        // interpolation does the same for the closing one.
        let mut segment_start = start;
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
                    return Ok(());
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
                    self.interpolation(tokens)?;
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
            let before = tokens.len();
            self.tokens_at_cursor(tokens)?;
            if tokens.len() == before {
                return Err(self.error(
                    Span::new(start, self.source.len()),
                    "unterminated interpolation",
                ));
            }
            let mut index = before;
            while index < tokens.len() {
                match tokens[index].kind {
                    TokenKind::LParen => depth += 1,
                    TokenKind::RParen => {
                        if depth == 0 {
                            // The closing parenthesis is not part of the
                            // interpolation: the boundary token takes its span.
                            let span = tokens[index].span;
                            tokens.truncate(index);
                            tokens.push(Token {
                                kind: TokenKind::InterpolationEnd,
                                span,
                            });
                            return Ok(());
                        }
                        depth -= 1;
                    }
                    _ => {}
                }
                index += 1;
            }
        }
    }
}

fn is_identifier_start(character: char) -> bool {
    character == '_' || character.is_alphabetic()
}

/// Whether a character may continue an identifier, after its first.
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
            kinds("fn build(repo: txt) { return repo.dir; };"),
            vec![
                TokenKind::Fn,
                TokenKind::Ident,
                TokenKind::LParen,
                TokenKind::Ident,
                TokenKind::Colon,
                TokenKind::Ident,
                TokenKind::RParen,
                TokenKind::LBrace,
                TokenKind::Return,
                TokenKind::Ident,
                TokenKind::Dot,
                TokenKind::Ident,
                TokenKind::Semi,
                TokenKind::RBrace,
                TokenKind::Semi,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_the_impl_keyword() {
        assert_eq!(
            kinds("impl Vec {};"),
            vec![
                TokenKind::Impl,
                TokenKind::Ident,
                TokenKind::LBrace,
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
                TokenKind::Ident,
                TokenKind::InterpolationEnd,
                TokenKind::Text("b".to_owned()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_a_let_binding_and_type_annotation() {
        assert_eq!(
            kinds("let x: txt = \"a\";"),
            vec![
                TokenKind::Let,
                TokenKind::Ident,
                TokenKind::Colon,
                TokenKind::Ident,
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
                TokenKind::Ident,
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
                TokenKind::Ident,
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::Arrow,
                TokenKind::Ident,
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
                TokenKind::Ident,
                TokenKind::PathSep,
                TokenKind::Ident,
                TokenKind::LParen,
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
            ("lone ampersand", "a & b", "expected `&&`"),
            ("lone pipe", "a | b", "expected `||`"),
            ("unknown character", "a @ b", "unexpected character `@`"),
        ]);
    }

    #[test]
    fn lexes_operators_and_literals() {
        assert_eq!(
            kinds("true false 42 x + y == z && !w"),
            vec![
                TokenKind::True,
                TokenKind::False,
                TokenKind::Int(42),
                TokenKind::Ident,
                TokenKind::Plus,
                TokenKind::Ident,
                TokenKind::EqEq,
                TokenKind::Ident,
                TokenKind::AndAnd,
                TokenKind::Bang,
                TokenKind::Ident,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_a_colon_and_a_path_separator() {
        assert_eq!(
            kinds("let x: txt = std::run();"),
            vec![
                TokenKind::Let,
                TokenKind::Ident,
                TokenKind::Colon,
                TokenKind::Ident,
                TokenKind::Equals,
                TokenKind::Ident,
                TokenKind::PathSep,
                TokenKind::Ident,
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::Semi,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_comparison_and_arrow_operators() {
        assert_eq!(
            kinds("a<=b >= c != d - e -> f"),
            vec![
                TokenKind::Ident,
                TokenKind::LessEq,
                TokenKind::Ident,
                TokenKind::GreaterEq,
                TokenKind::Ident,
                TokenKind::NotEq,
                TokenKind::Ident,
                TokenKind::Minus,
                TokenKind::Ident,
                TokenKind::Arrow,
                TokenKind::Ident,
                TokenKind::Eof,
            ]
        );
    }
}
