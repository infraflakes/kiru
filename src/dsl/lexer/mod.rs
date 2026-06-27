use crate::dsl::token::{Token, TokenType};

mod tokenizer;

/// Character-level lexer that emits tokens from source text.
#[derive(Debug)]
pub(crate) struct Lexer {
    pub(super) input: Vec<char>,
    pub(super) pos: usize,
    pub(super) read_pos: usize,
    pub(super) ch: Option<char>,
    pub(super) line: usize,
    pub(super) col: usize,
    pub(super) byte_offset: usize,
}

impl Lexer {
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

    pub(crate) fn source_len(&self) -> usize {
        self.input.iter().map(|ch| ch.len_utf8()).sum()
    }

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
                self.read_char();
                Token::new(
                    TokenType::LBrace,
                    start_line,
                    start_col,
                    start_byte_offset,
                    self.byte_offset - start_byte_offset,
                )
            }
            Some('}') => {
                self.read_char();
                Token::new(
                    TokenType::RBrace,
                    start_line,
                    start_col,
                    start_byte_offset,
                    self.byte_offset - start_byte_offset,
                )
            }
            Some('[') => {
                self.read_char();
                Token::new(
                    TokenType::LBracket,
                    start_line,
                    start_col,
                    start_byte_offset,
                    self.byte_offset - start_byte_offset,
                )
            }
            Some(']') => {
                self.read_char();
                Token::new(
                    TokenType::RBracket,
                    start_line,
                    start_col,
                    start_byte_offset,
                    self.byte_offset - start_byte_offset,
                )
            }
            Some(ch @ ('(' | ')')) => {
                self.read_char();
                Token::new(
                    TokenType::Illegal(format!("unexpected character: {}", ch)),
                    start_line,
                    start_col,
                    start_byte_offset,
                    self.byte_offset - start_byte_offset,
                )
            }
            Some(',') => {
                self.read_char();
                Token::new(
                    TokenType::Comma,
                    start_line,
                    start_col,
                    start_byte_offset,
                    self.byte_offset - start_byte_offset,
                )
            }
            Some('.') => {
                self.read_char();
                Token::new(
                    TokenType::Illegal("unexpected character: .".to_string()),
                    start_line,
                    start_col,
                    start_byte_offset,
                    self.byte_offset - start_byte_offset,
                )
            }
            Some(';') => {
                self.read_char();
                Token::new(
                    TokenType::Semicolon,
                    start_line,
                    start_col,
                    start_byte_offset,
                    self.byte_offset - start_byte_offset,
                )
            }
            Some('$') => {
                self.read_char();
                Token::new(
                    TokenType::Dollar,
                    start_line,
                    start_col,
                    start_byte_offset,
                    self.byte_offset - start_byte_offset,
                )
            }
            Some('=') => {
                self.read_char();
                if self.ch == Some('>') {
                    self.read_char();
                    Token::new(
                        TokenType::Arrow,
                        start_line,
                        start_col,
                        start_byte_offset,
                        self.byte_offset - start_byte_offset,
                    )
                } else {
                    Token::new(
                        TokenType::Assign,
                        start_line,
                        start_col,
                        start_byte_offset,
                        self.byte_offset - start_byte_offset,
                    )
                }
            }
            Some('`') => self.read_backtick(),
            Some(ch) if ch.is_alphabetic() || ch == '_' => self.read_ident(),
            Some(ch) => {
                self.read_char();
                Token::new(
                    TokenType::Illegal(format!("unexpected character: {}", ch)),
                    start_line,
                    start_col,
                    start_byte_offset,
                    self.byte_offset - start_byte_offset,
                )
            }
        }
    }
}

#[cfg(test)]
fn collect_tokens(input: &str) -> Vec<TokenType> {
    let mut lexer = Lexer::new(input.to_string());
    let mut tokens = Vec::new();
    loop {
        let tok = lexer.next_token();
        let is_eof = matches!(tok.ty, TokenType::Eof);
        if !matches!(tok.ty, TokenType::Eof | TokenType::Illegal(_)) {
            tokens.push(tok.ty);
        }
        if is_eof {
            break;
        }
    }
    tokens
}

#[cfg(test)]
fn collect_all_tokens(input: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(input.to_string());
    let mut tokens = Vec::new();
    loop {
        let tok = lexer.next_token();
        let is_eof = matches!(tok.ty, TokenType::Eof);
        tokens.push(tok);
        if is_eof {
            break;
        }
    }
    tokens
}

#[cfg(test)]
fn extract_errors(input: &str) -> Vec<String> {
    let mut lexer = Lexer::new(input.to_string());
    let mut errors = Vec::new();
    loop {
        let tok = lexer.next_token();
        match tok.ty {
            TokenType::Eof => break,
            TokenType::Illegal(msg) => errors.push(msg),
            _ => {}
        }
    }
    errors
}

#[cfg(test)]
mod tests;
