//! The builder: reserve declaration nodes and register their names.
//!
//! Registration builds the skeleton of the model: namespaces, declaration
//! nodes, and the worklist of bodies still to link. The linking pass fills
//! every body. A namespace keeps a name pool of its namespace-level names, a
//! method pool keyed by receiver head and name, and an associated-function
//! pool keyed by type head and name, so a method or associated-function name
//! may repeat once per head in its pool and is never a bare name.

mod module_values;
mod namespace_paths;
mod registration;
mod type_resolution;

pub(in crate::compiler::compile::resolve) use registration::{
    Builder, DeclarationSite, Pending, Skeleton, Slot,
};
pub(in crate::compiler::compile::resolve) use type_resolution::Variant;
