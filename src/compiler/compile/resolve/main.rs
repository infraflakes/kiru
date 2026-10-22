//! The program's entry: the root namespace's `main`.

use crate::compiler::Diagnostic;
use crate::compiler::compile::load::PROGRAM_TREE;
use crate::compiler::model::Type;
use crate::compiler::model::{DeclarationId, ENTRY_FUNCTION, TypeDecl, fits, list_type};

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

/// The root namespace's `main`, if the program declares one. A missing `main`
/// is not an error here: the compiler requires it, and a file analysis does
/// not. A `main` that exists but cannot be an entry is an error, because it is
/// a fact about the file that declares it.
pub(crate) fn program_main(
    program: &crate::compiler::model::Program,
) -> Result<Option<DeclarationId>, Diagnostic> {
    let root = program
        .namespace_at(&[PROGRAM_TREE.to_owned()])
        .unwrap_or_else(|| program.root());
    let Some(index) = program.namespace(root).get(&program.intern, ENTRY_FUNCTION) else {
        return Ok(None);
    };
    let declaration = program.declaration(index);
    let file = declaration
        .file
        .expect("a root main is a file declaration, so it has a file");
    let path = &program.files[file.0].path;
    if !declaration.kind.may_be_entry() {
        return Err(Diagnostic::new(
            path,
            declaration.name_span,
            format!("`{ENTRY_FUNCTION}` is not a function"),
        ));
    }
    let Type::Function {
        parameters,
        returns,
    } = &declaration.ty
    else {
        unreachable!("a function declaration carries a function type");
    };
    if !EntryShape::accepts_parameters(parameters.len()) {
        return Err(Diagnostic::new(
            path,
            declaration.name_span,
            format!("`{ENTRY_FUNCTION}` takes at most one parameter"),
        ));
    }
    if let Some(parameter) = parameters.first()
        && !fits(
            parameter,
            &EntryShape::parameter_type(&program.types),
            &program.types,
        )
    {
        return Err(Diagnostic::new(
            path,
            declaration.name_span,
            format!("`{ENTRY_FUNCTION}`'s parameter must be declared `std::dynamic::Vec<txt>`"),
        ));
    }
    if let Some(return_type) = returns
        && !EntryShape::returns(return_type)
    {
        return Err(Diagnostic::new(
            path,
            declaration.name_span,
            format!("`{ENTRY_FUNCTION}` returns int or nothing"),
        ));
    }
    Ok(Some(index))
}
