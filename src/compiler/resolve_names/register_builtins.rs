//! Registering the runtime's natives into the builtin namespace.

use crate::model::Type;
use crate::model::{DeclarationId, NamespaceId};
use crate::native_registry::{Native, NativeRow, native_rows};
use crate::syntax::Span;

use super::builder::{Builder, Skeleton, Slot};

impl Builder {
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
        let order = self.order;
        let ty = Type::Function {
            parameters: row.parameters.clone(),
            returns: row.returns.clone().map(Box::new),
        };
        let first_parameter_head = match &ty {
            Type::Function { parameters, .. } => parameters.first().map(Type::head),
            _ => None,
        };
        let id = self.create_node(
            Skeleton {
                name: row.name.to_owned(),
                name_span: Span::new(0, 0),
                public: true,
                first_parameter_head,
                file: None,
                namespace,
                order,
                syntax_index: None,
            },
            None,
            Some(Slot::Native(ty, native)),
        );
        // A native is a method when its first parameter has a head: it joins
        // the method pool like any other declaration.
        let table = &mut self.namespaces[namespace.0];
        if let Some(head) = first_parameter_head {
            table
                .methods
                .insert(head, row.name.to_owned(), id)
                .expect("native names are unique, so a builtin never collides");
        }
        table.names.insert(row.name.to_owned(), id);
        self.order += 1;
        id
    }

    pub(in crate::compiler::resolve_names) fn register_builtins(&mut self) {
        for (native, row) in native_rows(self.absence_type) {
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
