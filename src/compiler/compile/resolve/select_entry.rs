//! Selecting the entry: the program root's `main`.

use crate::compiler::Diagnostic;
use crate::compiler::compile::load::{LoadedProgram, PROGRAM_TREE};
use crate::compiler::model::Type;
use crate::compiler::model::{
    DeclarationId, ENTRY_FUNCTION, NamespaceId, TypeDecl, fits, list_type, namespace_at,
};
use crate::syntax::Span;

use super::builder::Builder;

/// The shape every program entry declares: at most one parameter, carrying the
/// command-line words as the runtime list of text, and `int` or no value as its
/// result. Selection checks the settled function type against this fact, the
/// decode boundary checks the lowered parameter count, and the runtime decides
/// whether to pass the words. The fact is declared here, beside the one place
/// the entry is chosen.
pub(crate) struct EntryShape;

impl EntryShape {
    /// The most parameters the entry may declare.
    pub(crate) const MAX_PARAMETERS: usize = 1;

    /// Whether an entry with `parameter_count` parameters is runnable.
    pub(crate) fn accepts_parameters(parameter_count: usize) -> bool {
        parameter_count <= Self::MAX_PARAMETERS
    }

    /// Whether an entry with `arity` parameters receives the command-line
    /// words.
    pub(crate) fn takes_words(arity: u16) -> bool {
        usize::from(arity) == Self::MAX_PARAMETERS
    }

    /// The type the entry's one parameter declares: the runtime list of text.
    pub(crate) fn parameter_type(types: &[TypeDecl]) -> Type {
        let list = list_type(types).expect("the compiler registers the runtime list");
        Type::Named(list, vec![Type::Text])
    }

    /// The type the entry's result declares, when it declares one.
    pub(crate) fn returns(ty: &Type) -> bool {
        *ty == Type::Int
    }
}

impl Builder {
    /// The entry is the **program root's** `main` function. A `main` inside a
    /// sub-namespace is an ordinary function, so only a root one counts.
    ///
    /// The declaration's type is settled before this runs, so the shape is
    /// checked against the model's resolved type rather than by resolving the
    /// written annotations a second time.
    pub(in crate::compiler::compile::resolve) fn resolve_entry(
        &self,
        loaded: &LoadedProgram,
    ) -> Result<DeclarationId, Diagnostic> {
        let root = namespace_at(&self.intern, &self.namespaces, &[PROGRAM_TREE.to_owned()])
            .unwrap_or(NamespaceId(0));
        // The entry is a plain function: a function filed under a type by an
        // `impl` block is not a namespace-level name, so the name pool holds
        // only the program's `main`.
        let entry_file = loaded
            .entry
            .expect("an entry is resolved only for a named compilation target");
        let Some(index) = self.namespaces[root.0].get(&self.intern, ENTRY_FUNCTION) else {
            let path = &loaded.files[entry_file].path;
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
        // The entry is the compilation target: the `main` reachability starts
        // from must stand in that file, so a tree whose `main` lives beside
        // the named target is rejected rather than silently run from another
        // file.
        if file.0 != entry_file {
            let path = &loaded.files[entry_file].path;
            return Err(Diagnostic::new(
                path,
                Span::new(0, 0),
                format!("the entry file declares no `{ENTRY_FUNCTION}` function"),
            )
            .with_related(
                self.files[file.0].path.clone(),
                skeleton.name_span,
                format!("`{ENTRY_FUNCTION}` is declared here"),
            ));
        }
        let path = &self.files[file.0].path;
        if !skeleton.kind.may_be_entry() {
            return Err(Diagnostic::new(
                path,
                skeleton.name_span,
                format!("`{ENTRY_FUNCTION}` is not a function"),
            ));
        }
        let Type::Function {
            parameters,
            returns,
        } = &self.declarations[index.0].ty
        else {
            unreachable!("a function declaration carries a function type");
        };
        if !EntryShape::accepts_parameters(parameters.len()) {
            return Err(Diagnostic::new(
                path,
                skeleton.name_span,
                format!("`{ENTRY_FUNCTION}` takes at most one parameter"),
            ));
        }
        if let Some(parameter) = parameters.first()
            && !fits(
                parameter,
                &EntryShape::parameter_type(&self.types),
                &self.types,
            )
        {
            return Err(Diagnostic::new(
                path,
                skeleton.name_span,
                format!("`{ENTRY_FUNCTION}`'s parameter must be declared `std::dynamic::Vec<txt>`"),
            ));
        }
        if let Some(return_type) = returns
            && !EntryShape::returns(return_type)
        {
            return Err(Diagnostic::new(
                path,
                skeleton.name_span,
                format!("`{ENTRY_FUNCTION}` returns int or nothing"),
            ));
        }
        Ok(index)
    }
}
