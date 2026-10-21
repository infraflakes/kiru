//! The trailer that turns the kiru binary into a compiled program.

use std::io::Write;
use std::path::Path;

const MAGIC: &[u8; 8] = b"KIRUPROG";
/// The trailer is the payload length, which is one `u64`, followed by the magic.
const TRAILER: usize = MAGIC.len() + std::mem::size_of::<u64>();

/// Append the payload trailer to a compiled binary.
pub(crate) fn attach_trailer(path: &Path, payload: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new().append(true).open(path)?;
    file.write_all(payload)?;
    file.write_all(&(payload.len() as u64).to_le_bytes())?;
    file.write_all(MAGIC)?;
    Ok(())
}

/// The payload a compiled binary's trailer carries, or `None` when the bytes
/// carry no trailer. The bytes are the executable's own: reading them is the
/// caller's job.
pub(crate) fn payload_in(executable: &[u8]) -> Option<&[u8]> {
    let length = executable.len();
    if length < TRAILER {
        return None;
    }
    let trailer = &executable[length - TRAILER..];
    if &trailer[MAGIC.len()..TRAILER] != MAGIC {
        return None;
    }
    let size = u64::from_le_bytes(trailer[..MAGIC.len()].try_into().ok()?);
    if size > (length - TRAILER) as u64 {
        return None;
    }
    let size = usize::try_from(size).ok()?;
    Some(&executable[length - TRAILER - size..length - TRAILER])
}
