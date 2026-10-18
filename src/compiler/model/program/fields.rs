//! Field lookups over the declared types, shared by the linker and the
//! checker.

use super::super::declarations::{TypeDecl, TypeFieldDecl, TypeKind};
use crate::compiler::model::TypeId;

/// The declared field node of a `rec` type, or `None` when the type's group has
/// no fields or it has no such field. The linker's receiver typing and the
/// checker's field access both read a field through this one lookup; a caller
/// that needs the field's type substitutes it with the applied type arguments.
pub(crate) fn rec_field<'a>(
    types: &'a [TypeDecl],
    id: TypeId,
    name: &str,
) -> Option<&'a TypeFieldDecl> {
    let declaration = &types[id.0];
    if !declaration.group().has_fields() {
        return None;
    }
    let TypeKind::Rec(fields) = &declaration.kind else {
        unreachable!("a group with fields is a rec");
    };
    fields.iter().find(|field| field.name == name)
}
