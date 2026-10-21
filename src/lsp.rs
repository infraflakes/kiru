//! The language server's glue: the session loop over the protocol, the
//! position encoding at the boundary, and nothing else. The compiler stays
//! protocol-free; the answers the editor asks for are the build's queries.

mod positions;
mod server;

pub use positions::Encoding;
pub use server::{run, serve};
