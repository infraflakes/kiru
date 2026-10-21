//! The language server's glue: the protocol session, the position encoding at
//! the boundary, and the editor's buffers as a program source. The compiler
//! stays protocol-free; the answer the editor asks for is the tree analysis,
//! published for the file the editor holds.

mod positions;
mod server;
mod source;
mod tokens;

pub use positions::Encoding;
pub use server::serve;
pub(crate) use source::OverlaySource;
