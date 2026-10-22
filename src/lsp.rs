//! The language server's glue: the protocol session, the position encoding at
//! the boundary, and the editor's buffers as a program source. The compiler
//! stays protocol-free; the answer the editor asks for is the tree analysis,
//! published one file or one tree at a time as the manifest asks.

mod positions;
mod server;
mod source;
mod tokens;

pub use positions::Encoding;
pub use server::serve;
pub(crate) use source::OverlaySource;
