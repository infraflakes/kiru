//! The execution plan: the compiler's only outward contract.
//!
//! Kiru lowers a `.kiru` config into a [`Plan`], a resolved, in-memory IR that
//! the runner consumes directly. The compiler also serializes `Plan` to a
//! textual "kirufile" s-expression (and parses it back) so the IR is
//! debuggable and inspectable. Everything is a resolved `String`: there is no
//! type or operator system — the DSL is an IaC task runner.

use std::collections::BTreeMap;

/// A single piece of a [`Template`].
///
/// - `Lit` is literal text.
/// - `Cmd` is a `$(command)` substitution whose inner template is run through
///   `shell -c` at runtime and replaced by its captured stdout.
///
/// `@(var)` references no longer exist in the plan: the compiler inlines every
/// variable into the template that uses it before the plan is built, so there is
/// no runtime variable scope to resolve against.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Lit(String),
    Cmd(Template),
}

/// A template: the single string-valued form in the DSL.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Template {
    pub parts: Vec<Part>,
}

impl Template {
    /// A template consisting of a single literal string. Test-only helper used
    /// while building round-trip fixtures.
    #[cfg(test)]
    pub fn lit(s: &str) -> Self {
        Template {
            parts: vec![Part::Lit(s.to_string())],
        }
    }
}

/// A single resolved `env` block pair.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvPair {
    pub key: String,
    pub value: Template,
}

/// A pattern arm inside a `switch` block.
#[derive(Debug, Clone, PartialEq)]
pub enum ArmPattern {
    /// A literal string to match the resolved subject against.
    Lit(String),
    /// The `_` default arm.
    Default,
}

/// A single arm of a resolved `switch` block.
#[derive(Debug, Clone, PartialEq)]
pub struct Arm {
    pub pattern: ArmPattern,
    pub body: Vec<Instruction>,
}

/// A fully resolved function-body instruction, ready to execute.
#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    /// Execute `value` for its side effects at runtime. Every `$(command)` part
    /// of the template is run through `shell -c` and streamed to output. There is
    /// no `target`: variable bindings are inlined away at compile time, so a
    /// `bind` is purely a command execution statement.
    Bind { value: Template },
    /// Emit `value` to the output log.
    Log(Template),
    /// Change the working directory to the resolved `value`.
    Cd(Template),
    /// Export `pairs` to the command subprocess environment for the duration
    /// of `body`.
    Env {
        pairs: Vec<EnvPair>,
        body: Vec<Instruction>,
    },
    /// Match the resolved `subject` against each arm's pattern; the first
    /// matching arm runs with an isolated local scope frame.
    Switch { subject: Template, arms: Vec<Arm> },
}

/// A `project::function` reference inside a `run` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub project: String,
    pub function: String,
}

impl Call {
    /// Fully-qualified `project::function` name used in labels and rendering.
    pub fn fqn(&self) -> String {
        format!("{}::{}", self.project, self.function)
    }
}

/// A resolved repository/sync declaration.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sync {
    pub url: Template,
    pub dir: Template,
    pub branch: Template,
    pub strategy: Template,
}

/// A fully compiled project: its functions (variables are inlined into the
/// templates that use them at compile time, so nothing static lives here).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Project {
    /// Functions belonging to this project, each lowered to `Instruction`s.
    pub functions: BTreeMap<String, Vec<Instruction>>,
}

/// The final, fully resolved plan. The runner works exclusively with this type.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Plan {
    /// Shell used for `$(command)` substitution and `exec` statements.
    pub shell: String,
    /// Repositories declared via `sync name { ... }`.
    pub syncs: BTreeMap<String, Sync>,
    /// Projects (the merge of a `sync` block and a `pr` block of the same name).
    pub projects: BTreeMap<String, Project>,
    /// Run blocks keyed by name. Each block is an ordered list of chains; calls
    /// joined by `=>` form one sequential chain (each runs after the previous),
    /// and `;` separates chains which run concurrently with one another.
    pub run_blocks: BTreeMap<String, Vec<Vec<Call>>>,
}

// ── Textual kirufile serialization ───────────────────────────────────────────

/// Write a string as a double-quoted, escaped literal (Rust `Debug` quoting).
pub(crate) fn quote_string(s: &str) -> String {
    format!("{:?}", s)
}

