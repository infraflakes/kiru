//! The model node types: the program, its files and namespaces, its
//! declarations and bindings, and the linked expressions and statements.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

use crate::compiler::{Kind, Native};
use crate::syntax::Span;

/// The identity of a loaded file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(crate) struct FileId(pub(crate) usize);

/// The identity of a namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(crate) struct NamespaceId(pub(crate) usize);

/// The identity of a declaration or a local binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(crate) struct DeclarationId(pub(crate) usize);

/// A whole program and the nodes it owns.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Program {
    pub(crate) files: Vec<File>,
    pub(crate) namespaces: Vec<Namespace>,
    pub(crate) declarations: Vec<Declaration>,
    /// The entry function: the entry file's own root `main`.
    pub(crate) entry: DeclarationId,
    /// Module values, keyed by node identity. The compiler ships no values:
    /// the binary evaluates every top-level initializer at startup and stores
    /// the result here. The lock lets `main` and the threads `async`
    /// starts share stored values.
    #[serde(skip)]
    pub(crate) values: RwLock<HashMap<DeclarationId, Value>>,
}

impl Program {
    pub(crate) fn declaration(&self, id: DeclarationId) -> &Declaration {
        &self.declarations[id.0]
    }

    pub(crate) fn declaration_mut(&mut self, id: DeclarationId) -> &mut Declaration {
        &mut self.declarations[id.0]
    }

    pub(crate) fn file(&self, id: FileId) -> &File {
        &self.files[id.0]
    }

    pub(crate) fn namespace(&self, id: NamespaceId) -> &Namespace {
        &self.namespaces[id.0]
    }

    pub(crate) fn root(&self) -> NamespaceId {
        NamespaceId(0)
    }

    /// The namespace path, root first.
    pub(crate) fn namespace_path(&self, id: NamespaceId) -> Vec<String> {
        namespace_path(&self.namespaces, id)
    }

    /// The namespace at a path, when one exists.
    pub(crate) fn namespace_at(&self, path: &[String]) -> Option<NamespaceId> {
        namespace_at(&self.namespaces, path)
    }

    /// The file a declaration came from. A native and a local binding have no
    /// file, so only a caller holding a file declaration asks for this.
    pub(crate) fn file_of(&self, id: DeclarationId) -> FileId {
        self.declaration(id)
            .file
            .expect("a file declaration has a file")
    }

    /// The display name of a declaration: a local binding shows its bare name,
    /// and a file declaration shows its namespace path and name joined with
    /// `::`, with no leading `::`.
    pub(crate) fn display(&self, id: DeclarationId) -> String {
        let declaration = self.declaration(id);
        if declaration.owner.is_some() {
            return declaration.name.clone();
        }
        let mut path = self.namespace_path(declaration.namespace);
        path.push(declaration.name.clone());
        join_path(&path, false)
    }
}

/// Where a loaded file came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Origin {
    /// A file seeded from the compiler's own sources.
    Embedded,
    /// A file read from the filesystem.
    File,
}

/// One loaded file and the nodes it owns.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct File {
    pub(crate) path: PathBuf,
    pub(crate) namespace: NamespaceId,
    pub(crate) imports: Vec<FileId>,
    pub(crate) declarations: Vec<DeclarationId>,
    pub(crate) origin: Origin,
}

/// One namespace node: the value and function registries, plus the parent and
/// child edges. Values are unique by name and functions are unique by name,
/// and a function may share a name with a value.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Namespace {
    pub(crate) name: String,
    pub(crate) parent: Option<NamespaceId>,
    pub(crate) children: HashMap<String, NamespaceId>,
    pub(crate) values: NameTable,
    pub(crate) functions: NameTable,
}

impl Namespace {
    /// A value declared here, if any. A bare name resolves through this.
    pub(crate) fn value(&self, name: &str) -> Option<DeclarationId> {
        self.values.get(name)
    }

    /// A function declared here, if any. A call resolves through this.
    pub(crate) fn function(&self, name: &str) -> Option<DeclarationId> {
        self.functions.get(name)
    }

