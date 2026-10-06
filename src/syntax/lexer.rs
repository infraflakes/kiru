//! Turning source text into tokens.

use crate::syntax::Span;

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
            self.skip_trivia();
            let Some((start, current)) = self.current() else {
                tokens.push(Token {
                    kind: TokenKind::Eof,
                    span: Span::new(self.source.len(), self.source.len()),
                });
                return Ok(tokens);
            };

            if current == '"' {
                tokens.push(self.text()?);
                continue;
            }

            if is_identifier_start(current) {
                tokens.push(self.identifier());
                continue;
            }

            let single_character_token = match current {
                '.' => Some(TokenKind::Dot),
                ',' => Some(TokenKind::Comma),
                ';' => Some(TokenKind::Semi),
                '=' => Some(TokenKind::Equals),
                '+' => Some(TokenKind::Plus),
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
                tokens.push(Token {
                    kind,
                    span: Span::new(start, self.offset()),
                });
                continue;
            }

            if current == '-' {
                if let Some((_, '>')) = self.peek(1) {
                    self.advance();
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Arrow,
                        span: Span::new(start, self.offset()),
                    });
                    continue;
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
                    tokens.push(Token {
                        kind: TokenKind::PathSep,
                        span: Span::new(start, self.offset()),
                    });
                    continue;
                }
                return Err(self.error(
                    Span::new(start, start + current.len_utf8()),
                    "expected `::` in a path",
                ));
            }

            return Err(self.error(
                Span::new(start, start + current.len_utf8()),
                format!("unexpected character `{current}`"),
            ));
        }
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
            "module" => TokenKind::Module,
            "import" => TokenKind::Import,
            "fn" => TokenKind::Fn,
            "txt" => TokenKind::Txt,
            "rec" => TokenKind::Rec,
            "list" => TokenKind::List,
            "mut" => TokenKind::Mut,
            "switch" => TokenKind::Switch,
            "case" => TokenKind::Case,
            "default" => TokenKind::Default,
            "return" => TokenKind::Return,
            "panic" => TokenKind::Panic,
            "async" => TokenKind::Async,
            "wait" => TokenKind::Wait,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "break" => TokenKind::Break,
            _ => TokenKind::Ident(text.to_owned()),
        };
        Token {
            kind,
            span: Span::new(start, self.offset()),
        }
    }

    fn text(&mut self) -> Result<Token, LexError> {
        let (start, _) = self.current().expect("string starts at a quote");
        self.advance();
        let mut value = String::new();
        loop {
            let Some((position, character)) = self.current() else {
                return Err(self.error(Span::new(start, self.source.len()), "unterminated string"));
            };
            match character {
                '"' => {
                    self.advance();
                    return Ok(Token {
                        kind: TokenKind::Text(value),
                        span: Span::new(start, self.offset()),
                    });
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
                        _ => {
                            return Err(self.error(
                                Span::new(escape_start, self.offset() + escape.len_utf8()),
                                format!(
                                    "invalid escape `\\{escape}`; only \\n, \\t, \\r, \\e, \\\\, and \\\" are allowed"
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
            kinds("fn build(rec repo) { return(repo.dir); };"),
            vec![
                TokenKind::Fn,
                TokenKind::Ident("build".to_owned()),
                TokenKind::LParen,
                TokenKind::Rec,
                TokenKind::Ident("repo".to_owned()),
                TokenKind::RParen,
                TokenKind::LBrace,
                TokenKind::Return,
                TokenKind::LParen,
                TokenKind::Ident("repo".to_owned()),
                TokenKind::Dot,
                TokenKind::Ident("dir".to_owned()),
                TokenKind::RParen,
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
            kinds("fn f() -> txt { return(\"x\"); };"),
            vec![
                TokenKind::Fn,
                TokenKind::Ident("f".to_owned()),
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::Arrow,
                TokenKind::Txt,
                TokenKind::LBrace,
                TokenKind::Return,
                TokenKind::LParen,
                TokenKind::Text("x".to_owned()),
                TokenKind::RParen,
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
                "invalid escape `\\q`; only \\n, \\t, \\r, \\e, \\\\, and \\\" are allowed",
            ),
            ("unterminated string", "\"open", "unterminated string"),
            ("lone colon", "a : b", "expected `::` in a path"),
            ("lone dash", "a - b", "expected `->`"),
            ("unknown character", "a @ b", "unexpected character `@`"),
        ]);
    }
}
