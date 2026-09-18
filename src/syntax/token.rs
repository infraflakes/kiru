/// Token types recognized by the kiru DSL lexer.
///
/// Call-form keywords carry their template payload fused into the token:
/// the lexer only produces `Log(Some(t))` etc. when `(` immediately follows
/// the keyword, so a spaced `log (x)` lexes as a bare `Log(None)` and is
/// rejected by the parser through the generic unexpected-token path.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TokenType {
    Eof,
    Ident(String),
    /// A standalone template token: the value after `=` in a declaration, a
    /// pair value inside `env(...)`, or a bare `$(cmd)` statement.
    Template(crate::syntax::source::Template),
    /// A bare `)` outside any template. Never accepted by the grammar except
    /// as the closing paren of an `env(...)` pair list, but kept as a distinct
    /// token so a stray `)` reports as "found `)`" instead of a generic
    /// illegal-character error.
    RParen,
    LBrace,
    RBrace,
    Semicolon,
    Assign,
    /// `import(path);` with the path argument fused.
    Import(Option<Vec<crate::syntax::source::Template>>),
    Var,
    /// `fn name(params) { ... }` - a bare keyword: declarations take a name,
    /// never a call-shaped template.
    Fn,
    Run,
    /// `log(...)` with the argument list fused. `None` is the bare keyword,
    /// which only exists so the parser can reject it generically.
    Log(Option<Vec<crate::syntax::source::Template>>),
    /// `async() { ... }` - the concurrency primitive. Its parens must be
    /// empty; the body is the concurrent unit.
    Async(Option<Vec<crate::syntax::source::Template>>),
    /// `exec(cmd)` with the command argument fused.
    Exec(Option<Vec<crate::syntax::source::Template>>),
    /// `project(name) { ... }` with the name argument fused.
    Project(Option<Vec<crate::syntax::source::Template>>),
    /// `cd(...)` with the argument list fused.
    Cd(Option<Vec<crate::syntax::source::Template>>),
    /// The bare `env` keyword, only existing so the parser can reject the
    /// brace form generically. The fused `env(` pair list is `EnvOpen`.
    Env,
    /// `env(` - the pair list is not a template, so only the opening paren is
    /// fused; the pairs follow as an ordinary token stream until the bare
    /// closing `)` (`RParen`).
    EnvOpen,
    /// `switch(...)` with the subject argument fused.
    Switch(Option<Vec<crate::syntax::source::Template>>),
    /// `case(...)` with the pattern argument fused. `None` is the bare
    /// keyword, only valid inside a `switch` when followed by... nothing:
    /// patterns must be fused, so `None` is always rejected.
    Case(Option<Vec<crate::syntax::source::Template>>),
    /// `default` - the wildcard switch arm. It takes no pattern, so it is a
    /// bare keyword with no call parens: `default { ... };`.
    Default,
    /// `name(args)` for a non-keyword identifier: a function call with its
    /// `;`-separated argument list fused.
    Call {
        name: String,
        args: Vec<crate::syntax::source::Template>,
    },
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

/// How a reserved keyword participates in call fusion and display. The
/// single grammar fact per keyword from which the lexical layer derives:
/// fusion dispatch, reservedness, and user-facing spelling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum KeywordForm {
    /// Valid only in fused call form: `log(...)`, `case(...)`,
    /// `import(path)`, `switch(subject)`. The bare spelling always rejects.
    TemplateCall,
    /// Valid only bare: the wildcard arm `default { ... }`.
    Bare,
    /// The pair-list opener `env(`: only the paren fuses; the pairs follow
    /// as ordinary tokens.
    PairList,
    /// Raw command text: the whole paren region is one template, so `;` is
    /// literal inside it. `exec` reads a command, not an argument list.
    Command,
    /// Declaration keyword: takes a name, never call parens
    /// (`project foo {`, `fn build {`, `var x = (...)`, `run ci {`).
    Declaration,
}

/// Every reserved keyword: its spelling, its bare token, and its grammar
/// shape. The single source of truth for the lexical layer - lookup,
/// call-fusion dispatch, reservedness, and error display all derive from
/// this table, so adding a keyword means one row plus its token variant.
const KEYWORDS: &[(&str, TokenType, KeywordForm)] = &[
    ("import", TokenType::Import(None), KeywordForm::TemplateCall),
    ("var", TokenType::Var, KeywordForm::Declaration),
    (
        "project",
        TokenType::Project(None),
        KeywordForm::TemplateCall,
    ),
    ("fn", TokenType::Fn, KeywordForm::Declaration),
    ("run", TokenType::Run, KeywordForm::Declaration),
    ("env", TokenType::Env, KeywordForm::PairList),
    ("log", TokenType::Log(None), KeywordForm::TemplateCall),
    ("exec", TokenType::Exec(None), KeywordForm::Command),
    ("async", TokenType::Async(None), KeywordForm::TemplateCall),
    ("cd", TokenType::Cd(None), KeywordForm::TemplateCall),
    ("case", TokenType::Case(None), KeywordForm::TemplateCall),
    ("switch", TokenType::Switch(None), KeywordForm::TemplateCall),
    ("default", TokenType::Default, KeywordForm::Bare),
];

