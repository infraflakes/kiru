use super::Lexer;
use crate::dsl::token::{Token, TokenType, lookup_ident};

impl Lexer {
    pub(super) fn read_char(&mut self) {
        if let Some(c) = self.ch {
            self.byte_offset += c.len_utf8();
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
        while let Some(c) = self.ch {
            if !c.is_whitespace() {
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

        while let Some(c) = self.ch {
            if c.is_alphanumeric() || c == '_' {
                self.read_char();
            } else {
                break;
            }
        }

        let ident: String = self.input[start_pos..self.pos].iter().collect();
        let ty = lookup_ident(&ident);
        Token::new(
            ty,
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

        self.read_char(); // skip opening backtick
        while let Some(c) = self.ch {
            if c == '`' {
                break;
            }
            if c == '\n' {
                // Unterminated backtick string - stop at newline
                break;
            }
            self.read_char();
        }

        let content: String = self.input[start_pos + 1..self.pos].iter().collect();

        if self.ch == Some('`') {
            self.read_char(); // skip closing backtick
            Token::new(
                TokenType::Backtick(content),
                start_line,
                start_col,
                start_byte_offset,
                self.byte_offset - start_byte_offset,
            )
        } else {
            // Unterminated backtick string
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
