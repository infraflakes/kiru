/// Token types recognized by the kiru DSL lexer.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TokenType {
    Eof,
    Ident(String),
    /// A parsed template expression `( ... )`, `$( ... )`, or `@( ... )`.
    /// The template carries its resolved parts (literal / var / command).
    Template(crate::syntax::source::Template),
    /// A bare `)` outside any template. Never accepted by the grammar, but
    /// kept as a distinct token so a stray `)` reports as "found `)`" instead
    /// of a generic illegal-character error.
    RParen,
    LBrace,
    RBrace,
    Semicolon,
    Assign,
    /// `=>` separator inside run blocks: starts a new sequential stage. Calls
    /// separated by `;` run concurrently in the same stage; `=>` runs the next
    /// stage only after the current one finishes.
    ChainArrow,
    Import,
    Var,
    Fn,
    Run,
    Env,
    Log,
    Cd,
    Pr,
    Switch,
    Case,
    /// `::` separator used in run-block references (`pr::fn`).
    NamespaceSep,
}

/// A lexical token with its byte-offset span into the source text.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Token {
    pub(crate) token_type: TokenType,
    pub(crate) offset: usize,
    pub(crate) len: usize,
}

impl Token {
    pub(crate) fn new(ty: TokenType, offset: usize, len: usize) -> Self {
        Self {
            token_type: ty,
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
    ("pr", TokenType::Pr),
    ("fn", TokenType::Fn),
    ("run", TokenType::Run),
    ("env", TokenType::Env),
    ("log", TokenType::Log),
    ("cd", TokenType::Cd),
    ("case", TokenType::Case),
    ("switch", TokenType::Switch),
];

/// Convert a keyword string to its corresponding token type,
/// or return `TokenType::Ident` if it is not a keyword.
pub(crate) fn lookup_ident(ident: &str) -> TokenType {
    KEYWORDS
        .iter()
        .find(|(keyword, _)| *keyword == ident)
        .map(|(_, ty)| ty.clone())
        .unwrap_or(TokenType::Ident(ident.to_string()))
}

/// Returns the user-facing name of a token type. Keyword display names are
/// derived from the keyword table; punctuation and special types keep their
/// own spelling.
pub(crate) fn format_token_type(ty: &TokenType) -> String {
    if let Some((keyword, _)) = KEYWORDS.iter().find(|(_, keyword_ty)| keyword_ty == ty) {
        return format!("`{}`", keyword);
    }
    match ty {
        TokenType::LBrace => "`{`".to_string(),
        TokenType::RBrace => "`}`".to_string(),
        TokenType::RParen => "`)`".to_string(),
        TokenType::Semicolon => "`;`".to_string(),
        TokenType::Assign => "`=`".to_string(),
        TokenType::ChainArrow => "`=>`".to_string(),
        TokenType::Ident(_) => "identifier".to_string(),
        TokenType::NamespaceSep => "`::`".to_string(),
        TokenType::Template(_) => "template".to_string(),
        TokenType::Eof => "end of file".to_string(),
        TokenType::Import
        | TokenType::Var
        | TokenType::Fn
        | TokenType::Run
        | TokenType::Pr
        | TokenType::Log
        | TokenType::Env
        | TokenType::Cd
        | TokenType::Case
        | TokenType::Switch => unreachable!("keyword tokens are named by the keyword table"),
    }
}

pub(crate) fn format_token(token: &Token) -> String {
    match &token.token_type {
        TokenType::Ident(s) => format!("`{}`", s),
        TokenType::Template(_) => "template".to_string(),
        _ => format_token_type(&token.token_type),
    }
}

pub(crate) fn is_keyword_token(ty: &TokenType) -> bool {
    KEYWORDS.iter().any(|(_, keyword_ty)| keyword_ty == ty)
}
