//! The completions a position earns.
//!
//! A completion list is what exists at the position, not what the server
//! thinks should be written: the items are a name and the kind the model
//! marks it with, and the editor sorts and filters. The position's context
//! is what the source stands under the cursor: a path separator names a
//! namespace to read, a dot names a value whose type the model checked, and
//! a bare prefix names the file's own namespace. One snapshot answers one
//! request, and the list is never partial.

use std::collections::HashSet;
use std::path::Path;

use crate::compiler::build::Build;
use crate::compiler::model::{
    DeclarationKind, FileId, Group, NamespaceId, Type, TypeDecl, TypeKind, UseTarget, reachable,
};
use crate::compiler::reads::{Read, collect_reads};
use crate::syntax::{Span, is_identifier_continue};

/// One completion: the name and the kind the model marks it with.
#[derive(Debug)]
pub struct Completion {
    pub name: String,
    pub kind: CompletionKind,
}

/// What a completion names, by the model's own structure. The boundary maps
/// these to the protocol's item kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionKind {
    /// A callable name: a declared function or a native.
    Function,
    /// A declared `rec` type.
    Rec,
    /// A declared `enum` type.
    Enum,
    /// A namespace a path may descend through.
    Namespace,
    /// A name that holds a value: a module value.
    Variable,
    /// One field of a declared `rec` type.
    Field,
    /// One variant of a declared `enum` type.
    Variant,
}

/// What the position stands under.
enum Context {
    /// A path's prefix is written: its segments, and the prefix typed after
    /// the separator.
    Qualified {
        segments: Vec<String>,
        prefix: String,
    },
    /// A dot stands before the cursor, and the word before it names a value.
    Dot { receiver: Span, prefix: String },
    /// The file's namespace's own names and deliveries.
    Bare { prefix: String },
}

impl Build {
    /// The completions the position in `path` earns: the names that exist at
    /// the position, each with its kind. `text` is the source as it stands
    /// now, which may not be the checked source: a client asks while a line is
    /// half written, so the context reads the current text and the names come
    /// from the checked model. A path the program reads no file by earns
    /// none.
    pub fn completions(&self, path: &Path, text: &str, offset: usize) -> Vec<Completion> {
        let Some(file) = self.program.files.iter().position(|file| file.path == path) else {
            return Vec::new();
        };
        let file = FileId(file);
        let Some(context) = context_at(text, offset) else {
            return Vec::new();
        };
        let from = self.program.files[file.0].namespace;
        match context {
            Context::Qualified { segments, prefix } => {
                self.namespace_items(from, &segments, &prefix)
            }
            Context::Dot { receiver, prefix } => self.member_items(file, receiver, &prefix),
            Context::Bare { prefix } => self.bare_items(from, &prefix),
        }
    }

    /// The names a namespace offers a caller: its own reachable names and
    /// declared types, the namespaces it is parent of, and the prefix the
    /// editor typed.
    fn namespace_items(
        &self,
        from: NamespaceId,
        segments: &[String],
        prefix: &str,
    ) -> Vec<Completion> {
        let Some(target) = self.program.qualified_namespace(from, segments) else {
            return Vec::new();
        };
        let mut seen = HashSet::new();
        let mut items = Vec::new();
        let mut offer = |name: String, kind: CompletionKind| {
            if name.starts_with(prefix) && seen.insert(name.clone()) {
                items.push(Completion { name, kind });
            }
        };
        let own_names = self.program.namespace(target).names.iter().copied();
        for id in own_names {
            let declaration = self.program.declaration(id);
            if !reachable(declaration.public, target, from) {
                continue;
            }
            let kind = match declaration.kind {
                DeclarationKind::Function | DeclarationKind::Native => CompletionKind::Function,
                DeclarationKind::ModuleValue => CompletionKind::Variable,
                // A name pool holds written names: a local is written in no
                // file, a type is no node, and an impl block is flattened
                // into its members.
                DeclarationKind::Local
                | DeclarationKind::Rec
                | DeclarationKind::Enum
                | DeclarationKind::Impl => unreachable!("a name pool holds written names"),
            };
            offer(declaration.name.clone(), kind);
        }
        let own_types = self.program.types.iter().filter(|type_decl| {
            type_decl.namespace == target && reachable(type_decl.public, target, from)
        });
        for type_decl in own_types {
            offer(type_decl.name.clone(), declared_kind(type_decl));
        }
        for child in self
            .program
            .namespaces
            .iter()
            .filter(|space| space.parent == Some(target))
        {
            offer(child.name.clone(), CompletionKind::Namespace);
        }
        items
    }

