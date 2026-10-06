//! Linking bodies: resolve every name into an edge and build the model.
//!
//! The linking pass walks each pending declaration's parsed body, creating
//! local bindings and resolving references and calls into edges to
//! declaration nodes. `scopes` holds the open scopes and the linking context,
//! `reference_resolution` turns a name into an edge, and `linking` walks the
//! bodies. After linking, no phase resolves a name again.

mod linking;
mod reference_resolution;
mod scopes;

pub(crate) use linking::resolve_names;
