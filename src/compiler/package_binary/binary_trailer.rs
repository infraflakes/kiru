//! The trailer that turns the kiru binary into a compiled program.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

const MAGIC: &[u8; 8] = b"KIRUPROG";
const TRAILER: usize = 16;

/// Append the payload trailer to a compiled binary.
pub(crate) fn attach_trailer(path: &Path, payload: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new().append(true).open(path)?;
    file.write_all(payload)?;
    file.write_all(&(payload.len() as u64).to_le_bytes())?;
    file.write_all(MAGIC)?;
    Ok(())
}

/// Read the payload from an executable, or `None` when it has no trailer.
pub(crate) fn read_trailer(path: &Path) -> Option<Vec<u8>> {
    let mut file = std::fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    if length < TRAILER as u64 {
        return None;
    }

    file.seek(SeekFrom::End(-(TRAILER as i64))).ok()?;
    let mut trailer = [0u8; TRAILER];
    file.read_exact(&mut trailer).ok()?;
    if &trailer[8..16] != MAGIC {
        return None;
    }

    let size = u64::from_le_bytes(trailer[0..8].try_into().ok()?);
    if size > length - TRAILER as u64 {
        return None;
    }
    let size = usize::try_from(size).ok()?;
    file.seek(SeekFrom::End(-((TRAILER + size) as i64))).ok()?;
    let mut payload = vec![0u8; size];
    file.read_exact(&mut payload).ok()?;
    Some(payload)
}
