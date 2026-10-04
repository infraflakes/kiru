//! Linking registration: reserve declaration nodes and register their names.
//!
//! Registration builds the skeleton of the model: namespaces, declaration
//! nodes, and the worklist of bodies still to link. The linking pass in
//! `link_bodies` then fills every body. A namespace keeps two registries: values
//! are unique by name and functions are unique by name, so a function may
//! share a name with a value.

use std::collections::HashMap;
use std::sync::RwLock;

use crate::compiler::load_files::LoadedProgram;
use crate::compiler::{
    BUILTIN_NAMESPACE, BindingKind, Declaration, DeclarationId, DeclarationKind, Derived,
    Diagnostic, ENTRY_FUNCTION, File, FileId, Kind, NATIVE_ROWS, NameTable, Namespace, NamespaceId,
    Native, NativeRow, Origin, Program, Registry, duplicate_in_namespace,
};
use crate::syntax::Span;
use crate::syntax::{Declaration as ParsedDeclaration, ValueKind};

/// Where a file declaration is written, for registration.
struct DeclarationSite<'a> {
    namespace: NamespaceId,
    name: &'a str,
    name_span: Span,
    file: Option<FileId>,
    syntax_index: Option<usize>,
    reported_path: &'a std::path::Path,
}

/// What is known about a declaration before its body is linked.
pub(super) struct Skeleton {
    pub(super) name: String,
    pub(super) name_span: Span,
    pub(super) file: Option<FileId>,
    pub(super) namespace: NamespaceId,
    pub(super) order: usize,
    /// The index of the parsed declaration in its file, when it came from one.
    pub(super) syntax_index: Option<usize>,
}

/// A worklist entry for the linking pass.
pub(super) struct Pending {
    pub(super) declaration: DeclarationId,
    pub(super) file: FileId,
    pub(super) syntax_index: usize,
    pub(super) namespace: NamespaceId,
    pub(super) order: usize,
}

pub(super) struct Builder {
    pub(super) namespaces: Vec<Namespace>,
    pub(super) skeletons: Vec<Skeleton>,
    pub(super) declarations: Vec<Option<Declaration>>,
    pub(super) files: Vec<File>,
    pub(super) pending: Vec<Pending>,
    pub(super) order: usize,
}

