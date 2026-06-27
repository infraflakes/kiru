use super::Lexer;
use crate::dsl::token::{Token, TokenType, lookup_ident};

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

    pub(super) fn read_backtick(&mut self) -> Token {
        let start_line = self.line;
        let start_col = self.col;
        let start_pos = self.pos;
        let start_byte_offset = self.byte_offset;

        self.read_char();
        while let Some(ch) = self.ch {
            if ch == '`' {
                break;
            }
            if ch == '\n' {
                break;
            }
            self.read_char();
        }

        let content: String = self.input[start_pos + 1..self.pos].iter().collect();

        if self.ch == Some('`') {
            self.read_char();
            Token::new(
                TokenType::Backtick(content),
                start_line,
                start_col,
                start_byte_offset,
                self.byte_offset - start_byte_offset,
            )
        } else {
            Token::new(
                TokenType::Illegal("unterminated backtick string".to_string()),
                start_line,
                start_col,
                start_byte_offset,
                self.byte_offset - start_byte_offset,
            )
        }
    }
}
