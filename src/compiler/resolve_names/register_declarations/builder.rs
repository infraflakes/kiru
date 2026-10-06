//! The builder: reserve declaration nodes and register their names.
//!
//! Registration builds the skeleton of the model: namespaces, declaration
//! nodes, and the worklist of bodies still to link. The linking pass fills
//! every body. A namespace keeps two registries: values are unique by name and
//! functions are unique by name, so a function may share a name with a value.

use std::collections::HashMap;

use crate::compiler::{Diagnostic, duplicate_in_namespace};
use crate::model::{
    Declaration, DeclarationId, DeclarationKind, Derived, File, FileId, NameTable, Namespace,
    NamespaceId, Program, Registry,
};
use crate::syntax::Span;

/// Where a file declaration is written, for registration.
pub(in crate::compiler::resolve_names) struct DeclarationSite<'a> {
    pub(in crate::compiler::resolve_names) namespace: NamespaceId,
    pub(in crate::compiler::resolve_names) name: &'a str,
    pub(in crate::compiler::resolve_names) name_span: Span,
    pub(in crate::compiler::resolve_names) file: Option<FileId>,
    pub(in crate::compiler::resolve_names) syntax_index: Option<usize>,
    pub(in crate::compiler::resolve_names) reported_path: &'a std::path::Path,
}

/// What is known about a declaration before its body is linked.
pub(in crate::compiler::resolve_names) struct Skeleton {
    pub(in crate::compiler::resolve_names) name: String,
    pub(in crate::compiler::resolve_names) name_span: Span,
    pub(in crate::compiler::resolve_names) file: Option<FileId>,
    pub(in crate::compiler::resolve_names) namespace: NamespaceId,
    pub(in crate::compiler::resolve_names) order: usize,
    /// The index of the parsed declaration in its file, when it came from one.
    pub(in crate::compiler::resolve_names) syntax_index: Option<usize>,
}

/// A worklist entry for the linking pass.
pub(in crate::compiler::resolve_names) struct Pending {
    pub(in crate::compiler::resolve_names) declaration: DeclarationId,
    pub(in crate::compiler::resolve_names) file: FileId,
    pub(in crate::compiler::resolve_names) syntax_index: usize,
    pub(in crate::compiler::resolve_names) namespace: NamespaceId,
    pub(in crate::compiler::resolve_names) order: usize,
}

pub(in crate::compiler::resolve_names) struct Builder {
    pub(in crate::compiler::resolve_names) namespaces: Vec<Namespace>,
    pub(in crate::compiler::resolve_names) skeletons: Vec<Skeleton>,
    pub(in crate::compiler::resolve_names) declarations: Vec<Option<Declaration>>,
    pub(in crate::compiler::resolve_names) files: Vec<File>,
    pub(in crate::compiler::resolve_names) pending: Vec<Pending>,
    pub(in crate::compiler::resolve_names) order: usize,
}

impl Builder {
    pub(in crate::compiler::resolve_names) fn new() -> Self {
        let root = Namespace {
            name: String::new(),
            parent: None,
            children: HashMap::new(),
            values: NameTable::default(),
            functions: NameTable::default(),
        };
        Self {
            namespaces: vec![root],
            skeletons: Vec::new(),
            declarations: Vec::new(),
            files: Vec::new(),
            pending: Vec::new(),
            order: 0,
        }
    }

    pub(in crate::compiler::resolve_names) fn finish(self, entry: DeclarationId) -> Program {
        Program {
            files: self.files,
            namespaces: self.namespaces,
            declarations: self
                .declarations
                .into_iter()
                .map(|declaration| declaration.expect("every node is built during linking"))
                .collect(),
            entry,
        }
    }

    /// The namespace node for a path, creating missing segments.
    pub(super) fn ensure_namespace(&mut self, segments: &[String]) -> NamespaceId {
        let mut current = NamespaceId(0);
        for segment in segments {
            current = match self.namespaces[current.0].children.get(segment) {
                Some(&child) => child,
                None => {
                    let child = NamespaceId(self.namespaces.len());
                    self.namespaces.push(Namespace {
                        name: segment.clone(),
                        parent: Some(current),
                        children: HashMap::new(),
                        values: NameTable::default(),
                        functions: NameTable::default(),
                    });
                    self.namespaces[current.0]
                        .children
                        .insert(segment.clone(), child);
                    child
                }
            };
        }
        current
    }

    /// Create one node and store it in the model: its skeleton and its
    /// declaration slot. A slot stays open (`kind` is `None`) when the linking
    /// pass will fill the body later. Name registration is the caller's task,
    /// because file declarations and local bindings register differently.
    pub(in crate::compiler::resolve_names) fn create_node(
        &mut self,
        skeleton: Skeleton,
        owner: Option<DeclarationId>,
        kind: Option<DeclarationKind>,
    ) -> DeclarationId {
        let id = DeclarationId(self.skeletons.len());
        let name = skeleton.name.clone();
        let name_span = skeleton.name_span;
        let file = skeleton.file;
        let namespace = skeleton.namespace;
        let order = skeleton.order;
        self.skeletons.push(skeleton);
        self.declarations.push(kind.map(|kind| Declaration {
            name,
            name_span,
            file,
            namespace,
            order,
            owner,
            parameters: Vec::new(),
            kind,
            derived: Derived { halts: false },
        }));
        id
    }

    /// Reserve a declaration node and register its name in one registry.
    pub(super) fn declare(
        &mut self,
        site: DeclarationSite<'_>,
        registry: Registry,
    ) -> Result<DeclarationId, Diagnostic> {
        let DeclarationSite {
            namespace,
            name,
            name_span,
            file,
            syntax_index,
            reported_path,
        } = site;
        let order = self.order;
        let id = self.create_node(
            Skeleton {
                name: name.to_owned(),
                name_span,
                file,
                namespace,
                order,
                syntax_index,
            },
            None,
            None,
        );
        let table = self.namespaces[namespace.0].table_mut(registry);
        if table.insert(name.to_owned(), id).is_err() {
            return Err(Diagnostic::new(
                reported_path,
                name_span,
                duplicate_in_namespace(name),
            ));
        }
        self.order += 1;
        Ok(id)
    }
}
