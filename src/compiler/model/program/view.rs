//! A view of the model pieces a stage reads and the derived facts it writes.
//!
//! The checker types a checked program and resolves a call's method while it
//! does, so it reads the method pools and the interner through this view too.
//! Both hands the slices they own to the one expression relation through this
//! view, so the relation reads declarations, types, namespaces, and files the
//! same way whichever stage is asking. The checker records the type of a
//! variant-pattern binding on the binding node it belongs to, and the flow
//! pass records whether a function stops the run on the function's
//! declaration.

use super::super::declarations::{Declaration, TypeDecl};
use super::super::ids::{DeclarationId, FileId, NamespaceId};
use super::super::types::{Type, TypeId, type_name};
use super::graph::{File, Namespace, Program};
use super::interner::Interner;
use super::namespace_paths::{join_path, namespace_path};

/// The declarations, types, namespaces, and files a stage can see.
pub(crate) struct ModelView<'a> {
    declarations: &'a mut [Declaration],
    types: &'a [TypeDecl],
    namespaces: &'a [Namespace],
    files: &'a [File],
    intern: &'a Interner,
}

impl<'a> ModelView<'a> {
    pub(crate) fn new(
        declarations: &'a mut [Declaration],
        types: &'a [TypeDecl],
        namespaces: &'a [Namespace],
        files: &'a [File],
        intern: &'a Interner,
    ) -> Self {
        Self {
            declarations,
            types,
            namespaces,
            files,
            intern,
        }
    }

    pub(crate) fn declaration(&self, id: DeclarationId) -> &Declaration {
        &self.declarations[id.0]
    }

    /// Every declaration, for a pass that walks the table once and reads each
    /// node through `declaration`.
    pub(crate) fn declarations(&self) -> &[Declaration] {
        self.declarations
    }

    /// Write one fact onto a declaration node: the type a variant-pattern
    /// binding holds once its arm instantiates the pattern.
    pub(crate) fn set_type(&mut self, id: DeclarationId, ty: Type) {
        self.declarations[id.0].ty = ty;
    }

    /// Record whether a function stops the run, the one derived fact the flow
    /// fixpoint settles.
    pub(crate) fn set_halts(&mut self, id: DeclarationId, halts: bool) {
        self.declarations[id.0].derived.halts = halts;
    }

    pub(crate) fn type_decl(&self, id: TypeId) -> &TypeDecl {
        &self.types[id.0]
    }

    pub(crate) fn types(&self) -> &[TypeDecl] {
        self.types
    }

    /// One namespace node, by id: its method pools and its children.
    pub(crate) fn namespace(&self, id: NamespaceId) -> &Namespace {
        &self.namespaces[id.0]
    }

    /// Every namespace node, for a pass that searches the method pools once.
    pub(crate) fn namespaces(&self) -> &[Namespace] {
        self.namespaces
    }

    /// The interner every table key came from. A name resolves to its symbol
    /// here before a pool read keys on it.
    pub(crate) fn intern(&self) -> &Interner {
        self.intern
    }

    pub(crate) fn file(&self, id: FileId) -> &File {
        &self.files[id.0]
    }

    /// The namespace path, root first.
    pub(crate) fn namespace_path(&self, id: NamespaceId) -> Vec<String> {
        namespace_path(self.namespaces, id)
    }

    /// The display name of a declaration: a local binding shows its bare name,
    /// and a file declaration shows its namespace path and name joined with
    /// `::`.
    pub(crate) fn display(&self, id: DeclarationId) -> String {
        let declaration = self.declaration(id);
        if declaration.kind.local() {
            return declaration.name.clone();
        }
        let mut path = self.namespace_path(declaration.namespace);
        path.push(declaration.name.clone());
        join_path(&path)
    }

    /// The name of a type for a diagnostic, rendered by its group's fact.
    pub(crate) fn type_name(&self, ty: &Type) -> String {
        type_name(ty, self.types)
    }
}

impl<'a> From<&'a mut Program> for ModelView<'a> {
    fn from(program: &'a mut Program) -> Self {
        let Program {
            declarations,
            types,
            namespaces,
            files,
            intern,
            ..
        } = program;
        Self::new(declarations, types, namespaces, files, intern)
    }
}
