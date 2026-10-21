//! Parsing: tokens become the syntax tree.
//!
//! `parser` owns the cursor and the entry point. The grammar layers live in
//! `declarations`, `statements`, and `expressions`, one file per category.

mod declarations;
mod expressions;
mod parser;
mod statements;

#[cfg(test)]
mod tests;

pub(crate) use parser::parse_file;
