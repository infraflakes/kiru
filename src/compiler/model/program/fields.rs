//! Field lookups over the declared types, shared by the linker and the
//! checker.

use super::super::declarations::{Declaration, TypeFieldDecl, TypeKind};
use super::super::ids::DeclarationId;

/// The declared field node of a `rec` type, or `None` when the type's group has
/// no fields or it has no such field. The linker's receiver typing and the
/// checker's field access both read a field through this one lookup; a caller
/// that needs the field's type substitutes it with the applied type arguments.
pub(crate) fn rec_field<'a>(
    declarations: &'a [Declaration],
    id: DeclarationId,
    name: &str,
) -> Option<&'a TypeFieldDecl> {
    let body = declarations[id.0]
        .type_body()
        .expect("a named type names a type declaration");
    if !body.group().has_fields() {
        return None;
    }
    let TypeKind::Rec(fields) = &body.kind else {
        unreachable!("a group with fields is a rec");
    };
    fields.iter().find(|field| field.name == name)
}
