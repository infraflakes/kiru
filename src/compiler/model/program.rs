//! The program graph: the program, its files and namespaces, the name
//! registries a namespace keeps, type-name rendering, namespace paths, and the
//! field lookups the linker and the checker share.

mod fields;
mod graph;
mod namespace_paths;
mod tables;
mod type_name;
mod visibility;

pub(crate) use fields::rec_field;
pub(crate) use graph::{File, Namespace, Origin, Program, first_duplicate};
pub(crate) use namespace_paths::{join_path, namespace_at};
pub(crate) use tables::{MethodTable, NameTable, TypeTable};
pub(crate) use visibility::reachable;
