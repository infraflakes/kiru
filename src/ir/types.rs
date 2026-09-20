//! Type definitions for the execution IR.
//!
//! The compiler lowers every run into one [`Program`]: an arena of [`Node`]s
//! plus the root children of each run. A [`NodeId`] is the node's identity in
//! that arena, not a number derived from traversal order, so the same id
//! addresses a node in the program and its state in the runtime display.
//!
//! Everything is a resolved `String` or a [`Template`]: there is no type or
//! operator system, the DSL is an IaC task runner. `@(var)` references no
//! longer exist here; the compiler inlines them before the IR is built, so
//! only literal text, `$(command)` substitutions, and variable identities
//! remain. Each variable's value is stored once in [`Program::vars`]; a
//! reference carries only its identity.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The identity of one variable declaration. Every `@(name)` reference to a
/// variable carries its identity, so the runtime can compute the variable's
/// value once and reuse the text instead of running its commands again.
pub(crate) type VarId = usize;

/// A single piece of a [`Template`].
///
/// - `Lit` is literal text.
/// - `Cmd` is a `$(command)` substitution whose inner template is run through
///   `shell -c` at runtime and replaced by its captured stdout.
/// - `Ref` is one variable declaration, addressed by identity. The value
///   lives once in [`Program::vars`]; the runtime resolves it on first use
///   and reuses the text afterwards.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum Segment {
    Lit(String),
    /// A `$(command)` substitution. The inner template is run through `shell -c`
    /// at runtime.
    Cmd(Template),
    /// A tagged variable reference: the identity whose value is stored in
    /// [`Program::vars`].
    Ref(VarId),
}

/// A template: the single string-valued form in the DSL.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub(crate) struct Template {
    pub(crate) parts: Vec<Segment>,
}

impl Template {
    /// A template consisting of a single literal string. Test-only helper.
    #[cfg(test)]
    pub(crate) fn lit(s: &str) -> Self {
        Template {
            parts: vec![Segment::Lit(s.to_string())],
        }
    }

    /// The plan preview of the template: its literal text with every
    /// `$(command)` kept verbatim, because parenthesized content is data,
    /// not an operator to summarize. A variable reference shows the value it
    /// stands for, exactly as the inliner would have spliced it.
    pub(crate) fn plan_text(&self, vars: &[Template]) -> String {
        self.parts
            .iter()
            .map(|segment| match segment {
                Segment::Lit(text) => text.clone(),
                Segment::Cmd(inner) => format!("$({})", inner.plan_text(vars)),
                Segment::Ref(id) => vars[*id].plan_text(vars),
            })
            .collect()
    }

    /// Whether the template resolves only at runtime: any `$()` part means
    /// its value is not visible in the plan label. A variable reference is
    /// as dynamic as the value it stands for.
    pub(crate) fn is_dynamic(&self, vars: &[Template]) -> bool {
        self.parts.iter().any(|segment| match segment {
            Segment::Cmd(_) => true,
            Segment::Ref(id) => vars[*id].is_dynamic(vars),
            Segment::Lit(_) => false,
        })
    }
}

/// A single resolved `env` block pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct EnvPair {
    pub(crate) key: String,
    pub(crate) value: Template,
}

/// A pattern arm inside a `switch` block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum ArmPattern {
    /// A literal string to match the resolved subject against.
    Lit(String),
    /// The `default` arm.
    Default,
}

/// A node's identity in the [`Program`] arena. Stable across serialization;
/// the runtime display keeps one state per node under the same id.
pub(crate) type NodeId = usize;

/// One statement or block of the program: its kind and the child nodes that
/// belong to it. Containers (`env`, `async`, arm, project) own their body as
/// children; leaves have none; a `switch` owns its arms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Node {
    pub(crate) kind: NodeKind,
    pub(crate) children: Vec<NodeId>,
}

/// What a node does. The kind doubles as the display identity: its label
/// derives from the kind and the templates inside it, never from stored text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum NodeKind {
    /// Run the resolved `command` template through `shell -c` with live
    /// output; a non-zero exit aborts.
    Exec(Template),
    /// Emit `value` to the output log.
    Log(Template),
    /// Change the working directory to the resolved `value`.
    Cd(Template),
    /// Export `pairs` to the command subprocess environment for the duration
    /// of the child nodes.
    Env(Vec<EnvPair>),
    /// Run the children now and join them at the end of the enclosing body.
    /// The children execute on a copy of the current context (working
    /// directory, environment layers, direnv wrapping), so changes inside
    /// never leak out.
    Async,
    /// Match the resolved subject against each arm child's pattern; the
    /// first match runs. No display row of its own: the arms are rows.
    Switch(Template),
    /// One `switch` arm: a display row whose children are the arm body.
    Arm(ArmPattern),
    /// Execute the children in a project's context: the directory and direnv
    /// setting from `kiru.toml`. No display row of its own; its resolved
    /// name annotates every row underneath it.
    Project(Template),
}

