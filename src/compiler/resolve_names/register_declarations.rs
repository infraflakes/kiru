//! Linking registration: reserve declaration nodes and register their names.
//!
//! Registration builds the skeleton of the model: namespaces, declaration
//! nodes, and the worklist of bodies still to link. The linking pass in
//! `link_bodies` then fills every body. `builder` holds the core, `builtins`
//! registers the runtime natives, `files` registers parsed files, and `entry`
//! selects the entry.

mod builder;
mod builtins;
mod entry;
mod files;

pub(super) use builder::{Builder, Skeleton};