mod kirufile {
    use super::*;

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

    /// Read a `(t <part> ...)` template node.
    fn read_template(node: &Sexp) -> Result<Template, String> {
        let items = as_list(node).ok_or("expected template list".to_string())?;
        expect_sym(items, 0, "t")?;
        let mut parts = Vec::new();
        for part_node in &items[1..] {
            let pitems = as_list(part_node).ok_or("expected part list".to_string())?;
            match pitems.first() {
                Some(Sexp::Sym(s)) if s == "lit" => {
                    let text = expect_str(pitems, 1, "lit")?;
                    parts.push(Part::Lit(text));
                }
                Some(Sexp::Sym(s)) if s == "cmd" => {
                    let inner = read_template(&pitems[1])?;
                    parts.push(Part::Cmd(inner));
                }
                other => return Err(format!("unknown part: {:?}", other)),
            }
        }
        Ok(Template { parts })
    }

    fn read_instructions(nodes: &[Sexp]) -> Result<Vec<Instruction>, String> {
        let mut out = Vec::new();
        for node in nodes {
            let items = as_list(node).ok_or("expected instruction list".to_string())?;
            let head = sym(items.first().ok_or("empty instruction".to_string())?)
                .ok_or("instruction head must be a symbol".to_string())?;
            match head {
                "bind" => {
                    let value = read_template(&items[1])?;
                    out.push(Instruction::Bind { value });
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
                    let body_items =
                        as_list(body_node).ok_or("env body must be a list".to_string())?;
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

    /// Append a `Template` as a `(t ...)` s-expression node to `out`.
    fn append_template(buf: &mut String, tmpl: &Template) {
        buf.push_str("(t");
        for part in &tmpl.parts {
            match part {
                Part::Lit(s) => buf.push_str(&format!(" (lit {})", quote_string(s))),
                Part::Cmd(inner) => {
                    buf.push_str(" (cmd");
                    append_template(buf, inner);
                    buf.push(')');
                }
            }
        }
        buf.push(')');
    }

    impl Plan {
        /// Serialize this plan to the textual kirufile s-expression format.
        pub fn to_kirufile(&self) -> String {
            let mut out = String::new();
            out.push_str("(kirufile\n");
            out.push_str("  (version 1)\n");
            out.push_str(&format!("  (shell {})\n", quote_string(&self.shell)));

            for (id, sync) in &self.syncs {
                out.push_str(&format!("  (sync {} (url ", id));
                append_template(&mut out, &sync.url);
                out.push_str(") (dir ");
                append_template(&mut out, &sync.dir);
                out.push_str(") (branch ");
                append_template(&mut out, &sync.branch);
                out.push_str(") (strategy ");
                append_template(&mut out, &sync.strategy);
                out.push_str("))\n");
            }

            for (id, project) in &self.projects {
                out.push_str(&format!("  (project {}\n", id));
                for (fn_name, body) in &project.functions {
                    out.push_str(&format!("    (fn {}\n", fn_name));
                    out.push_str(&write_instructions(body, 3));
                    out.push_str("    )\n");
                }
                out.push_str("  )\n");
            }

            for (id, stages) in &self.run_blocks {
                out.push_str(&format!("  (run {}", id));
                for stage in stages {
                    out.push_str(" (stage");
                    for call in stage {
                        out.push_str(&format!(" (call {} {})", call.project, call.function));
                    }
                    out.push(')');
                }
                out.push_str(")\n");
            }

            out.push_str(")\n");
            out
        }

        /// Parse a textual kirufile into a `Plan`.
        pub fn from_kirufile(src: &str) -> Result<Plan, String> {
            let tokens = tokenize_kirufile(src)?;
            let mut pos = 0;
            let root = parse_sexp_tokens(&tokens, &mut pos)?;
            let items = as_list(&root).ok_or("kirufile must be a list".to_string())?;
            expect_sym(items, 0, "kirufile")?;

            let mut plan = Plan::default();
            let mut idx = 1;
            while idx < items.len() {
                let node = &items[idx];
                let ni = as_list(node).ok_or("top-level entry must be a list".to_string())?;
                let head = sym(ni.first().ok_or("empty entry".to_string())?)
                    .ok_or("entry head must be a symbol".to_string())?;
                match head {
                    "version" => {}
                    "shell" => plan.shell = expect_str(ni, 1, "shell")?,
                    "sync" => {
                        let id = expect_sym_arg(ni, 1, "sync id")?;
                        let mut sync = Sync::default();
                        let mut j = 2;
                        while j < ni.len() {
                            let fi =
                                as_list(&ni[j]).ok_or("sync field must be list".to_string())?;
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
                        plan.syncs.insert(id, sync);
                    }
                    "project" => {
                        let id = expect_sym_arg(ni, 1, "project id")?;
                        let mut project = Project::default();
                        let mut j = 2;
                        while j < ni.len() {
                            let bi =
                                as_list(&ni[j]).ok_or("project entry must be list".to_string())?;
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
                        plan.projects.insert(id, project);
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
                                let ci =
                                    as_list(&stage[k]).ok_or("call must be list".to_string())?;
                                expect_sym(ci, 0, "call")?;
                                let project = expect_sym_arg(ci, 1, "call project")?;
                                let function = expect_sym_arg(ci, 2, "call function")?;
                                calls.push(Call { project, function });
                                k += 1;
                            }
                            stages.push(calls);
                            j += 1;
                        }
                        plan.run_blocks.insert(id, stages);
                    }
                    other => return Err(format!("unknown top-level entry: {}", other)),
                }
                idx += 1;
            }
            Ok(plan)
        }
    }

    fn write_instructions(insts: &[Instruction], indent: usize) -> String {
        let pad: String = "  ".repeat(indent);
        let mut out = String::new();
        for inst in insts {
            match inst {
                Instruction::Bind { value } => {
                    out.push_str(&format!("{}(bind {})\n", pad, write_template(value)));
                }
                Instruction::Log(value) => {
                    out.push_str(&format!("{}(log {})\n", pad, write_template(value)));
                }
                Instruction::Cd(value) => {
                    out.push_str(&format!("{}(cd {})\n", pad, write_template(value)));
                }
                Instruction::Env { pairs, body } => {
                    out.push_str(&pad);
                    out.push_str("(env (");
                    let mut first = true;
                    for p in pairs {
                        if !first {
                            out.push(' ');
                        }
                        first = false;
                        out.push_str(&format!("({} {})", p.key, write_template(&p.value)));
                    }
                    out.push_str(") (");
                    let mut first_b = true;
                    for b in body {
                        if !first_b {
                            out.push(' ');
                        }
                        first_b = false;
                        out.push_str(&write_instruction_inline(b, indent));
                    }
                    out.push_str("))\n");
                }
                Instruction::Switch { subject, arms } => {
                    out.push_str(&format!("{}(switch {}", pad, write_template(subject)));
                    for arm in arms {
                        match &arm.pattern {
                            ArmPattern::Lit(p) => {
                                out.push_str(&format!(" (case {} ", quote_string(p)));
                            }
                            ArmPattern::Default => {
                                out.push_str(" (case _ ");
                            }
                        }
                        let mut first = true;
                        for b in &arm.body {
                            if !first {
                                out.push(' ');
                            }
                            first = false;
                            out.push_str(&write_instruction_inline(b, indent));
                        }
                        out.push(')');
                    }
                    out.push_str(")\n");
                }
            }
        }
        out
    }

    /// Write a single instruction without a trailing newline (used inside `env`/
    /// `switch` bodies which are inline lists).
    fn write_instruction_inline(inst: &Instruction, _indent: usize) -> String {
        match inst {
            Instruction::Bind { value } => {
                format!("(bind {})", write_template(value))
            }
            Instruction::Log(value) => format!("(log {})", write_template(value)),
            Instruction::Cd(value) => format!("(cd {})", write_template(value)),
            Instruction::Env { pairs, body } => {
                let mut s = String::from("(env (");
                let mut first = true;
                for p in pairs {
                    if !first {
                        s.push(' ');
                    }
                    first = false;
                    s.push_str(&format!("({} {})", p.key, write_template(&p.value)));
                }
                s.push_str(") (");
                let mut first_b = true;
                for b in body {
                    if !first_b {
                        s.push(' ');
                    }
                    first_b = false;
                    s.push_str(&write_instruction_inline(b, _indent));
                }
                s.push(')');
                s
            }
            Instruction::Switch { subject, arms } => {
                let mut s = format!("(switch {}", write_template(subject));
                for arm in arms {
                    match &arm.pattern {
                        ArmPattern::Lit(p) => s.push_str(&format!(" (case {} ", quote_string(p))),
                        ArmPattern::Default => s.push_str(" (case _ "),
                    }
                    let mut first = true;
                    for b in &arm.body {
                        if !first {
                            s.push(' ');
                        }
                        first = false;
                        s.push_str(&write_instruction_inline(b, _indent));
                    }
                    s.push(')');
                }
                s.push(')');
                s
            }
        }
    }
}

/// Render a template back to its `(t (lit ...) (cmd ...))` s-expression form.
/// Used by `to_kirufile` and by the status/sync printers.
pub fn write_template(tmpl: &Template) -> String {
    let mut out = String::from("(t");
    for part in &tmpl.parts {
        match part {
            Part::Lit(s) => out.push_str(&format!(" (lit {})", quote_string(s))),
            Part::Cmd(inner) => out.push_str(&format!(" (cmd {})", write_template(inner))),
        }
    }
    out.push(')');
    out
}

/// Render a template as a human-readable string for terminal display: literals
/// are shown verbatim and `$(command)` parts are shown by their inner literal
/// text. Commands are never executed here.
pub fn render_template(tmpl: &Template) -> String {
    let mut out = String::new();
    for part in &tmpl.parts {
        match part {
            Part::Lit(s) => out.push_str(s),
            Part::Cmd(inner) => out.push_str(&render_template(inner)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(test)]
    fn sample_plan() -> Plan {
        let check_cmd = Template {
            parts: vec![Part::Cmd(Template::lit("test -f $HOME"))],
        };

        let mut project = Project::default();
        project.functions.insert(
            "ssh".to_string(),
            vec![
                Instruction::Bind {
                    value: check_cmd.clone(),
                },
                Instruction::Switch {
                    subject: check_cmd,
                    arms: vec![
                        Arm {
                            pattern: ArmPattern::Lit("1".to_string()),
                            body: vec![Instruction::Log(Template::lit("switching"))],
                        },
                        Arm {
                            pattern: ArmPattern::Default,
                            body: vec![Instruction::Log(Template::lit("default"))],
                        },
                    ],
                },
                Instruction::Env {
                    pairs: vec![EnvPair {
                        key: "GO".to_string(),
                        value: Template::lit("1"),
                    }],
                    body: vec![Instruction::Cd(Template::lit("project"))],
                },
            ],
        );

        let mut syncs = BTreeMap::new();
        syncs.insert(
            "nix".to_string(),
            Sync {
                url: Template::lit("https://example.com/nix"),
                dir: Template::lit("/home/me/nix"),
                branch: Template::lit("main"),
                strategy: Template::lit("clone"),
            },
        );

        let mut run_blocks = BTreeMap::new();
        run_blocks.insert(
            "bootstrap".to_string(),
            vec![vec![Call {
                project: "nix".to_string(),
                function: "ssh".to_string(),
            }]],
        );

        Plan {
            shell: "sh".to_string(),
            syncs,
            projects: {
                let mut m = BTreeMap::new();
                m.insert("nix".to_string(), project);
                m
            },
            run_blocks,
        }
    }

    #[cfg(test)]
    #[test]
    fn test_kirufile_round_trip() {
        let plan = sample_plan();
        let text = plan.to_kirufile();
        let parsed = Plan::from_kirufile(&text).expect("should parse");
        assert_eq!(plan, parsed, "round trip mismatch:\n{}", text);
    }

    #[cfg(test)]
    #[test]
    fn test_kirufile_escapes() {
        let mut plan = Plan {
            shell: "sh".to_string(),
            ..Default::default()
        };
        let mut project = Project::default();
        project.functions.insert(
            "weird".to_string(),
            vec![Instruction::Bind {
                value: Template::lit("has \"quotes\" and ) parens"),
            }],
        );
        plan.projects.insert("p".to_string(), project);
        let text = plan.to_kirufile();
        let parsed = Plan::from_kirufile(&text).expect("should parse");
        assert_eq!(
            parsed.projects["p"].functions["weird"][0],
            Instruction::Bind {
                value: Template::lit("has \"quotes\" and ) parens"),
            }
        );
    }
}
