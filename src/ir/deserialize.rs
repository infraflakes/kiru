//! Textual kirufile deserialization.

use super::types::*;
use crate::diagnostics::Span;

/// A parsed s-expression node used while reading the textual kirufile.
#[derive(Debug, Clone)]
enum Sexp {
    Sym(String),
    Str(String),
    List(Vec<Sexp>),
}

/// Tokenize the textual kirufile into s-expression tokens.
/// Lightweight token for the textual kirufile s-expression format.
#[derive(Clone, Copy)]
enum Token {
    LParen,
    RParen,
    Sym,
    Str,
}

fn tokenize_kirufile(src: &str) -> Result<Vec<(Token, String)>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut tokens: Vec<(Token, String)> = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '(' => {
                tokens.push((Token::LParen, "(".to_string()));
                i += 1;
            }
            ')' => {
                tokens.push((Token::RParen, ")".to_string()));
                i += 1;
            }
            '"' => {
                i += 1;
                let mut s = String::new();
                while i < chars.len() {
                    let ch = chars[i];
                    if ch == '\\' {
                        i += 1;
                        if i >= chars.len() {
                            break;
                        }
                        let esc = chars[i];
                        match esc {
                            'n' => s.push('\n'),
                            't' => s.push('\t'),
                            'r' => s.push('\r'),
                            '\\' => s.push('\\'),
                            '"' => s.push('"'),
                            '0' => s.push('\0'),
                            'u' => {
                                // Expect '{' ... '}'.
                                if i + 1 < chars.len() && chars[i + 1] == '{' {
                                    i += 2;
                                    let mut hex = String::new();
                                    while i < chars.len() && chars[i] != '}' {
                                        hex.push(chars[i]);
                                        i += 1;
                                    }
                                    if i < chars.len() {
                                        i += 1; // consume '}'
                                    }
                                    if let Ok(cp) = u32::from_str_radix(&hex, 16)
                                        && let Some(ch) = char::from_u32(cp)
                                    {
                                        s.push(ch);
                                    }
                                }
                            }
                            other => s.push(other),
                        }
                        i += 1;
                    } else if ch == '"' {
                        i += 1;
                        break;
                    } else {
                        s.push(ch);
                        i += 1;
                    }
                }
                tokens.push((Token::Str, s));
            }
            _ => {
                let start = i;
                while i < chars.len() {
                    let ch = chars[i];
                    if ch.is_whitespace() || ch == '(' || ch == ')' || ch == '"' {
                        break;
                    }
                    i += 1;
                }
                let sym: String = chars[start..i].iter().collect();
                tokens.push((Token::Sym, sym));
            }
        }
    }
    Ok(tokens)
}

/// Parse s-expression tokens into a tree.
fn parse_sexp_tokens(tokens: &[(Token, String)], pos: &mut usize) -> Result<Sexp, String> {
    use Token::*;
    if *pos >= tokens.len() {
        return Err("unexpected end of kirufile".to_string());
    }
    match &tokens[*pos].0 {
        LParen => {
            *pos += 1;
            let mut items = Vec::new();
            while *pos < tokens.len() && !matches!(tokens[*pos].0, RParen) {
                items.push(parse_sexp_tokens(tokens, pos)?);
            }
            if *pos >= tokens.len() {
                return Err("unterminated list in kirufile".to_string());
            }
            *pos += 1; // consume ')'
            Ok(Sexp::List(items))
        }
        RParen => Err("unexpected `)` in kirufile".to_string()),
        Sym => {
            let s = tokens[*pos].1.clone();
            *pos += 1;
            Ok(Sexp::Sym(s))
        }
        Str => {
            let s = tokens[*pos].1.clone();
            *pos += 1;
            Ok(Sexp::Str(s))
        }
    }
}

fn sym(sexp: &Sexp) -> Option<&str> {
    match sexp {
        Sexp::Sym(s) => Some(s),
        _ => None,
    }
}

fn as_list(sexp: &Sexp) -> Option<&[Sexp]> {
    match sexp {
        Sexp::List(items) => Some(items),
        _ => None,
    }
}

fn expect_sym(items: &[Sexp], idx: usize, expected: &str) -> Result<(), String> {
    match items.get(idx) {
        Some(Sexp::Sym(s)) if s == expected => Ok(()),
        other => Err(format!(
            "expected `{}` at position {}, found {:?}",
            expected, idx, other
        )),
    }
}

