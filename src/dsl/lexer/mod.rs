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
            Some(':') => {
                self.read_char();
                if self.ch == Some(':') {
                    self.read_char();
                    Token::new(
                        TokenType::NamespaceSep,
                        start_line,
                        start_col,
                        start_byte_offset,
                        self.byte_offset - start_byte_offset,
                    )
                } else {
                    Token::new(
                        TokenType::Illegal("unexpected character: :".to_string()),
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
            ("$", TokenType::Dollar),
        ];
        for (input, expected) in cases {
            let mut lexer = Lexer::new(input.to_string());
            assert_eq!(lexer.next_token().ty, expected, "input: {:?}", input);
        }
    }

    #[test]
    fn test_keywords() {
        let tokens = collect_tokens("import var string pr fn run env log exec cd shell");
        assert_eq!(
            tokens,
            vec![
                TokenType::Import,
                TokenType::Var,
                TokenType::StringKw,
                TokenType::Pr,
                TokenType::Fn,
                TokenType::Run,
                TokenType::Env,
                TokenType::Log,
                TokenType::Exec,
                TokenType::Cd,
                TokenType::Shell,
            ]
        );
    }

    #[test]
    fn test_identifiers() {
        let cases = vec!["todo", "port1", "idx_port", "url", "myVar", "x", "abc123"];
        for ident in cases {
            let mut lexer = Lexer::new(ident.to_string());
            assert_eq!(
                lexer.next_token().ty,
                TokenType::Ident(ident.to_string()),
                "ident: {:?}",
                ident
            );
        }
    }

    #[test]
    fn test_backtick_literals() {
        let cases = vec![
            ("`echo hello`", "echo hello", false),
            ("``", "", false),
            ("`hello ${name}`", "hello ${name}", false),
            ("`line1\nline2`", "unterminated backtick string", true),
        ];
        for (input, expected, is_error) in cases {
            let mut lexer = Lexer::new(input.to_string());
            let tok = lexer.next_token();
            if is_error {
                assert!(
                    matches!(&tok.ty, TokenType::Illegal(msg) if msg == expected),
                    "input: {:?}, expected error {:?}, got {:?}",
                    input,
                    expected,
                    tok.ty
                );
            } else {
                assert_eq!(
                    tok.ty,
                    TokenType::Backtick(expected.to_string()),
                    "input: {:?}",
                    input
                );
            }
        }
    }

    #[test]
    fn test_path_literals() {
        let cases = vec![
            "./file.kiru",
            "./path/to/file.kiru",
            "../file.kiru",
            "../../dir/file.kiru",
            "./a",
        ];
        for input in cases {
            let mut lexer = Lexer::new(input.to_string());
            let tok = lexer.next_token();
            assert!(
                matches!(&tok.ty, TokenType::Illegal(msg) if msg == "unexpected character: ."),
                "input: {:?}, expected illegal, got {:?}",
                input,
                tok.ty
            );
        }
    }

    #[test]
    fn test_dot_and_dotdot_are_not_paths() {
        let errors = extract_errors(".");
        assert!(errors.iter().any(|e| e == "unexpected character: ."));

        let errors = extract_errors("..");
        assert_eq!(errors.len(), 2);
        assert!(errors.iter().all(|e| e == "unexpected character: ."));

        let mut lexer = Lexer::new(".../".to_string());
        let first = lexer.next_token();
        assert!(matches!(first.ty, TokenType::Illegal(_)));
        let second = lexer.next_token();
        assert!(matches!(second.ty, TokenType::Illegal(_)));
        let third = lexer.next_token();
        assert!(matches!(third.ty, TokenType::Illegal(_)));
    }

    #[test]
    fn test_variable_reference() {
        let tokens = collect_tokens("$port1");
        assert_eq!(
            tokens,
            vec![TokenType::Dollar, TokenType::Ident("port1".to_string())]
        );
    }

    #[test]
    fn test_namespace_sep() {
        let mut lexer = Lexer::new("a::b".to_string());
        assert_eq!(lexer.next_token().ty, TokenType::Ident("a".to_string()));
        assert_eq!(lexer.next_token().ty, TokenType::NamespaceSep);
        assert_eq!(lexer.next_token().ty, TokenType::Ident("b".to_string()));
    }

    #[test]
    fn test_comments() {
        let tokens = collect_tokens("# comment\nshell = `bash`;");
        assert_eq!(
            tokens,
            vec![
                TokenType::Shell,
                TokenType::Assign,
                TokenType::Backtick("bash".to_string()),
                TokenType::Semicolon,
            ]
        );
    }

    #[test]
    fn test_consecutive_comments() {
        let tokens = collect_tokens("# a\n# b\nvar");
        assert_eq!(tokens, vec![TokenType::Var]);
    }

    #[test]
    fn test_comment_at_eof_without_newline() {
        let input = "var string x = `a`; # comment";
        let tokens = collect_tokens(input);
        assert_eq!(
            tokens,
            vec![
                TokenType::Var,
                TokenType::StringKw,
                TokenType::Ident("x".to_string()),
                TokenType::Assign,
                TokenType::Backtick("a".to_string()),
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
    fn test_line_col_tracking() {
        let input = "var string x = `hello`;\nvar string y = `world`;";
        let tokens = collect_all_tokens(input);
        assert_eq!(tokens[0].line, 1);
        assert_eq!(tokens[0].col, 1);
        let second_var = tokens
            .iter()
            .find(|t| matches!(&t.ty, TokenType::Var) && t.line == 2);
        assert!(second_var.is_some(), "expected 'var' on line 2");
        assert_eq!(second_var.unwrap().col, 1);
    }

    #[test]
    fn test_error_cases() {
        let cases = vec![
            ("bare:", "unexpected character: :"),
            ("@", "unexpected character: @"),
            ("`unterminated", "unterminated backtick string"),
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

    #[test]
    fn test_case_keyword() {
        let tokens = collect_tokens("case");
        assert_eq!(tokens, vec![TokenType::Case]);
    }

    #[test]
    fn test_case_inside_fn_body() {
        let input = "case $os { `Linux` { log `linux`; }; _ { log `other`; }; }";
        let tokens = collect_tokens(input);
        assert!(tokens.contains(&TokenType::Case));
        assert!(tokens.contains(&TokenType::Dollar));
        assert!(tokens.contains(&TokenType::LBrace));
        assert!(tokens.contains(&TokenType::RBrace));
        assert!(tokens.contains(&TokenType::Semicolon));
        assert!(tokens.contains(&TokenType::Log));
    }

    #[test]
    fn test_default_pattern() {
        let input = "case $x { _ { log `default`; }; }";
        let tokens = collect_tokens(input);
        assert!(tokens.contains(&TokenType::Case));
        assert!(tokens.contains(&TokenType::Ident("_".to_string())));
    }

    #[test]
    fn test_full_snippet() {
        let input = "import `./a.kiru`;\n\
                      var string port1 = `127.0.0.1:8080`;\n\
                      pr hello {\n\
                          url = `git@github.com:foo/bar.git`;\n\
                          dir = `bar`;\n\
                      }\n\
                      fn init {\n\
                          log `starting`;\n\
                          exec `go build`;\n\
                      }";
        let tokens = collect_tokens(input);
        assert!(tokens.contains(&TokenType::Import));
        assert!(tokens.contains(&TokenType::Var));
        assert!(tokens.contains(&TokenType::Pr));
        assert!(tokens.contains(&TokenType::Fn));
        assert!(tokens.contains(&TokenType::Log));
        assert!(tokens.contains(&TokenType::Exec));
    }

    #[test]
    fn test_path_termination_at_semicolons() {
        let input = "import `./foo.kiru`; import `./bar.kiru`;";
        let tokens = collect_tokens(input);
        assert_eq!(tokens[0], TokenType::Import);
        assert_eq!(tokens[1], TokenType::Backtick("./foo.kiru".to_string()));
        assert_eq!(tokens[2], TokenType::Semicolon);
        assert_eq!(tokens[3], TokenType::Import);
        assert_eq!(tokens[4], TokenType::Backtick("./bar.kiru".to_string()));
        assert_eq!(tokens[5], TokenType::Semicolon);
    }
}