    /// One name looked up in one registry of this namespace.
    pub(crate) fn lookup(&self, registry: Registry, name: &str) -> Option<DeclarationId> {
        match registry {
            Registry::Value => self.value(name),
            Registry::Function => self.function(name),
        }
    }

    /// One registry of this namespace, for registering a declaration.
    pub(crate) fn table_mut(&mut self, registry: Registry) -> &mut NameTable {
        match registry {
            Registry::Value => &mut self.values,
            Registry::Function => &mut self.functions,
        }
    }
}

/// Which registry of a namespace a name lives in.
#[derive(Clone, Copy)]
pub(crate) enum Registry {
    Value,
    Function,
}

impl Registry {
    /// The other registry of the same namespace.
    pub(crate) fn other(self) -> Registry {
        match self {
            Registry::Value => Registry::Function,
            Registry::Function => Registry::Value,
        }
    }
}

/// One namespace registry: names that are unique within a namespace. A
/// namespace keeps one for values and one for functions, so the duplicate
/// rule lives here once. The table serializes exactly as the map it holds,
/// so a compiled binary stores it without a wrapper.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub(crate) struct NameTable {
    entries: HashMap<String, DeclarationId>,
}

impl NameTable {
    /// Register a name. Returns the id already registered under it when the
    /// name is taken, and registers nothing then.
    pub(crate) fn insert(&mut self, name: String, id: DeclarationId) -> Result<(), DeclarationId> {
        match self.entries.entry(name) {
            std::collections::hash_map::Entry::Occupied(entry) => Err(*entry.get()),
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(id);
                Ok(())
            }
        }
    }

    /// The declaration registered under a name, if any.
    pub(crate) fn get(&self, name: &str) -> Option<DeclarationId> {
        self.entries.get(name).copied()
    }

    /// The registered declaration ids, in no particular order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &DeclarationId> {
        self.entries.values()
    }

    /// Keep only the entries whose id the mapping keeps, and rewrite each
    /// surviving id through it. Retention compacts the declaration table.
    pub(crate) fn remap(&mut self, mapping: &[Option<DeclarationId>]) {
        self.entries.retain(|_, id| mapping[id.0].is_some());
        for id in self.entries.values_mut() {
            if let Some(mapped) = mapping[id.0] {
                *id = mapped;
            }
        }
    }
}

/// The namespace path, root first, walked over a namespace table. The program
/// model and the linker both derive namespace paths through this function.
pub(crate) fn namespace_path(namespaces: &[Namespace], id: NamespaceId) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = Some(id);
    while let Some(namespace_id) = current {
        let namespace = &namespaces[namespace_id.0];
        current = namespace.parent;
        if namespace.parent.is_some() {
            segments.push(namespace.name.clone());
        }
    }
    segments.reverse();
    segments
}

/// Join namespace segments with `::`, with an optional leading `::` for a
/// rooted path. The program model and the linker both render names through
/// this function.
pub(crate) fn join_path(segments: &[String], root: bool) -> String {
    let joined = segments.join("::");
    if root { format!("::{joined}") } else { joined }
}

/// The namespace at a path, walked over a namespace table. The program model
/// and the linker both look namespaces up through this function.
pub(crate) fn namespace_at(namespaces: &[Namespace], path: &[String]) -> Option<NamespaceId> {
    let mut current = NamespaceId(0);
    for segment in path {
        current = *namespaces[current.0].children.get(segment)?;
    }
    Some(current)
}

/// One declaration or binding node.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Declaration {
    pub(crate) name: String,
    pub(crate) name_span: Span,
    /// The file that declared it; natives have none.
    pub(crate) file: Option<FileId>,
    pub(crate) namespace: NamespaceId,
    /// Registration order, for the top-down reference rule.
    pub(crate) order: usize,
    /// The function that owns a parameter or local binding.
    pub(crate) owner: Option<DeclarationId>,
    /// The parameter nodes of a callable, in order. A native and a user
    /// function both carry them, so the checker has one call path.
    pub(crate) parameters: Vec<DeclarationId>,
    pub(crate) kind: DeclarationKind,
    pub(crate) derived: Derived,
}

