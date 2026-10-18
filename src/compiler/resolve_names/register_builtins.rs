//! Registering the runtime's natives into the builtin namespace, and the
//! runtime's `Vec` type declaration beside them.

use crate::compiler::model::Type;
use crate::compiler::model::{
    DeclarationId, DeclarationKind, NamespaceId, TypeDecl, TypeHead, TypeId, TypeKind,
};
use crate::native_registry::{Native, NativeRow, native_rows};
use crate::syntax::Span;

use super::builder::{Builder, Skeleton, Slot};

impl Builder {
    /// Register the runtime's `Vec` type as an ordinary declaration in the
    /// `std::dynamic` namespace: kind `Rec`, one type parameter, no fields.
    /// The representation is the runtime's — exactly what a `rec` cannot say —
    /// so the lowering is the one stage that reads its id.
    pub(super) fn register_vec_type(&mut self) -> TypeId {
        let namespace = self.ensure_namespace(&["std".to_owned(), "dynamic".to_owned()]);
        let id = TypeId(self.types.len());
        self.types.push(TypeDecl {
            name: "Vec".to_owned(),
            public: true,
            namespace,
            parameters: vec!["T".to_owned()],
            kind: TypeKind::Rec(Vec::new()),
        });
        self.namespaces[namespace.0]
            .types
            .insert("Vec".to_owned(), id)
            .expect("the compiler registers `Vec` before any file declares types");
        self.vec_type = Some(id);
        id
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
    ) -> DeclarationId {
        let ty = Type::Function {
            parameters: row.parameters.clone(),
            returns: row.returns.clone().map(Box::new),
        };
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
                Some(parameter) => (Some(parameter.head(&self.types)), None),
                None => match returns.as_ref().map(|returns| returns.head(&self.types)) {
                    Some(head @ TypeHead::Named(_)) => (None, Some(head)),
                    _ => (None, None),
                },
            },
            _ => (None, None),
        };
        let id = self.create_node(
            Skeleton {
                kind: DeclarationKind::Native,
                name: row.name.to_owned(),
                name_span: Span::new(0, 0),
                public: true,
                receiver_head,
                associated_head,
                file: None,
                namespace,
                syntax_index: None,
            },
            None,
            Some(Slot::Native(ty, native)),
        );
        // A native with a head joins the pool its row names: a method under its
        // receiver's head, an associated function under the head of the type it
        // belongs to. A bound function is not a namespace-level name, so it
        // does not join the name pool; a plain native does.
        let table = &mut self.namespaces[namespace.0];
        if let Some(head) = receiver_head {
            table
                .methods
                .insert(head, row.name.to_owned(), id)
                .expect("a native's name and head are unique, so a builtin never collides");
        } else if let Some(head) = associated_head {
            table
                .associates
                .insert(head, row.name.to_owned(), id)
                .expect("a native's name and head are unique, so a builtin never collides");
        } else {
            table.names.insert(row.name.to_owned(), id);
        }
        id
    }

    pub(in crate::compiler::resolve_names) fn register_builtins(&mut self) {
        // The runtime's `Vec` type comes first: a native row that names it
        // needs the declaration's id.
        let vec_type = Some(self.register_vec_type());
        for (native, row) in native_rows(self.absence_type, vec_type) {
            let namespace = self.ensure_namespace(
                &row.path
                    .iter()
                    .map(|segment| (*segment).to_owned())
                    .collect::<Vec<String>>(),
            );
            self.declare_native(namespace, &row, native);
        }
    }
}
