//! The identities the model hands out: a loaded file, a namespace, and a
//! declaration or local binding.

/// The identity of a loaded file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct FileId(pub(crate) usize);

/// The identity of a namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct NamespaceId(pub(crate) usize);

/// The identity of a declaration or a local binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DeclarationId(pub(crate) usize);
