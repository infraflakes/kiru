//! Reading the standard library and the program tree.
//!
//! A file belongs to its directory's namespace: the path from the tree root,
//! with the tree's name as the first segment. The standard library's files come
//! from a table compiled into the binary; a program tree is every `.kiru` file
//! under the target's directory, named by the program source. There is no
//! import: a file is in the tree because the source names it, and resolution is
//! order-free.

pub(super) mod embedded;
mod record;
mod tree;

pub(crate) use tree::{PROGRAM_TREE, STD_TREE, load_files, load_single};
