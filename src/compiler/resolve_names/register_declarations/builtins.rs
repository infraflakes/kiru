//! Registering the runtime's natives into the builtin namespace.

use crate::model::{Binding, DeclarationId, DeclarationKind, NamespaceId};
use crate::native_registry::{NATIVE_ROWS, Native, NativeRow};
use crate::syntax::Span;

use super::builder::{Builder, Skeleton};

impl Builder {
    /// Create a native callable node and its parameter nodes. A native has no
    /// file and no body, but it carries parameters exactly like a user
    /// function, so the checker has one call path. Its result type is read
    /// from its row, so no type is seeded on the node.
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
        );
        self.namespaces[namespace.0]
            .functions
            .insert(row.name.to_owned(), id)
            .expect("native names are unique, so a builtin never collides");
        self.order += 1;

        let mut parameters = Vec::new();
        for ty in row.parameters {
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
                Some(DeclarationKind::Binding(Binding {
                    ty: *ty,
                    mutable: false,
                })),
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

    pub(in crate::compiler::resolve_names) fn register_builtins(&mut self) {
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
}