/// Attach a fused argument list to a call-form keyword's bare token.
/// The single bare-to-fused mapping: adding a call-form keyword means one
/// arm here on top of its `KEYWORDS` row.
pub(crate) fn fuse_call_arguments(
    bare: TokenType,
    args: Vec<crate::syntax::source::Template>,
) -> TokenType {
    match bare {
        TokenType::Log(_) => TokenType::Log(Some(args)),
        TokenType::Exec(_) => TokenType::Exec(Some(args)),
        TokenType::Project(_) => TokenType::Project(Some(args)),
        TokenType::Async(_) => TokenType::Async(Some(args)),
        TokenType::Cd(_) => TokenType::Cd(Some(args)),
        TokenType::Switch(_) => TokenType::Switch(Some(args)),
        TokenType::Case(_) => TokenType::Case(Some(args)),
        TokenType::Import(_) => TokenType::Import(Some(args)),
        _ => unreachable!("only call-form keywords reach argument fusion"),
    }
}

/// Convert a keyword string to its corresponding bare token type,
/// or return `TokenType::Ident` if it is not a keyword.
pub(crate) fn lookup_ident(ident: &str) -> TokenType {
    KEYWORDS
        .iter()
        .find(|(keyword, _, _)| *keyword == ident)
        .map(|(_, ty, _)| ty.clone())
        .unwrap_or(TokenType::Ident(ident.to_string()))
}

/// The reserved word behind a keyword token (payload ignored) and whether
/// it appeared in its fused call form. Every other keyword behavior derives
/// from the `KEYWORDS` table through this function. The non-keyword
/// variants are listed explicitly, without a wildcard, so adding a keyword
/// token without listing it here is a compile error, not a silent miss.
fn keyword_word_and_fusion(ty: &TokenType) -> Option<(&'static str, bool)> {
    let (word, fused) = match ty {
        TokenType::Import(template) => ("import", template.is_some()),
        TokenType::Log(template) => ("log", template.is_some()),
        TokenType::Async(template) => ("async", template.is_some()),
        TokenType::Exec(template) => ("exec", template.is_some()),
        TokenType::Project(template) => ("project", template.is_some()),
        TokenType::Cd(template) => ("cd", template.is_some()),
        TokenType::Switch(template) => ("switch", template.is_some()),
        TokenType::Case(template) => ("case", template.is_some()),
        TokenType::Env => ("env", false),
        TokenType::EnvOpen => ("env", true),
        TokenType::Var => ("var", false),
        TokenType::Fn => ("fn", false),
        TokenType::Run => ("run", false),
        TokenType::Default => ("default", false),
        TokenType::Eof
        | TokenType::Ident(_)
        | TokenType::Template(_)
        | TokenType::Call { .. }
        | TokenType::LBrace
        | TokenType::RBrace
        | TokenType::RParen
        | TokenType::Semicolon
        | TokenType::Assign => return None,
    };
    Some((word, fused))
}

/// The grammar shape of a keyword token, derived from the `KEYWORDS` table.
/// Drives the lexer's call-fusion dispatch.
pub(crate) fn keyword_shape(ty: &TokenType) -> Option<KeywordForm> {
    let (word, _) = keyword_word_and_fusion(ty)?;
    KEYWORDS
        .iter()
        .find(|(keyword, _, _)| *keyword == word)
        .map(|(_, _, shape)| *shape)
}

/// Whether `ty` is any spelling of a reserved keyword, fused or bare.
/// Reserved keywords cannot be used where an identifier is expected.
pub(crate) fn is_keyword_token(ty: &TokenType) -> bool {
    keyword_word_and_fusion(ty).is_some()
}

/// Returns the user-facing name of a token type. Keyword tokens are named
/// from the `KEYWORDS` table; call-form keywords display with their parens
/// when fused, so errors show the shape the grammar expects. Non-keyword
/// tokens keep their own spelling. Keyword and non-keyword variants are
/// both listed explicitly so a new token type cannot skip this match.
pub(crate) fn format_token_type(ty: &TokenType) -> String {
    match ty {
        TokenType::Import(_)
        | TokenType::Log(_)
        | TokenType::Exec(_)
        | TokenType::Project(_)
        | TokenType::Async(_)
        | TokenType::Cd(_)
        | TokenType::Switch(_)
        | TokenType::Case(_)
        | TokenType::Env
        | TokenType::EnvOpen
        | TokenType::Var
        | TokenType::Fn
        | TokenType::Run
        | TokenType::Default => {
            let (word, fused) =
                keyword_word_and_fusion(ty).expect("keyword arms carry the keyword table");
            let shape = KEYWORDS
                .iter()
                .find(|(keyword, _, _)| *keyword == word)
                .map(|(_, _, shape)| *shape)
                .expect("keyword table entry");
            match (shape, fused) {
                (KeywordForm::TemplateCall, true)
                | (KeywordForm::PairList, true)
                | (KeywordForm::Command, true) => format!("`{word}(...)`"),
                _ => format!("`{word}`"),
            }
        }
        TokenType::Eof => "end of file".to_string(),
        TokenType::Ident(_) => "identifier".to_string(),
        TokenType::Template(_) => "template".to_string(),
        TokenType::Call { name, .. } => format!("`{name}(...)`"),
        TokenType::LBrace => "`{`".to_string(),
        TokenType::RBrace => "`}`".to_string(),
        TokenType::RParen => "`)`".to_string(),
        TokenType::Semicolon => "`;`".to_string(),
        TokenType::Assign => "`=`".to_string(),
    }
}

pub(crate) fn format_token(token: &Token) -> String {
    match &token.token_type {
        TokenType::Ident(s) => format!("`{}`", s),
        TokenType::Template(_) => "template".to_string(),
        _ => format_token_type(&token.token_type),
    }
}
