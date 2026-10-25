//! Registering the runtime's natives into the builtin namespace, and the
//! runtime's container type declarations beside them.

use std::path::Path;

use crate::compiler::Diagnostic;
use crate::compiler::duplicate_in_namespace;
use crate::compiler::graph::Type;
use crate::compiler::graph::{
    DeclarationId, DeclarationKind, NamespaceId, RuntimeType, TypeBody, TypeHead, TypeKind,
};
use crate::compiler::native_registry::{
    Native, NativeParameter, NativeRow, NativeType, native_rows,
};
use crate::syntax::Span;

use super::builder::{Builder, Skeleton, Slot};

impl Builder {
    /// Register the runtime's list as an ordinary declaration in the
    /// `std::dynamic` namespace: kind `Rec`, one type parameter, no fields,
    /// and the runtime's marker. The marker is the one place the type's
    /// runtime representation, its missing literal shape, and its element
    /// parameter are declared. It is the one type declaration with no file.
    /// Register one container declaration the runtime represents itself: no
    /// file, no fields, and no literal. Its parameters are its element, or its
    /// key and its value; its operations are the natives that name it.
    fn register_container_type(
        &mut self,
        name: &str,
        parameters: Vec<String>,
        runtime: RuntimeType,
    ) -> Result<DeclarationId, Diagnostic> {
        let namespace = self.ensure_namespace(&["std".to_owned(), "dynamic".to_owned()]);
        let id = self.create_node(
            Skeleton {
                kind: DeclarationKind::Rec,
                binding: None,
                name: name.to_owned(),
                name_span: Span::new(0, 0),
                public: true,
                receiver_head: None,
                associated_head: None,
                file: None,
                namespace,
            },
            None,
            None,
        );
        self.declarations[id.0].ty = Type::Named(
            id,
            parameters.iter().cloned().map(Type::Parameter).collect(),
        );
        self.declarations[id.0].type_body = Some(TypeBody {
            parameters,
            runtime,
            kind: TypeKind::Rec(Vec::new()),
        });
        if let Err(existing) = self.namespaces[namespace.0]
            .types
            .insert(self.intern.intern(name), id)
        {
            let declaration = &self.declarations[existing.0];
            let file = declaration
                .file
                .expect("a source type is the only earlier type name");
            return Err(Diagnostic::new(
                &self.files[file.0].path,
                declaration.name_span,
                duplicate_in_namespace(name),
            ));
        }
        Ok(id)
    }

    /// Register the runtime list, `std::dynamic::Vec`, whose one parameter is
    /// its element.
    pub(super) fn register_list_type(&mut self) -> Result<DeclarationId, Diagnostic> {
        let id = self.register_container_type("Vec", vec!["T".to_owned()], RuntimeType::List)?;
        self.list_type = Some(id);
        Ok(id)
    }

    /// Register the runtime table, `std::dynamic::HashMap`, whose two
    /// parameters are its key and its value.
    pub(super) fn register_table_type(&mut self) -> Result<DeclarationId, Diagnostic> {
        let id = self.register_container_type(
            "HashMap",
            vec!["K".to_owned(), "V".to_owned()],
            RuntimeType::Table,
        )?;
        self.table_type = Some(id);
        Ok(id)
    }