    /// The names the file's namespace offers bare: its own names, and the
    /// deliveries its `use`s recorded.
    fn bare_items(&self, from: NamespaceId, prefix: &str) -> Vec<Completion> {
        let mut seen = HashSet::new();
        let mut items = Vec::new();
        let mut offer = |name: String, kind: CompletionKind| {
            if name.starts_with(prefix) && seen.insert(name.clone()) {
                items.push(Completion { name, kind });
            }
        };
        let own_names = self.program.namespace(from).names.iter().copied();
        for id in own_names {
            let declaration = self.program.declaration(id);
            let kind = match declaration.kind {
                DeclarationKind::Function | DeclarationKind::Native => CompletionKind::Function,
                DeclarationKind::ModuleValue => CompletionKind::Variable,
                DeclarationKind::Local
                | DeclarationKind::Rec
                | DeclarationKind::Enum
                | DeclarationKind::Impl => unreachable!("a name pool holds written names"),
            };
            offer(declaration.name.clone(), kind);
        }
        // The namespace's own declared types: a bare name resolves them by
        // the same pool the qualified form reads, so they are offered too.
        let own_types = self
            .program
            .types
            .iter()
            .filter(|type_decl| type_decl.namespace == from);
        for type_decl in own_types {
            offer(type_decl.name.clone(), declared_kind(type_decl));
        }
        let deliveries = self.program.namespace(from).brought.values();
        for entries in deliveries {
            // The first entry that fits is the one a read would take.
            let Some(delivery) = entries.iter().find(|entry| match entry.target {
                UseTarget::Function(declaration) => {
                    let node = self.program.declaration(declaration);
                    reachable(node.public, node.namespace, from)
                }
                _ => true,
            }) else {
                continue;
            };
            let (name, kind) = match delivery.target {
                UseTarget::Function(declaration) => (
                    self.program.declaration(declaration).name.clone(),
                    CompletionKind::Function,
                ),
                UseTarget::Type(id) => {
                    let type_decl = &self.program.types[id.0];
                    (type_decl.name.clone(), declared_kind(type_decl))
                }
                UseTarget::Namespace(id) => (
                    self.program.namespace(id).name.clone(),
                    CompletionKind::Namespace,
                ),
            };
            offer(name, kind);
        }
        items
    }

    /// The names a value's type declares: a rec's fields, an enum's variants.
    /// The type is the one the check settled for the receiver the dot stands
    /// after, when the receiver names a declaration the model holds.
    fn member_items(&self, file: FileId, receiver: Span, prefix: &str) -> Vec<Completion> {
        let reads = collect_reads(&self.program);
        let covers = |read: &Read| {
            read.file == file && read.span.start <= receiver.start && receiver.end <= read.span.end
        };
        let names = |write: &crate::compiler::reads::Write| {
            write.file == file
                && write.span.start <= receiver.start
                && receiver.end <= write.span.end
        };
        let target = reads
            .reads
            .iter()
            .find(|read| covers(read))
            .map(|read| read.declaration)
            .or_else(|| {
                reads
                    .writes
                    .iter()
                    .find(|write| names(write))
                    .map(|write| write.declaration)
            });
        let Some(target) = target else {
            return Vec::new();
        };
        let Some(id) = (match &self.program.declaration(target).ty {
            Type::Named(id, _) => Some(*id),
            _ => None,
        }) else {
            return Vec::new();
        };
        match &self.program.types[id.0].kind {
            TypeKind::Rec(fields) => fields
                .iter()
                .filter(|field| field.name.starts_with(prefix))
                .map(|field| Completion {
                    name: field.name.clone(),
                    kind: CompletionKind::Field,
                })
                .collect(),
            TypeKind::Enum(variants) => variants
                .iter()
                .filter(|variant| variant.name.starts_with(prefix))
                .map(|variant| Completion {
                    name: variant.name.clone(),
                    kind: CompletionKind::Variant,
                })
                .collect(),
        }
    }
}

