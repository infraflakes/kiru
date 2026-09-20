use super::MAX_PANEL_HEIGHT;
use crate::exec::colors;
use crate::exec::model::{Display, TaskStatus};
use crate::ir::{NodeId, NodeKind, Program};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

/// One visible row produced by the view walk.
struct RowInfo {
    node: NodeId,
    depth: usize,
    label: String,
    project: Option<String>,
    prefix: String,
    output_prefix: String,
}

/// Collect every visible row of a run in display order. A skipped subtree is
/// pruned; structural nodes (`switch`, project) contribute no row of their
/// own, they only group the rows underneath them. This is the single walk
/// behind the live view and the final dump.
fn collect_rows(
    program: &Program,
    display: &Display,
    children: &[NodeId],
    depth: usize,
    project: Option<String>,
    rows: &mut Vec<RowInfo>,
) {
    for &node in children {
        let state = display.state(node);
        if state.status == TaskStatus::Skipped {
            continue;
        }
        let kind = &program.node(node).kind;
        // A resolved annotation on this row wins over the inherited one.
        let inherited = state.project.clone().or_else(|| project.clone());
        match kind {
            NodeKind::Project(template) => {
                // An empty name has no annotation to show; brackets would
                // just be noise.
                let annotation = template.plan_text(&program.vars);
                let annotation = if annotation.is_empty() {
                    None
                } else {
                    Some(annotation)
                };
                collect_rows(
                    program,
                    display,
                    &program.node(node).children,
                    depth,
                    annotation,
                    rows,
                );
            }
            NodeKind::Switch(_) => {
                collect_rows(
                    program,
                    display,
                    &program.node(node).children,
                    depth,
                    project.clone(),
                    rows,
                );
            }
            _ => {
                let label = state
                    .label
                    .clone()
                    .or_else(|| kind.row_label(&program.vars))
                    .unwrap_or_default();
                rows.push(RowInfo {
                    node,
                    depth,
                    label,
                    project: inherited.clone(),
                    prefix: String::new(),
                    output_prefix: String::new(),
                });
                if matches!(kind, NodeKind::Env(_) | NodeKind::Async | NodeKind::Arm(_)) {
                    collect_rows(
                        program,
                        display,
                        &program.node(node).children,
                        depth + 1,
                        inherited,
                        rows,
                    );
                }
            }
        }
    }
}

/// Assign tree-branch prefixes to the flat rows. A row is the last child of
/// its parent when the next row at its depth or shallower closes its
/// subtree; ancestor continuation bars come from the open rows above it.
fn assign_prefixes(rows: &mut [RowInfo]) {
    let count = rows.len();
    let mut is_last = vec![false; count];
    for (index, row) in rows.iter().enumerate() {
        let mut next = index + 1;
        while next < count && rows[next].depth > row.depth {
            next += 1;
        }
        is_last[index] = next >= count || rows[next].depth < row.depth;
    }

    let mut open: Vec<bool> = Vec::new();
    for index in 0..count {
        let depth = rows[index].depth;
        open.truncate(depth);
        let mut prefix = String::new();
        for &last in &open {
            prefix.push_str(if last { "   " } else { "│  " });
        }
        let output_prefix = format!("{prefix}{}  ", if is_last[index] { "   " } else { "│  " });
        // The outermost level is the run body itself: no branch there, the
        // statements just list top-down. Deeper levels draw connectors.
        if depth > 0 {
            prefix.push_str(if is_last[index] { "└─ " } else { "├─ " });
        }
        rows[index].prefix = prefix;
        rows[index].output_prefix = output_prefix;
        open.push(is_last[index]);
    }
}

/// Every row of a run that is part of what ran, in display order.
fn visible_rows(program: &Program, display: &Display, run: &str) -> Vec<RowInfo> {
    let mut rows = Vec::new();
    collect_rows(
        program,
        display,
        program.run_children(run),
        0,
        None,
        &mut rows,
    );
    assign_prefixes(&mut rows);
    rows
}

/// Render the TUI frame for a `run` command: every visible row with its tree
/// prefix, status glyph, label, and project annotation.
pub(crate) fn render_run_output(
    frame: &mut Frame,
    program: &Program,
    display: &Display,
    run: &str,
    spinner_idx: usize,
) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    if area.height < 1 {
        return;
    }

    let bottom = area.y + area.height;
    let rows = visible_rows(program, display, run);
    for (y_pos, row) in (area.y..).zip(rows.iter()) {
        if y_pos >= bottom {
            break;
        }
        let state = display.state(row.node);
        let line = format!(
            "{}{} {}{}",
            row.prefix,
            state.status.glyph(spinner_idx),
            row.label,
            row.project
                .as_deref()
                .map(|project| format!(" [{project}]"))
                .unwrap_or_default()
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                line,
                Style::default().fg(state.status.color()),
            ))),
            Rect::new(area.x, y_pos, area.width, 1),
        );
    }
}

