//! The language server's glue: the protocol session, the position encoding at
//! the boundary, and the editor's buffers as a program source. The compiler
//! stays protocol-free; the answer the editor asks for is the single-file
//! analysis.

mod positions;
mod server;
mod source;

pub use positions::Encoding;
pub use server::serve;
pub(crate) use source::OverlaySource;