/// The completion kind a declared type earns, by its own group.
fn declared_kind(type_decl: &TypeDecl) -> CompletionKind {
    match type_decl.group() {
        Group::Rec => CompletionKind::Rec,
        Group::Enum => CompletionKind::Enum,
        Group::Scalar | Group::Function | Group::Parameter => {
            unreachable!("a declared type is a rec or an enum")
        }
    }
}

/// The context the source stands under the cursor: the path separator, the
/// dot, or the bare prefix written before it.
fn context_at(source: &str, offset: usize) -> Option<Context> {
    let before = &source[..offset.min(source.len())];
    let typed = before.len() - before.trim_end_matches(is_identifier_continue).len();
    if typed > 0 {
        let head = before[..before.len() - typed].trim_end();
        let prefix = before[before.len() - typed..].to_owned();
        if let Some(segments) = path_chain(head) {
            return Some(Context::Qualified { segments, prefix });
        }
        if head.ends_with('.') {
            let before_dot = &before[..head.len() - 1].trim_end();
            return trailing_word(before_dot).map(|receiver| Context::Dot { receiver, prefix });
        }
        return Some(Context::Bare { prefix });
    }
    let trimmed = before.trim_end();
    if let Some(segments) = path_chain(trimmed) {
        return Some(Context::Qualified {
            segments,
            prefix: String::new(),
        });
    }
    if trimmed.ends_with('.') {
        let before_dot = &before[..trimmed.len() - 1].trim_end();
        return trailing_word(before_dot).map(|receiver| Context::Dot {
            receiver,
            prefix: String::new(),
        });
    }
    Some(Context::Bare {
        prefix: String::new(),
    })
}

/// The path segments a text ends with, when it ends with a path separator
/// and every segment between the separators is written.
fn path_chain(text: &str) -> Option<Vec<String>> {
    let chain = text.strip_suffix("::")?;
    // The chain is the identifier word the separator ends, so it starts
    // where the text's trailing word starts.
    let start = chain
        .trim_end_matches(|character: char| is_identifier_continue(character) || character == ':')
        .len();
    let segments: Vec<String> = chain[start..].split("::").map(ToOwned::to_owned).collect();
    (segments.iter().all(|segment| !segment.is_empty())).then_some(segments)
}