impl Declaration {
    /// The function body when this node is a function.
    pub(crate) fn function(&self) -> Option<&Function> {
        match &self.kind {
            DeclarationKind::Function(function) => Some(function),
            DeclarationKind::Text(_)
            | DeclarationKind::Record(_)
            | DeclarationKind::Binding(_)
            | DeclarationKind::Native(_) => None,
        }
    }

    /// The function body when this node is a function.
    pub(crate) fn function_mut(&mut self) -> Option<&mut Function> {
        match &mut self.kind {
            DeclarationKind::Function(function) => Some(function),
            DeclarationKind::Text(_)
            | DeclarationKind::Record(_)
            | DeclarationKind::Binding(_)
            | DeclarationKind::Native(_) => None,
        }
    }

    /// The initializer expression when this node is a text or record value.
    pub(crate) fn initializer(&self) -> Option<&Expression> {
        match &self.kind {
            DeclarationKind::Text(expression) | DeclarationKind::Record(expression) => {
                Some(expression)
            }
            DeclarationKind::Function(_)
            | DeclarationKind::Binding(_)
            | DeclarationKind::Native(_) => None,
        }
    }

    /// The initializer expression when this node is a text or record value.
    pub(crate) fn initializer_mut(&mut self) -> Option<&mut Expression> {
        match &mut self.kind {
            DeclarationKind::Text(expression) | DeclarationKind::Record(expression) => {
                Some(expression)
            }
            DeclarationKind::Function(_)
            | DeclarationKind::Binding(_)
            | DeclarationKind::Native(_) => None,
        }
    }
}

/// What a phase has learned about a node.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct Derived {
    /// The value's kind, a binding's kind, or a function's return kind. A
    /// function seeds this from its declaration at link time, so a call reads
    /// the kind the declaration fixed without looking at the body.
    pub(crate) kind: Option<Kind>,
    /// Whether every path of a function ends in `panic;` or a call to a
    /// function that stops the run. A call to such a function ends its
    /// caller's path exactly like `panic;` does.
    pub(crate) halts: bool,
}

/// What a declaration node is.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) enum DeclarationKind {
    Function(Function),
    /// A module-level `txt`; the startup pass evaluates it once and it holds
    /// text.
    Text(Expression),
    /// A module-level `rec`; the startup pass evaluates it once. The
    /// expression is a record literal, a record variable, or a call returning
    /// a record.
    Record(Expression),
    /// A parameter or local binding.
    Binding(BindingKind),
    Native(Native),
}

/// The role of a binding node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum BindingKind {
    /// A function parameter.
    Parameter,
    /// A `txt` binding; it holds text.
    Text,
    /// A `rec` binding.
    Record,
}

/// A function body; its parameters are separate nodes. `return_kind` is the
/// kind the declaration fixes: `Nothing` when no arrow is written.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Function {
    pub(crate) body: Vec<Statement>,
    pub(crate) return_kind: Kind,
}

/// A linked expression. Every name is already an edge to a node.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) enum Expression {
    Text {
        value: String,
        span: Span,
    },
    Record {
        fields: Vec<Field>,
        span: Span,
    },
    /// A reference to a declaration or a binding. The leading `::`, if any,
    /// was consumed by resolution and is not part of the edge.
    Reference {
        declaration: DeclarationId,
        span: Span,
    },
    Call {
        callee: DeclarationId,
        callee_span: Span,
        arguments: Vec<Expression>,
        span: Span,
    },
    Field {
        target: Box<Expression>,
        name: String,
        name_span: Span,
        span: Span,
    },
    Add {
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },
}

impl Expression {
    pub(crate) fn span(&self) -> Span {
        match self {
            Expression::Text { span, .. }
            | Expression::Record { span, .. }
            | Expression::Reference { span, .. }
            | Expression::Call { span, .. }
            | Expression::Field { span, .. }
            | Expression::Add { span, .. } => *span,
        }
    }
}