fn expect_str(items: &[Sexp], idx: usize, ctx: &str) -> Result<String, String> {
    match items.get(idx) {
        Some(Sexp::Str(s)) => Ok(s.clone()),
        other => Err(format!("expected string for {} , found {:?}", ctx, other)),
    }
}

fn expect_sym_arg(items: &[Sexp], idx: usize, ctx: &str) -> Result<String, String> {
    match items.get(idx) {
        Some(Sexp::Sym(s)) => Ok(s.clone()),
        other => Err(format!(
            "expected identifier for {}, found {:?}",
            ctx, other
        )),
    }
}

/// Read a `(t <segment> ...)` template node.
fn read_template(node: &Sexp) -> Result<Template, String> {
    let items = as_list(node).ok_or("expected template list".to_string())?;
    expect_sym(items, 0, "t")?;
    let mut segments = Vec::new();
    for segment_node in &items[1..] {
        let pitems = as_list(segment_node).ok_or("expected segment list".to_string())?;
        match pitems.first() {
            Some(Sexp::Sym(s)) if s == "lit" => {
                let text = expect_str(pitems, 1, "lit")?;
                segments.push(Segment::Literal(text));
            }
            Some(Sexp::Sym(s)) if s == "cmd" => {
                // New format: (cmd START LEN FILE INNER_TEMPLATE)
                if pitems.len() >= 5 {
                    let start: usize = expect_sym_arg(pitems, 1, "cmd start")?
                        .parse()
                        .map_err(|e| format!("cmd start: {}", e))?;
                    let len: usize = expect_sym_arg(pitems, 2, "cmd len")?
                        .parse()
                        .map_err(|e| format!("cmd len: {}", e))?;
                    let file = expect_str(pitems, 3, "cmd file")?;
                    let inner = read_template(&pitems[4])?;
                    segments.push(Segment::Command(inner, Span::new(start, len), file));
                } else {
                    // Legacy format fallback: (cmd INNER_TEMPLATE)
                    let inner = read_template(&pitems[1])?;
                    segments.push(Segment::Command(inner, Span::new(0, 0), String::new()));
                }
            }
            other => return Err(format!("unknown segment: {:?}", other)),
        }
    }
    Ok(Template { segments })
}

fn read_instructions(nodes: &[Sexp]) -> Result<Vec<Instruction>, String> {
    let mut out = Vec::new();
    for node in nodes {
        let items = as_list(node).ok_or("expected instruction list".to_string())?;
        let head = sym(items.first().ok_or("empty instruction".to_string())?)
            .ok_or("instruction head must be a symbol".to_string())?;
        match head {
            "exec" => {
                let value = read_template(&items[1])?;
                out.push(Instruction::Exec { value });
            }
            "log" => {
                let value = read_template(&items[1])?;
                out.push(Instruction::Log(value));
            }
            "cd" => {
                let value = read_template(&items[1])?;
                out.push(Instruction::Cd(value));
            }
            "env" => {
                let pairs_node = items.get(1).ok_or("env missing pairs".to_string())?;
                let body_node = items.get(2).ok_or("env missing body".to_string())?;
                let pair_items =
                    as_list(pairs_node).ok_or("env pairs must be a list".to_string())?;
                let mut pairs = Vec::new();
                for p in pair_items {
                    let pi = as_list(p).ok_or("env pair must be a list".to_string())?;
                    let key = expect_sym_arg(pi, 0, "env key")?;
                    let value = read_template(&pi[1])?;
                    pairs.push(EnvPair { key, value });
                }
                let body_items = as_list(body_node).ok_or("env body must be a list".to_string())?;
                let body = read_instructions(body_items)?;
                out.push(Instruction::Env { pairs, body });
            }
            "switch" => {
                let subject = read_template(&items[1])?;
                let mut arms = Vec::new();
                for arm_node in &items[2..] {
                    let ai = as_list(arm_node).ok_or("case must be a list".to_string())?;
                    expect_sym(ai, 0, "case")?;
                    let pat = match ai.get(1) {
                        Some(Sexp::Sym(s)) if s == "_" => ArmPattern::Default,
                        Some(Sexp::Str(s)) => ArmPattern::Lit(s.clone()),
                        other => return Err(format!("bad case pattern: {:?}", other)),
                    };
                    let body = read_instructions(&ai[2..])?;
                    arms.push(Arm { pattern: pat, body });
                }
                out.push(Instruction::Switch { subject, arms });
            }
            other => return Err(format!("unknown instruction: {}", other)),
        }
    }
    Ok(out)
}