/// The span of the word a text ends with, when it ends with one.
fn trailing_word(text: &str) -> Option<Span> {
    let typed = text.len() - text.trim_end_matches(is_identifier_continue).len();
    (typed > 0).then(|| Span::new(text.len() - typed, text.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{VirtualSource, build};
    use std::path::PathBuf;

    /// The build of a virtual program.
    fn built(files: &[(&str, &str)]) -> Build {
        build(Path::new("main.kiru"), &VirtualSource::new(files)).expect("the program checks")
    }

    /// The file the build read the entry by.
    fn entry(built: &Build) -> PathBuf {
        built.loaded.files[built.loaded.entry].path.clone()
    }

    /// The completions at `offset` of `text`, as (kind, name) pairs, sorted.
    fn offered(built: &Build, text: &str, offset: usize) -> Vec<(String, String)> {
        let mut items = built
            .completions(&entry(built), text, offset)
            .into_iter()
            .map(|item| (format!("{:?}", item.kind), item.name))
            .collect::<Vec<_>>();
        items.sort();
        items
    }

    /// The offset just after `needle`'s first place.
    fn after(source: &str, needle: &str) -> usize {
        source.find(needle).expect("the needle is written") + needle.len()
    }

    /// The program with a half written line appended: what a client sends
    /// while a line is being typed, which is not the text the model checked.
    fn halfway(suffix: &str) -> String {
        format!("{PROGRAM}\nfn probe() {{ {suffix}")
    }

    const PROGRAM: &str = "use root::speak;\nuse std::dynamic;\nrec Point { x: int, y: int };\nenum Shape { Dot, Ring(int) };\nfn speak() -> txt { return \"spoken\"; };\nfn main() {\n  let point: Point = { x = 1, y = 2 };\n  let shape: Shape = Shape.Dot;\n  let spoken: txt = speak();\n  point.x = 0;\n  let taken: int = match shape { Shape.Dot => 0; _ => 1; };\n  std::print(spoken);\n};";

    #[test]
    fn a_path_separator_offers_the_target_namespace_s_reachable_names() {
        let program = built(&[("main.kiru", PROGRAM)]);
        // After `std::` in a half written line: the std tree's namespaces and
        // std's own names.
        let text = halfway("std::");
        let answers = offered(&program, &text, text.len());
        assert!(
            answers.contains(&("Namespace".to_owned(), "dynamic".to_owned())),
            "the std tree's namespaces are offered: {answers:?}"
        );
        assert!(
            answers.contains(&("Function".to_owned(), "print".to_owned())),
            "a native is a reachable name of its namespace: {answers:?}"
        );
        // A qualified prefix filters.
        let text = halfway("std::dy");
        assert_eq!(
            offered(&program, &text, text.len()),
            vec![("Namespace".to_owned(), "dynamic".to_owned())],
            "the typed prefix filters the names"
        );
    }

    #[test]
    fn a_bare_prefix_offers_the_namespace_s_own_names_and_deliveries() {
        let program = built(&[("main.kiru", PROGRAM)]);
        let text = halfway("spe");
        assert_eq!(
            offered(&program, &text, text.len()),
            vec![("Function".to_owned(), "speak".to_owned())],
            "the namespace's own names and the deliveries, prefix filtered"
        );
        // An empty prefix offers the namespace's own names, its types, and
        // the deliveries.
        let text = halfway("");
        let answers = offered(&program, &text, text.len());
        assert!(
            answers.contains(&("Function".to_owned(), "speak".to_owned()))
                && answers.contains(&("Rec".to_owned(), "Point".to_owned()))
                && answers.contains(&("Namespace".to_owned(), "dynamic".to_owned())),
            "the own names, the types, and the deliveries: {answers:?}"
        );
        assert!(
            !answers.contains(&("Function".to_owned(), "print".to_owned())),
            "another namespace's own names arrive only qualified: {answers:?}"
        );
    }

    #[test]
    fn a_dot_after_a_value_offers_its_type_s_members() {
        let program = built(&[("main.kiru", PROGRAM)]);
        // The receiver stands in the checked text, so the model knows its
        // type: the dot is the one `point.x` writes.
        assert_eq!(
            offered(&program, PROGRAM, after(PROGRAM, "point.")),
            vec![
                ("Field".to_owned(), "x".to_owned()),
                ("Field".to_owned(), "y".to_owned()),
            ],
            "a rec value offers its fields"
        );
    }

    #[test]
    fn a_position_offers_nothing_its_receiver_declares_no_members() {
        // A dot on a written type's name: the type is no node in the model,
        // so the receiver resolves to no typed value and nothing is offered.
        let typed = [(
            "main.kiru",
            "fn main() { let list: std::dynamic::Vec<int> = std::dynamic::Vec.new(); return; };",
        )];
        assert!(
            offered(&built(&typed), typed[0].1, after(typed[0].1, "Vec.")).is_empty(),
            "a dot on a type's own name offers no members"
        );
    }
}
