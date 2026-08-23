/// Token types recognized by the kiru DSL lexer.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    Eof,
    Illegal(String),
    Ident(String),
    NamespaceSep,
    Backtick(String),
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Semicolon,
    Assign,
    Dollar,
    Import,
    Shell,
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
    Use,
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

/// All DSL keywords paired with their token types. Single source of truth
/// used by the lexer (`lookup_ident`), error formatting and keyword checks.
const KEYWORDS: &[(&str, TokenType)] = &[
    ("import", TokenType::Import),
    ("var", TokenType::Var),
    ("string", TokenType::StringKw),
    ("pr", TokenType::Pr),
    ("fn", TokenType::Fn),
    ("run", TokenType::Run),
    ("env", TokenType::Env),
    ("log", TokenType::Log),
    ("exec", TokenType::Exec),
    ("cd", TokenType::Cd),
    ("shell", TokenType::Shell),
    ("case", TokenType::Case),
    ("use", TokenType::Use),
];

/// Convert a keyword string to its corresponding token type,
/// or return `TokenType::Ident` if it is not a keyword.
pub fn lookup_ident(ident: &str) -> TokenType {
    KEYWORDS
        .iter()
        .find(|(keyword, _)| *keyword == ident)
        .map(|(_, ty)| ty.clone())
        .unwrap_or(TokenType::Ident(ident.to_string()))
}

/// Returns the user-facing name of a token type. Keyword display names are
/// derived from the keyword table; punctuation and special types keep their
/// own spelling.
pub fn format_token_type(ty: &TokenType) -> String {
    if let Some((keyword, _)) = KEYWORDS.iter().find(|(_, keyword_ty)| keyword_ty == ty) {
        return format!("`{}`", keyword);
    }
    match ty {
        TokenType::LBrace => "`{`".to_string(),
        TokenType::RBrace => "`}`".to_string(),
        TokenType::LBracket => "`[`".to_string(),
        TokenType::RBracket => "`]`".to_string(),
        TokenType::LParen => "`(`".to_string(),
        TokenType::RParen => "`)`".to_string(),
        TokenType::Semicolon => "`;`".to_string(),
        TokenType::Assign => "`=`".to_string(),
        TokenType::Dollar => "`$`".to_string(),
        TokenType::Arrow => "`=>`".to_string(),
        TokenType::Ident(_) => "identifier".to_string(),
        TokenType::NamespaceSep => "`::`".to_string(),
        TokenType::Backtick(_) => "backtick string".to_string(),
        TokenType::Illegal(_) => "illegal token".to_string(),
        TokenType::Eof => "end of file".to_string(),
        // Keywords are named by the keyword table above; this grouped arm
        // forces the compiler to keep the enum, the table, and this match
        // in sync when a new keyword is added.
        TokenType::Import
        | TokenType::Shell
        | TokenType::Var
        | TokenType::StringKw
        | TokenType::Fn
        | TokenType::Run
        | TokenType::Pr
        | TokenType::Log
        | TokenType::Exec
        | TokenType::Cd
        | TokenType::Case
        | TokenType::Use
        | TokenType::Env => unreachable!("keyword tokens are named by the keyword table"),
    }
}

pub fn format_token(token: &Token) -> String {
    match &token.ty {
        TokenType::Ident(s) => format!("`{}`", s),
        TokenType::Backtick(s) => format!("`{}`", s),
        TokenType::Illegal(s) => format!("`{}`", s),
        _ => format_token_type(&token.ty),
    }
}

pub fn is_keyword_token(ty: &TokenType) -> bool {
    KEYWORDS.iter().any(|(_, keyword_ty)| keyword_ty == ty)
}