/// Append one row's final output (status marker, label, and output lines)
/// to the buffer, prefixed so the tree structure stays visible.
fn format_row_output(buf: &mut String, display: &Display, row: &RowInfo) {
    let state = display.state(row.node);
    let project = row
        .project
        .as_deref()
        .map(|project| format!(" [{project}]"))
        .unwrap_or_default();

    buf.push_str(&row.prefix);
    buf.push_str(state.status.ansi());
    buf.push(state.status.glyph(0));
    buf.push_str(colors::RESET);
    buf.push(' ');
    buf.push_str(&row.label);
    buf.push_str(&project);
    buf.push('\n');

    if !state.output.is_empty() {
        let total = state.output.len();
        let visible_lines = total.min(MAX_PANEL_HEIGHT);
        let hidden_lines = total - visible_lines;

        if hidden_lines > 0 {
            buf.push_str(&row.output_prefix);
            buf.push_str(colors::GRAY_ANSI);
            buf.push('↑');
            buf.push(' ');
            buf.push_str(&hidden_lines.to_string());
            buf.push_str(" lines hidden ");
            buf.push_str(colors::RESET);
            buf.push('\n');
        }

        for output_line in state.output.iter().rev().take(visible_lines).rev() {
            buf.push_str(&row.output_prefix);
            buf.push_str(&colors::colored_line_string(output_line));
            buf.push('\n');
        }
    }
}

/// Build the final ANSI-colored text dump after all rows complete: the
/// executed structure (async/env groups and the taken switch arms) with
/// statuses and each row's output hanging underneath. The branches are what
/// make concurrency and nesting visible.
pub(crate) fn format_final_output(program: &Program, display: &Display, run: &str) -> String {
    let mut buf = String::new();
    buf.push('\n');

    for row in visible_rows(program, display, run) {
        format_row_output(&mut buf, display, &row);
    }

    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ArmPattern, NodeKind, ProgramBuilder, Template};
    use std::collections::BTreeMap;

    fn lit(s: &str) -> Template {
        Template::lit(s)
    }

    /// The outermost level lists plain (no branch), nested lines draw
    /// connectors, and ancestor bars continue through non-last blocks.
    #[test]
    fn dump_renders_the_settled_tree_structure() {
        let mut build = ProgramBuilder::default();
        let child_a = build.push(NodeKind::Log(lit("a")), vec![]);
        let group_a = build.push(NodeKind::Async, vec![child_a]);
        let child_b = build.push(NodeKind::Log(lit("b")), vec![]);
        let group_b = build.push(NodeKind::Async, vec![child_b]);
        let program = build.build(BTreeMap::from([("d".to_string(), vec![group_a, group_b])]));
        let display = Display::for_program(&program);

        let dump = format_final_output(&program, &display, "d");
        let lines: Vec<&str> = dump.lines().filter(|line| !line.is_empty()).collect();
        assert_eq!(lines.len(), 4, "one line per row: {dump:?}");
        assert!(lines[0].contains("async"), "{}", lines[0]);
        assert!(
            !lines[0].contains("├─") && !lines[0].contains("└─"),
            "the outermost level has no branch: {}",
            lines[0]
        );
        assert!(
            lines[1].contains("└─") && lines[1].contains("log: a"),
            "{}",
            lines[1]
        );
        assert!(lines[2].contains("async"), "{}", lines[2]);
        assert!(
            lines[3].contains("└─") && lines[3].contains("log: b"),
            "{}",
            lines[3]
        );
    }

    /// A skipped switch arm is pruned with everything under it, and the
    /// remaining tree repairs its branches.
    #[test]
    fn skipped_subtrees_are_hidden_from_the_dump() {
        let mut build = ProgramBuilder::default();
        let taken_log = build.push(NodeKind::Log(lit("taken")), vec![]);
        let taken = build.push(
            NodeKind::Arm(ArmPattern::Lit("a".to_string())),
            vec![taken_log],
        );
        let untaken_log = build.push(NodeKind::Log(lit("untaken")), vec![]);
        let untaken = build.push(NodeKind::Arm(ArmPattern::Default), vec![untaken_log]);
        let switch = build.push(NodeKind::Switch(lit("a")), vec![taken, untaken]);
        let tail = build.push(NodeKind::Log(lit("after")), vec![]);
        let program = build.build(BTreeMap::from([("d".to_string(), vec![switch, tail])]));
        let mut display = Display::for_program(&program);
        display.set_status(taken, TaskStatus::Success);
        display.set_status(taken_log, TaskStatus::Success);
        display.set_status(untaken, TaskStatus::Skipped);
        display.set_status(untaken_log, TaskStatus::Skipped);
        display.set_status(tail, TaskStatus::Success);

        let dump = format_final_output(&program, &display, "d");
        assert!(
            !dump.contains("untaken"),
            "hidden arm stays hidden: {dump:?}"
        );
        let lines: Vec<&str> = dump.lines().filter(|line| !line.is_empty()).collect();
        assert_eq!(lines.len(), 3, "{dump:?}");
        assert!(lines[0].contains("switch case a"), "{}", lines[0]);
        assert!(
            lines[1].contains("└─") && lines[1].contains("log: taken"),
            "{}",
            lines[1]
        );
        assert!(lines[2].contains("log: after"), "{}", lines[2]);
    }

    /// A resolved label and project replace the plan text in every view.
    #[test]
    fn resolved_values_render_in_the_dump() {
        let mut build = ProgramBuilder::default();
        let log = build.push(NodeKind::Log(lit("$(date)")), vec![]);
        let project = build.push(NodeKind::Project(lit("$(echo app)")), vec![log]);
        let program = build.build(BTreeMap::from([("d".to_string(), vec![project])]));
        let mut display = Display::for_program(&program);
        display.set_label(log, "log: Fri".to_string());
        display.set_project(log, "app".to_string());

        let dump = format_final_output(&program, &display, "d");
        assert!(dump.contains("log: Fri"), "{dump:?}");
        assert!(dump.contains("[app]"), "{dump:?}");
    }
}