impl Builder {
    pub(super) fn new() -> Self {
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

    pub(super) fn finish(self, entry: DeclarationId) -> Program {
        Program {
            files: self.files,
            namespaces: self.namespaces,
            declarations: self
                .declarations
                .into_iter()
                .map(|declaration| declaration.expect("every node is built during linking"))
                .collect(),
            entry,
            values: RwLock::new(HashMap::new()),
        }
    }

    /// The namespace node for a path, creating missing segments.
    fn ensure_namespace(&mut self, segments: &[String]) -> NamespaceId {
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
    /// pass will fill the body later. `derived_kind` seeds a kind that is known
    /// at link time, such as a parameter's written kind. Name registration is
    /// the caller's task, because file declarations and local bindings register
    /// differently.
    pub(super) fn create_node(
        &mut self,
        skeleton: Skeleton,
        owner: Option<DeclarationId>,
        kind: Option<DeclarationKind>,
        derived_kind: Option<Kind>,
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
            derived: Derived { kind: derived_kind },
        }));
        id
    }

    /// Reserve a declaration node and register its name in one registry.
    fn declare(
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

    /// Create a native callable node and its parameter nodes. A native has no
    /// file and no body, but it carries parameters exactly like a user
    /// function, so the checker has one call path. Its result kind is known at
    /// registration, so it is seeded directly.
    fn declare_native(
        &mut self,
        namespace: NamespaceId,
        row: &'static NativeRow,
        native: Native,
    ) -> DeclarationId {
        let order = self.order;
        let id = self.create_node(
            Skeleton {
                name: row.name.to_owned(),
                name_span: Span::new(0, 0),
                file: None,
                namespace,
                order,
                syntax_index: None,
            },
            None,
            Some(DeclarationKind::Native(native)),
            Some(row.returns),
        );
        self.namespaces[namespace.0]
            .functions
            .insert(row.name.to_owned(), id)
            .expect("native names are unique, so a builtin never collides");
        self.order += 1;

        let mut parameters = Vec::new();
        for kind in row.parameters {
            let parameter = self.create_node(
                Skeleton {
                    name: String::new(),
                    name_span: Span::new(0, 0),
                    file: None,
                    namespace,
                    order: self.order,
                    syntax_index: None,
                },
                Some(id),
                Some(DeclarationKind::Binding(BindingKind::Parameter)),
                Some(*kind),
            );
            parameters.push(parameter);
            self.order += 1;
        }
        self.declarations[id.0]
            .as_mut()
            .expect("a native node is built during registration")
            .parameters = parameters;
        id
    }

    pub(super) fn register_builtins(&mut self) {
        for (native, row) in NATIVE_ROWS {
            let namespace = self.ensure_namespace(
                &row.path
                    .iter()
                    .map(|segment| (*segment).to_owned())
                    .collect::<Vec<String>>(),
            );
            self.declare_native(namespace, row, *native);
        }
    }

    pub(super) fn register_file(
        &mut self,
        loaded: &LoadedProgram,
        index: usize,
        parsed: &[ParsedDeclaration],
    ) -> Result<(), Diagnostic> {
        let loaded_file = &loaded.files[index];
        let namespace_segments: &[String] = loaded_file
            .file
            .module
            .as_ref()
            .map(|module| module.segments.as_slice())
            .unwrap_or(&[]);
        if loaded_file.origin != Origin::Embedded
            && namespace_segments.first().map(String::as_str) == BUILTIN_NAMESPACE.first().copied()
        {
            let span = loaded_file
                .file
                .module
                .as_ref()
                .map(|module| module.span)
                .unwrap_or_else(|| Span::new(0, 0));
            return Err(Diagnostic::new(
                &loaded_file.path,
                span,
                "`std` is reserved and cannot be declared",
            ));
        }
        let namespace = self.ensure_namespace(namespace_segments);
        let file = FileId(self.files.len());
        self.files.push(File {
            path: loaded_file.path.clone(),
            namespace,
            imports: loaded_file.imports.iter().map(|i| FileId(*i)).collect(),
            declarations: Vec::new(),
            origin: loaded_file.origin,
        });

        for (syntax_index, declaration) in parsed.iter().enumerate() {
            let (name, name_span, registry) = match declaration {
                ParsedDeclaration::Function(function) => {
                    (&function.name, function.name_span, Registry::Function)
                }
                ParsedDeclaration::Binding(binding) => {
                    (&binding.name, binding.name_span, Registry::Value)
                }
            };
            let id = self.declare(
                DeclarationSite {
                    namespace,
                    name,
                    name_span,
                    file: Some(file),
                    syntax_index: Some(syntax_index),
                    reported_path: &loaded_file.path,
                },
                registry,
            )?;
            self.pending.push(Pending {
                declaration: id,
                file,
                syntax_index,
                namespace,
                order: self.skeletons[id.0].order,
            });
            self.files[file.0].declarations.push(id);
        }
        Ok(())
    }

    /// The entry is the entry file's own root `main` function.
    pub(super) fn resolve_entry(
        &self,
        loaded: &LoadedProgram,
        entry_file: FileId,
        parsed: &[Vec<ParsedDeclaration>],
    ) -> Result<DeclarationId, Diagnostic> {
        let path = &loaded.files[entry_file.0].path;
        let candidate = self.skeletons.iter().enumerate().find(|(_, skeleton)| {
            skeleton.file == Some(entry_file)
                && skeleton.name == ENTRY_FUNCTION
                && skeleton_is_function(skeleton, parsed)
        });
        let Some((index, skeleton)) = candidate else {
            return Err(Diagnostic::new(
                path,
                Span::new(0, 0),
                format!("the entry file has no `{ENTRY_FUNCTION}` function"),
            ));
        };
        if skeleton.namespace != NamespaceId(0) {
            return Err(Diagnostic::new(
                path,
                skeleton.name_span,
                format!(
                    "`{ENTRY_FUNCTION}` in the entry file must be declared in the root namespace"
                ),
            ));
        }
        let Some(ParsedDeclaration::Function(function)) = skeleton
            .syntax_index
            .and_then(|index| parsed[entry_file.0].get(index))
        else {
            return Err(Diagnostic::new(
                path,
                skeleton.name_span,
                format!("`{ENTRY_FUNCTION}` in the entry file is not a function"),
            ));
        };
        if function.parameters.len() > 1 {
            return Err(Diagnostic::new(
                path,
                function.name_span,
                format!("`{ENTRY_FUNCTION}` takes at most one parameter"),
            ));
        }
        if let Some(parameter) = function.parameters.first()
            && parameter.kind != ValueKind::Record
        {
            return Err(Diagnostic::new(
                path,
                parameter.span,
                format!("`{ENTRY_FUNCTION}`'s parameter must be declared `rec`"),
            ));
        }
        Ok(DeclarationId(index))
    }
}

/// Whether a skeleton points at a parsed function, so a value named `main`
/// cannot stand in for the entry.
fn skeleton_is_function(skeleton: &Skeleton, parsed: &[Vec<ParsedDeclaration>]) -> bool {
    let Some(file) = skeleton.file else {
        return false;
    };
    matches!(
        skeleton
            .syntax_index
            .and_then(|index| parsed[file.0].get(index)),
        Some(ParsedDeclaration::Function(_))
    )
}
