/// Token types recognized by the kiru DSL lexer.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    Eof,
    Illegal(String),
    Ident(String),
    Backtick(String),
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semicolon,
    Comma,
    Assign,
    Dollar,
    Import,
    Shell,
    Sanctuary,
    Var,
    StringKw,
    Fn,
    Run,
    Arrow,
    Pr,
    Log,
    Exec,
    Cd,
    Env,
    Case,
}

/// A lexical token with source position tracking.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub ty: TokenType,
    pub line: usize,
    pub col: usize,
    pub offset: usize,
    pub len: usize,
}

impl Token {
    pub fn new(ty: TokenType, line: usize, col: usize, offset: usize, len: usize) -> Self {
        Self {
            ty,
            line,
            col,
            offset,
            len,
        }
    }
}

/// Convert a keyword string to its corresponding token type,
/// or return `TokenType::Ident` if it is not a keyword.
pub fn lookup_ident(ident: &str) -> TokenType {
    match ident {
        "sanctuary" => TokenType::Sanctuary,
        "import" => TokenType::Import,
        "var" => TokenType::Var,
        "string" => TokenType::StringKw,
        "pr" => TokenType::Pr,
        "fn" => TokenType::Fn,
        "run" => TokenType::Run,
        "env" => TokenType::Env,
        "log" => TokenType::Log,
        "exec" => TokenType::Exec,
        "cd" => TokenType::Cd,
        "shell" => TokenType::Shell,
        "case" => TokenType::Case,
        _ => TokenType::Ident(ident.to_string()),
    }
}

pub fn format_token_type(ty: &TokenType) -> &'static str {
    match ty {
        TokenType::LBrace => "`{`",
        TokenType::RBrace => "`}`",
        TokenType::LBracket => "`[`",
        TokenType::RBracket => "`]`",
        TokenType::Semicolon => "`;`",
        TokenType::Comma => "`,`",
        TokenType::Assign => "`=`",
        TokenType::Dollar => "`$`",
        TokenType::Shell => "`shell`",
        TokenType::StringKw => "`string`",
        TokenType::Sanctuary => "`sanctuary`",
        TokenType::Import => "`import`",
        TokenType::Var => "`var`",
        TokenType::Pr => "`pr`",
        TokenType::Fn => "`fn`",
        TokenType::Run => "`run`",
        TokenType::Arrow => "`=>`",
        TokenType::Env => "`env`",
        TokenType::Case => "`case`",
        TokenType::Log => "`log`",
        TokenType::Exec => "`exec`",
        TokenType::Cd => "`cd`",
        TokenType::Ident(_) => "identifier",
        TokenType::Backtick(_) => "backtick string",
        TokenType::Illegal(_) => "illegal token",
        TokenType::Eof => "end of file",
    }
}

pub fn format_token(token: &Token) -> String {
    match &token.ty {
        TokenType::Ident(s) => format!("`{}`", s),
        TokenType::Backtick(s) => format!("`{}`", s),
        TokenType::Illegal(s) => format!("`{}`", s),
        _ => format_token_type(&token.ty).to_string(),
    }
}

pub fn is_keyword_token(ty: &TokenType) -> bool {
    matches!(
        ty,
        TokenType::Log
            | TokenType::Exec
            | TokenType::Cd
            | TokenType::Case
            | TokenType::Env
            | TokenType::Var
            | TokenType::Fn
            | TokenType::Run
            | TokenType::Pr
            | TokenType::Shell
            | TokenType::StringKw
            | TokenType::Sanctuary
            | TokenType::Import
    )
}
