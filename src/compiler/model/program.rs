//! The program graph: the program, its files and namespaces, the name
//! registries a namespace keeps, type-name rendering, namespace paths, and the
//! field lookups the linker and the checker share.

mod fields;
mod graph;
mod interner;
mod namespace_paths;
mod tables;
mod view;
mod visibility;

pub(crate) use fields::rec_field;
pub(crate) use graph::{Delivery, File, Namespace, Program, UseTarget, first_duplicate};
/// The name hash a test reads to pin the compiled image's bytes; nothing in
/// the model itself reads it.
#[cfg(test)]
pub(crate) use interner::name_hash;
pub(crate) use interner::{Interner, Symbol};
pub(crate) use namespace_paths::{join_path, namespace_at};
pub(crate) use tables::{MethodTable, NameTable, TypeTable};
pub(crate) use view::ModelView;
pub(crate) use visibility::{bound_reachable, reachable};
