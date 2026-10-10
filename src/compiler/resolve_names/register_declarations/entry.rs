//! Selecting the entry: the root namespace's `main`.

use crate::compiler::Diagnostic;
use crate::compiler::load_files::LoadedProgram;
use crate::model::{DeclarationId, ENTRY_FUNCTION, NamespaceId};
use crate::syntax::Declaration as ParsedDeclaration;
use crate::syntax::Span;
use crate::types::Type;

use super::builder::Builder;

impl Builder {
    /// The entry is the **root namespace's** `main` function. A `main` inside a
    /// `mod` is an ordinary function, so only a root one counts.
    pub(in crate::compiler::resolve_names) fn resolve_entry(
        &self,
        loaded: &LoadedProgram,
    ) -> Result<DeclarationId, Diagnostic> {
        let Some(index) = self.namespaces[NamespaceId(0).0].function(ENTRY_FUNCTION) else {
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
        if let Some(parameter) = function.parameters.first()
            && parameter.ty != Type::Record
        {
            return Err(Diagnostic::new(
                path,
                parameter.span,
                format!("`{ENTRY_FUNCTION}`'s parameter must be declared `rec`"),
            ));
        }
        if let Some(return_type) = function.return_type
            && return_type != Type::Text
        {
            return Err(Diagnostic::new(
                path,
                function.name_span,
                format!("`{ENTRY_FUNCTION}` returns text or nothing"),
            ));
        }
        Ok(index)
    }
}
