//! Selecting the entry: the entry file's own root `main`.

use crate::compiler::Diagnostic;
use crate::compiler::load_files::LoadedProgram;
use crate::model::{DeclarationId, ENTRY_FUNCTION, FileId, NamespaceId};
use crate::syntax::Span;
use crate::syntax::{Declaration as ParsedDeclaration, ValueKind};

use super::builder::{Builder, Skeleton};

impl Builder {
    /// The entry is the entry file's own root `main` function.
    pub(in crate::compiler::resolve_names) fn resolve_entry(
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
        if let Some(return_kind) = function.return_kind
            && return_kind != ValueKind::Text
        {
            return Err(Diagnostic::new(
                path,
                function.name_span,
                format!("`{ENTRY_FUNCTION}` returns text or nothing"),
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
