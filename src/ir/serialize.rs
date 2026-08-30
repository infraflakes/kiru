//! Textual kirufile serialization.

use super::types::*;

/// Write a string as a double-quoted, escaped literal (Rust `Debug` quoting).
pub fn quote_string(s: &str) -> String {
    format!("{:?}", s)
}

/// Append a `Template` as a `(t ...)` s-expression node to `out`.
fn append_template(buf: &mut String, tmpl: &Template) {
    buf.push_str("(t");
    for segment in &tmpl.segments {
        match segment {
            Segment::Literal(s) => buf.push_str(&format!(" (lit {})", quote_string(s))),
            Segment::Command(inner) => {
                buf.push_str(" (cmd");
                append_template(buf, inner);
                buf.push(')');
            }
        }
    }
    buf.push(')');
}

/// Write a single instruction without a trailing newline (used inside `env`/
/// `switch` bodies which are inline lists).
fn write_instruction_inline(inst: &Instruction, _indent: usize) -> String {
    match inst {
        Instruction::Exec { value } => {
            format!("(exec {})", write_template(value))
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

fn write_instructions(insts: &[Instruction], indent: usize) -> String {
    let pad: String = "  ".repeat(indent);
    let mut out = String::new();
    for inst in insts {
        match inst {
            Instruction::Exec { value } => {
                out.push_str(&format!("{}(exec {})\n", pad, write_template(value)));
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

impl Ir {
    /// Serialize this IR to the textual kirufile s-expression format.
    pub fn serialize(&self) -> String {
        let mut out = String::new();
        out.push_str("(kirufile\n");
        out.push_str("  (version 1)\n");
        out.push_str(&format!("  (shell {})\n", quote_string(&self.shell)));

        for (id, sync) in &self.repositories {
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
pub fn write_template(tmpl: &Template) -> String {
    let mut out = String::from("(t");
    for segment in &tmpl.segments {
        match segment {
            Segment::Literal(s) => out.push_str(&format!(" (lit {})", quote_string(s))),
            Segment::Command(inner) => out.push_str(&format!(" (cmd {})", write_template(inner))),
        }
    }
    out.push(')');
    out
}

/// Render a template as a human-readable string for terminal display: literals
/// are shown verbatim and `$(command)` parts are shown by their inner literal
/// text. Commands are never executed here.
pub fn render_ir_literal(tmpl: &Template) -> String {
    let mut out = String::new();
    for segment in &tmpl.segments {
        match segment {
            Segment::Literal(s) => out.push_str(s),
            Segment::Command(inner) => out.push_str(&render_ir_literal(inner)),
        }
    }
    out
}
