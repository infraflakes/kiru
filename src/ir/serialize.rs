//! Textual kirufile serialization.

use super::types::*;

/// Write a string as a double-quoted, escaped literal (Rust `Debug` quoting).
pub(crate) fn quote_string(s: &str) -> String {
    format!("{:?}", s)
}

/// Append a `Template` as a `(t ...)` s-expression node to `buf`.
fn fmt_template(buf: &mut String, tmpl: &Template) {
    buf.push_str("(t");
    for segment in &tmpl.segments {
        match segment {
            Segment::Literal(s) => buf.push_str(&format!(" (lit {})", quote_string(s))),
            Segment::Command(inner) => {
                buf.push_str(" (cmd ");
                fmt_template(buf, inner);
                buf.push(')');
            }
        }
    }
    buf.push(')');
}

/// Append a single instruction to `buf`. When `indent` is `Some(level)`, a
/// leading indent and trailing newline are added (top-level output). When
/// `None`, the instruction is rendered inline for nested bodies (`env`,
/// `switch`).
fn fmt_instruction(buf: &mut String, inst: &Instruction, indent: Option<usize>) {
    if let Some(level) = indent {
        buf.push_str(&"  ".repeat(level));
    }
    match inst {
        Instruction::Exec { value } => {
            buf.push_str("(exec ");
            fmt_template(buf, value);
            buf.push(')');
        }
        Instruction::Log(value) => {
            buf.push_str("(log ");
            fmt_template(buf, value);
            buf.push(')');
        }
        Instruction::Cd(value) => {
            buf.push_str("(cd ");
            fmt_template(buf, value);
            buf.push(')');
        }
        Instruction::Env { pairs, body } => {
            buf.push_str("(env (");
            for (i, p) in pairs.iter().enumerate() {
                if i > 0 {
                    buf.push(' ');
                }
                buf.push_str(&format!("({} ", p.key));
                fmt_template(buf, &p.value);
                buf.push(')');
            }
            buf.push_str(") (");
            for (i, b) in body.iter().enumerate() {
                if i > 0 {
                    buf.push(' ');
                }
                fmt_instruction(buf, b, None);
            }
            buf.push_str("))");
        }
        Instruction::Switch { subject, arms } => {
            buf.push_str("(switch ");
            fmt_template(buf, subject);
            for arm in arms {
                match &arm.pattern {
                    ArmPattern::Lit(p) => buf.push_str(&format!(" (case {} ", quote_string(p))),
                    ArmPattern::Default => buf.push_str(" (case _ "),
                }
                for (i, b) in arm.body.iter().enumerate() {
                    if i > 0 {
                        buf.push(' ');
                    }
                    fmt_instruction(buf, b, None);
                }
                buf.push(')');
            }
            buf.push(')');
        }
    }
    if indent.is_some() {
        buf.push('\n');
    }
}

impl Ir {
    /// Serialize this IR to the textual kirufile s-expression format.
    pub(crate) fn serialize(&self) -> String {
        let mut out = String::new();
        out.push_str("(kirufile\n");
        out.push_str("  (version 1)\n");
        out.push_str(&format!("  (shell {})\n", quote_string(&self.shell)));
        out.push_str(&format!("  (timeout {})\n", self.timeout));

        for (id, sync) in &self.repositories {
            out.push_str(&format!("  (sync {} (url ", id));
            fmt_template(&mut out, &sync.url);
            out.push_str(") (dir ");
            fmt_template(&mut out, &sync.dir);
            out.push_str(") (branch ");
            fmt_template(&mut out, &sync.branch);
            out.push_str(") (strategy ");
            fmt_template(&mut out, &sync.strategy);
            out.push_str("))\n");
        }

        for (id, project) in &self.projects {
            out.push_str(&format!("  (project {}\n", id));
            for (fn_name, body) in &project.functions {
                out.push_str(&format!("    (fn {}\n", fn_name));
                for inst in body {
                    fmt_instruction(&mut out, inst, Some(3));
                }
                out.push_str("    )\n");
            }
            out.push_str("  )\n");
        }

        for (id, stages) in &self.execution_chains {
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
}

/// Render a template back to its `(t (lit ...) (cmd ...))` s-expression form.
/// Used by `serialize` and by the status/sync printers.
#[cfg(test)]
pub(crate) fn write_template(tmpl: &Template) -> String {
    let mut out = String::new();
    fmt_template(&mut out, tmpl);
    out
}

/// Render a template as a human-readable string for terminal display: literals
/// are shown verbatim and `$(command)` parts are shown by their inner literal
/// text. Commands are never executed here.
pub(crate) fn render_ir_literal(tmpl: &Template) -> String {
    let mut out = String::new();
    for segment in &tmpl.segments {
        match segment {
            Segment::Literal(s) => out.push_str(s),
            Segment::Command(inner) => out.push_str(&render_ir_literal(inner)),
        }
    }
    out
}
