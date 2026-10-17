//! Registering the runtime's natives into the builtin namespace, and the
//! runtime's `Vec` type declaration beside them.

use crate::model::Type;
use crate::model::{DeclarationId, NamespaceId, TypeDecl, TypeId, TypeKind};
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
        let first_parameter_head = match &ty {
            Type::Function { parameters, .. } => parameters.first().map(Type::head),
            _ => None,
        };
        // A native with no parameters is reachable through the type it
        // returns, exactly as a file function is.
        let constructor_head = match &ty {
            Type::Function {
                parameters,
                returns,
            } if parameters.is_empty() => returns.as_ref().map(|returns| returns.head()),
            _ => None,
        };
        let id = self.create_node(
            Skeleton {
                name: row.name.to_owned(),
                name_span: Span::new(0, 0),
                public: true,
                first_parameter_head,
                constructor_head,
                file: None,
                namespace,
                syntax_index: None,
            },
            None,
            Some(Slot::Native(ty, native)),
        );
        // A native with a head joins the method pool like any other
        // declaration: a method under its receiver's head, a constructor under
        // the head of the type it returns.
        let table = &mut self.namespaces[namespace.0];
        if let Some(head) = first_parameter_head.or(constructor_head) {
            table
                .methods
                .insert(head, row.name.to_owned(), id)
                .expect("native names are unique, so a builtin never collides");
        }
        table.names.insert(row.name.to_owned(), id);
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
