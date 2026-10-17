//! Selecting the entry: the program root's `main`.

use crate::compiler::Diagnostic;
use crate::compiler::load_files::{LoadedProgram, PROGRAM_TREE};
use crate::model::Type;
use crate::model::{DeclarationId, ENTRY_FUNCTION, NamespaceId, namespace_at};
use crate::syntax::Declaration as ParsedDeclaration;
use crate::syntax::Span;

use super::builder::Builder;

impl Builder {
    /// The entry is the **program root's** `main` function. A `main` inside a
    /// sub-namespace is an ordinary function, so only a root one counts.
    pub(in crate::compiler::resolve_names) fn resolve_entry(
        &self,
        loaded: &LoadedProgram,
    ) -> Result<DeclarationId, Diagnostic> {
        let root =
            namespace_at(&self.namespaces, &[PROGRAM_TREE.to_owned()]).unwrap_or(NamespaceId(0));
        let Some(index) = self.namespaces[root.0].get(ENTRY_FUNCTION).first().copied() else {
            let path = &loaded.files[loaded.entry].path;
            return Err(Diagnostic::new(
                path,
                Span::new(0, 0),
                format!("the program has no root `{ENTRY_FUNCTION}` function"),
            ));
        };
        let skeleton = &self.skeletons[index.0];
        let file = skeleton
            .file
            .expect("a root main is a file declaration, so it has a file");
        let path = &loaded.files[file.0].path;
        let Some(ParsedDeclaration::Function(function)) = skeleton
            .syntax_index
            .and_then(|item| loaded.items.get(item))
            .map(|scoped| &scoped.declaration)
        else {
            return Err(Diagnostic::new(
                path,
                skeleton.name_span,
                format!("`{ENTRY_FUNCTION}` is not a function"),
            ));
        };
        if function.parameters.len() > 1 {
            return Err(Diagnostic::new(
                path,
                function.name_span,
                format!("`{ENTRY_FUNCTION}` takes at most one parameter"),
            ));
        }
        // The entry's shape is checked against the resolved types: the runtime's
        // `Vec` is a declaration like any other, so its name resolves through
        // `use` exactly as it does in a body.
        if let Some(parameter) = function.parameters.first() {
            let declared = self.resolve_type_expr(&parameter.ty, root, &[], path)?;
            let vec = self
                .vec_type
                .expect("the compiler registers the Vec type before the entry is selected");
            if declared != Type::Named(vec, vec![Type::Text]) {
                return Err(Diagnostic::new(
                    path,
                    parameter.span,
                    format!(
                        "`{ENTRY_FUNCTION}`'s parameter must be declared `std::dynamic::Vec<txt>`"
                    ),
                ));
            }
        }
        if let Some(return_type) = &function.return_type {
            let declared = self.resolve_type_expr(return_type, root, &[], path)?;
            if declared != Type::Int {
                return Err(Diagnostic::new(
                    path,
                    function.name_span,
                    format!("`{ENTRY_FUNCTION}` returns int or nothing"),
                ));
            }
        }
        Ok(index)
    }
}