/// One `key = expression` entry of a record literal.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Field {
    pub(crate) name: String,
    pub(crate) name_span: Span,
    pub(crate) value: Expression,
    pub(crate) span: Span,
}

/// A linked statement.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) enum Statement {
    /// A local `txt` or `rec` binding and its initializer.
    Bind {
        declaration: DeclarationId,
        value: Expression,
        span: Span,
    },
    Assign {
        declaration: DeclarationId,
        name_span: Span,
        value: Expression,
        span: Span,
    },
    Expression(Expression),
    /// An early exit. A value return carries the function's declared kind; a
    /// valueless return ends a function that declares no return kind.
    Return {
        value: Option<Expression>,
        span: Span,
    },
    /// The keyword statement `panic;`, which ends the run.
    Panic {
        span: Span,
    },
    /// The keyword statement `async <call>;`, which spawns the call.
    Async {
        call: Expression,
        span: Span,
    },
    /// The keyword statement `wait;`, which joins the calling thread's asyncs.
    Wait {
        span: Span,
    },
    Switch {
        subject: Expression,
        cases: Vec<Case>,
        default: Option<Vec<Statement>>,
        span: Span,
    },
    Defer {
        body: Vec<Statement>,
        span: Span,
    },
}

impl Statement {
    /// The span one statement covers. A bare expression statement has no span
    /// of its own, so it covers the expression it discards.
    pub(crate) fn span(&self) -> Span {
        match self {
            Statement::Expression(expression) => expression.span(),
            Statement::Bind { span, .. }
            | Statement::Assign { span, .. }
            | Statement::Return { span, .. }
            | Statement::Panic { span }
            | Statement::Async { span, .. }
            | Statement::Wait { span }
            | Statement::Switch { span, .. }
            | Statement::Defer { span, .. } => *span,
        }
    }
}

/// One `case(pattern) { ... };` arm of a switch.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Case {
    pub(crate) pattern: Expression,
    pub(crate) body: Vec<Statement>,
    pub(crate) span: Span,
}

/// A runtime value: text, a record of text, or the no-value result of a call
/// that returns nothing.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Value {
    Text(String),
    Record(Record),
    Nothing,
}

impl Kind {
    /// The kind a runtime value carries. This is the dynamic counterpart of
    /// the checker's static kind: a value only ever holds one concrete kind.
    pub(crate) fn of(value: &Value) -> Kind {
        match value {
            Value::Text(_) => Kind::Text,
            Value::Record(_) => Kind::Record,
            Value::Nothing => Kind::Nothing,
        }
    }
}

/// A record of text fields in insertion order. A missing key reads as `""`.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Record {
    entries: Vec<(String, String)>,
}

impl Record {
    /// Build a record from key/value pairs in order. A later duplicate key
    /// replaces an earlier one, exactly as `set` does.
    pub(crate) fn from_pairs<I>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let mut record = Self::default();
        for (key, value) in pairs {
            record.set(key, value);
        }
        record
    }

    /// Replace any value already stored for `key`.
    pub(crate) fn set(&mut self, key: String, value: String) {
        match self.entries.iter_mut().find(|(name, _)| *name == key) {
            Some(entry) => entry.1 = value,
            None => self.entries.push((key, value)),
        }
    }

    /// Read a field, or `""` when it is absent.
    pub(crate) fn get(&self, key: &str) -> &str {
        self.entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
            .unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A value that carries a concrete kind, for the round-trip test.
    fn value_of(kind: Kind) -> Value {
        match kind {
            Kind::Text => Value::Text(String::new()),
            Kind::Record => Value::Record(Record::default()),
            Kind::Nothing => Value::Nothing,
        }
    }

    #[test]
    fn every_concrete_kind_round_trips_through_a_value() {
        for kind in [Kind::Text, Kind::Record, Kind::Nothing] {
            assert_eq!(
                Kind::of(&value_of(kind)),
                kind,
                "{kind:?} did not round trip"
            );
        }
    }
}