impl NodeKind {
    /// The keyword a self-naming statement shows before its colon, or `None`
    /// for block and arm kinds that name themselves differently.
    fn keyword(&self) -> Option<&'static str> {
        match self {
            NodeKind::Log(_) => Some("log"),
            NodeKind::Exec(_) => Some("exec"),
            NodeKind::Cd(_) => Some("cd"),
            NodeKind::Env(_) => Some("env"),
            _ => None,
        }
    }

    /// The resolved display label of a self-naming statement: `keyword:
    /// data`. This is the runtime override used once the values are known.
    pub(crate) fn resolved_label(&self, data: &str) -> Option<String> {
        Some(format!("{}: {data}", self.keyword()?))
    }

    /// The label this node shows as a display row, or `None` when the node
    /// is structural (a `switch` or a project context) and only groups rows.
    pub(crate) fn row_label(&self, vars: &[Template]) -> Option<String> {
        match self {
            NodeKind::Log(t) | NodeKind::Exec(t) | NodeKind::Cd(t) => {
                self.resolved_label(&t.plan_text(vars))
            }
            NodeKind::Env(pairs) => {
                let keys: Vec<&str> = pairs.iter().map(|pair| pair.key.as_str()).collect();
                self.resolved_label(&keys.join(", "))
            }
            NodeKind::Async => Some("async".to_string()),
            NodeKind::Arm(ArmPattern::Lit(pattern)) => Some(if pattern.is_empty() {
                "switch case \"\"".to_string()
            } else {
                format!("switch case {pattern}")
            }),
            NodeKind::Arm(ArmPattern::Default) => Some("switch default".to_string()),
            NodeKind::Switch(_) | NodeKind::Project(_) => None,
        }
    }
}

/// The compiled form of every run: the root children of each run plus the
/// arena holding every node they reference. This is the compiler's only
/// outward contract and the executor's only input.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub(crate) struct Program {
    pub(crate) runs: BTreeMap<String, Vec<NodeId>>,
    pub(crate) nodes: Vec<Node>,
    /// Every variable declaration's value, addressed by [`VarId`]. Stored
    /// once no matter how many references use it.
    pub(crate) vars: Vec<Template>,
}

impl Program {
    /// The node at `id`. The executor and renderers address nodes by id, so
    /// every id they hold came from this arena and is in range.
    pub(crate) fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    /// The value of the variable declaration with this identity.
    pub(crate) fn var(&self, id: VarId) -> &Template {
        &self.vars[id]
    }

    /// Visit every node in the subtree rooted at `roots` in pre-order,
    /// including the roots themselves. Used for status marking and project
    /// annotation, so it must reach structural nodes too, not just rows.
    pub(crate) fn visit_subtree(&self, roots: &[NodeId], visit: &mut impl FnMut(NodeId)) {
        for &root in roots {
            visit(root);
            self.visit_subtree(&self.node(root).children, visit);
        }
    }

    /// How many display rows one run starts with: its subtree's nodes that
    /// become lines. Structural nodes (`switch`, project) group rows without
    /// becoming one. This sizes the live viewport, so it must count the run
    /// being shown, never the whole program.
    pub(crate) fn run_row_count(&self, run: &str) -> usize {
        let mut count = 0;
        self.visit_subtree(self.run_children(run), &mut |id| {
            if self.node(id).kind.row_label(&self.vars).is_some() {
                count += 1;
            }
        });
        count
    }

    /// The root children of one run, or an empty slice for an unknown run.
    pub(crate) fn run_children(&self, run: &str) -> &[NodeId] {
        self.runs.get(run).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Serialize this program to its textual RON form. The format is
    /// derived by serde, so the codec can never drift from these type
    /// definitions.
    pub(crate) fn serialize(&self) -> String {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .expect("the program only contains serializable data")
    }

    /// Parse a textual RON program.
    pub(crate) fn deserialize(src: &str) -> Result<Program, String> {
        ron::from_str(src).map_err(|e| e.to_string())
    }
}

/// Builds the node arena while the compiler lowers runs. Nodes may be pushed
/// in any order: ids are identity, not traversal position.
#[derive(Debug, Default)]
pub(crate) struct ProgramBuilder {
    nodes: Vec<Node>,
    vars: Vec<Template>,
}

impl ProgramBuilder {
    /// Append one node and return its id.
    pub(crate) fn push(&mut self, kind: NodeKind, children: Vec<NodeId>) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node { kind, children });
        id
    }

    /// Store one variable declaration's value and return its identity.
    /// Called once per declaration, never per reference.
    pub(crate) fn push_var(&mut self, value: Template) -> VarId {
        let id = self.vars.len();
        self.vars.push(value);
        id
    }

    /// Freeze the arena together with the run roots and the variable values.
    pub(crate) fn build(self, runs: BTreeMap<String, Vec<NodeId>>) -> Program {
        Program {
            runs,
            nodes: self.nodes,
            vars: self.vars,
        }
    }
}