    /// Resolve one type a native row names. A path resolves through the
    /// ordinary type resolver; a container parameter is the spelling the
    /// container's own declaration gave it.
    fn resolve_native_type(
        &self,
        ty: &NativeType,
        namespace: NamespaceId,
        reported_path: &Path,
    ) -> Result<Type, Diagnostic> {
        match ty {
            NativeType::Direct(ty) => Ok(ty.clone()),
            NativeType::Parameter(role) => {
                let id = match role {
                    NativeParameter::Element => self.list_type,
                    NativeParameter::Key | NativeParameter::Value => self.table_type,
                }
                .expect("the compiler registers a container before the natives that name it");
                let body = self.declarations[id.0]
                    .type_body()
                    .expect("a runtime container is a type declaration");
                let parameter = match role {
                    NativeParameter::Element => body.element_parameter(),
                    NativeParameter::Key => body.key_parameter(),
                    NativeParameter::Value => body.value_parameter(),
                }
                .expect("the container declares the parameter the row names");
                Ok(Type::Parameter(parameter.to_owned()))
            }
            NativeType::Path { path, arguments } => {
                let mut resolved = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    resolved.push(self.resolve_native_type(argument, namespace, reported_path)?);
                }
                let path: Vec<String> = path.iter().map(|segment| (*segment).to_owned()).collect();
                self.apply_declared_type(
                    &path,
                    &Span::new(0, 0),
                    resolved,
                    namespace,
                    reported_path,
                )
            }
        }
    }

    /// Create a native callable node. A native has no file and no value form:
    /// the runtime provides it, so it carries its runtime index. Its type is a
    /// function type read from its row; a call reads its parameter types from
    /// that type, so it needs no parameter nodes.
    fn declare_native(
        &mut self,
        namespace: NamespaceId,
        row: &NativeRow,
        native: Native,
        ty: Type,
    ) -> DeclarationId {
        // The row's own signature names the type the native belongs to: a first
        // parameter gives the receiver head of a method, and a row with no
        // parameters is an associated function of the declared type it returns.
        // A no-parameter native that returns a built-in type belongs to no
        // type: it is a plain namespace-level name, like `getcwd`.
        let (receiver_head, associated_head) = match &ty {
            Type::Function {
                parameters,
                returns,
            } => match parameters.first() {
                Some(parameter) => (Some(parameter.head(&self.declarations)), None),
                None => match returns
                    .as_ref()
                    .map(|returns| returns.head(&self.declarations))
                {
                    Some(head @ TypeHead::Named(_)) => (None, Some(head)),
                    _ => (None, None),
                },
            },
            _ => (None, None),
        };
        let id = self.create_node(
            Skeleton {
                kind: DeclarationKind::Native,
                binding: None,
                name: row.name.to_owned(),
                name_span: Span::new(0, 0),
                public: true,
                receiver_head,
                associated_head,
                file: None,
                namespace,
            },
            None,
            Some(Slot::Native(ty, native)),
        );
        // A native with a head joins the pool its row names: a method under its
        // receiver's head, an associated function under the head of the type it
        // belongs to. A bound function is not a namespace-level name, so it
        // does not join the name pool; a plain native does.
        let symbol = self.intern.intern(row.name);
        if let Some(head) = receiver_head {
            self.insert_bound_function(namespace, head, symbol, id, true)
                .expect("a native's name and head are unique, so a builtin never collides");
        } else if let Some(head) = associated_head {
            self.insert_bound_function(namespace, head, symbol, id, false)
                .expect("a native's name and head are unique, so a builtin never collides");
        } else {
            self.namespaces[namespace.0].names.insert(symbol, id);
        }
        id
    }

    pub(in crate::compiler::derive::resolve) fn register_builtins(
        &mut self,
    ) -> Result<(), Diagnostic> {
        // The runtime containers come first: a native row that names one, and
        // a row that names its parameters, need its declaration.
        self.register_list_type()?;
        self.register_table_type()?;
        for (native, row) in native_rows() {
            let namespace = self.ensure_namespace(
                &row.path
                    .iter()
                    .map(|segment| (*segment).to_owned())
                    .collect::<Vec<String>>(),
            );
            let mut parameters = Vec::with_capacity(row.parameters.len());
            for parameter in &row.parameters {
                parameters.push(self.resolve_native_type(
                    parameter,
                    namespace,
                    &self.files[0].path,
                )?);
            }
            let returns = match &row.returns {
                Some(returns) => Some(Box::new(self.resolve_native_type(
                    returns,
                    namespace,
                    &self.files[0].path,
                )?)),
                None => None,
            };
            let ty = Type::Function {
                parameters,
                returns,
            };
            self.declare_native(namespace, &row, native, ty);
        }
        Ok(())
    }
}
