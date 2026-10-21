//! The language server's glue: the protocol session, the position encoding at
//! the boundary, and nothing else. The compiler stays protocol-free; the
//! answer the editor asks for is the single-file check.

mod positions;
mod server;

pub use positions::Encoding;
pub use server::serve;
