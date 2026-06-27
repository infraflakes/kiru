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