impl Ir {
    /// Parse a textual kirufile into an `Ir`.
    pub fn deserialize(src: &str) -> Result<Ir, String> {
        let tokens = tokenize_kirufile(src)?;
        let mut pos = 0;
        let root = parse_sexp_tokens(&tokens, &mut pos)?;
        let items = as_list(&root).ok_or("kirufile must be a list".to_string())?;
        expect_sym(items, 0, "kirufile")?;

        let mut ir = Ir::default();
        let mut idx = 1;
        while idx < items.len() {
            let node = &items[idx];
            let ni = as_list(node).ok_or("top-level entry must be a list".to_string())?;
            let head = sym(ni.first().ok_or("empty entry".to_string())?)
                .ok_or("entry head must be a symbol".to_string())?;
            match head {
                "version" => {}
                "shell" => ir.shell = expect_str(ni, 1, "shell")?,
                "timeout" => {
                    let val: u64 = expect_sym_arg(ni, 1, "timeout")?
                        .parse()
                        .map_err(|e| format!("timeout value: {}", e))?;
                    ir.timeout = val;
                }
                "sources" => {
                    let mut j = 1;
                    while j < ni.len() {
                        let si = as_list(&ni[j]).ok_or("source entry must be list".to_string())?;
                        let name = expect_str(si, 0, "source name")?;
                        let text = expect_str(si, 1, "source text")?;
                        ir.sources.insert(name, text);
                        j += 1;
                    }
                }
                "sync" => {
                    let id = expect_sym_arg(ni, 1, "sync id")?;
                    let mut sync = Sync::default();
                    let mut j = 2;
                    while j < ni.len() {
                        let fi = as_list(&ni[j]).ok_or("sync field must be list".to_string())?;
                        let f = sym(fi.first().ok_or("sync field head".to_string())?)
                            .ok_or("sync field head".to_string())?;
                        match f {
                            "url" => sync.url = read_template(&fi[1])?,
                            "dir" => sync.dir = read_template(&fi[1])?,
                            "branch" => sync.branch = read_template(&fi[1])?,
                            "strategy" => sync.strategy = read_template(&fi[1])?,
                            other => return Err(format!("unknown sync field: {}", other)),
                        }
                        j += 1;
                    }
                    ir.repositories.insert(id, sync);
                }
                "project" => {
                    let id = expect_sym_arg(ni, 1, "project id")?;
                    let mut project = Project::default();
                    let mut j = 2;
                    while j < ni.len() {
                        let bi = as_list(&ni[j]).ok_or("project entry must be list".to_string())?;
                        let b = sym(bi.first().ok_or("project entry head".to_string())?)
                            .ok_or("project entry head".to_string())?;
                        match b {
                            "fn" => {
                                let fn_name = expect_sym_arg(bi, 1, "fn name")?;
                                let body = read_instructions(&bi[2..])?;
                                project.functions.insert(fn_name, body);
                            }
                            other => return Err(format!("unknown project entry: {}", other)),
                        }
                        j += 1;
                    }
                    ir.projects.insert(id, project);
                }
                "run" => {
                    let id = expect_sym_arg(ni, 1, "run id")?;
                    let mut stages = Vec::new();
                    let mut j = 2;
                    while j < ni.len() {
                        let stage = as_list(&ni[j]).ok_or("stage must be list".to_string())?;
                        expect_sym(stage, 0, "stage")?;
                        let mut calls = Vec::new();
                        let mut k = 1;
                        while k < stage.len() {
                            let ci = as_list(&stage[k]).ok_or("call must be list".to_string())?;
                            expect_sym(ci, 0, "call")?;
                            let project = expect_sym_arg(ci, 1, "call project")?;
                            let function = expect_sym_arg(ci, 2, "call function")?;
                            calls.push(Call { project, function });
                            k += 1;
                        }
                        stages.push(calls);
                        j += 1;
                    }
                    ir.execution_chains.insert(id, stages);
                }
                other => return Err(format!("unknown top-level entry: {}", other)),
            }
            idx += 1;
        }
        Ok(ir)
    }
}
