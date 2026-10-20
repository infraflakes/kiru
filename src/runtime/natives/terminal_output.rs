//! The output natives: writing to a file descriptor and asking whether one is
//! a terminal.
//!
//! Both are the operating system's call. The native takes any descriptor and
//! hands it to the kernel, which answers `EBADF` for a descriptor it does not
//! know; that error is the run failure, not a name check above the syscall.

use std::io::Read;
use std::os::fd::BorrowedFd;

use nix::errno::Errno;
use nix::unistd::{isatty, write as write_fd};

use crate::runtime::Value;

/// The descriptor the kernel will be asked about. A number outside the
/// platform's descriptor type cannot name a descriptor, so the kernel's answer
/// for it is `EBADF`.
fn descriptor(file_descriptor: i64) -> Result<i32, String> {
    i32::try_from(file_descriptor).map_err(|_| Errno::EBADF.to_string())
}

/// The borrowed descriptor a syscall reads. The native owns no descriptor, so
/// it never closes the caller's stream.
fn borrowed(file_descriptor: i64) -> Result<BorrowedFd<'static>, String> {
    let raw = descriptor(file_descriptor)?;
    // SAFETY: the descriptor is the caller's, and this value neither closes it
    // nor outlives the call.
    Ok(unsafe { BorrowedFd::borrow_raw(raw) })
}

/// Write `text` to `file_descriptor`, whatever descriptor it names.
pub(crate) fn write(file_descriptor: i64, text: &str) -> Result<Value, String> {
    let stream = borrowed(file_descriptor)?;
    let mut remaining = text.as_bytes();
    while !remaining.is_empty() {
        let written = write_fd(stream, remaining).map_err(|error| error.to_string())?;
        if written == 0 {
            break;
        }
        remaining = &remaining[written..];
    }
    Ok(Value::Nothing)
}

/// Whether `file_descriptor` is a terminal. A valid descriptor that is not a
/// terminal answers false; an unknown descriptor is the kernel's error.
pub(crate) fn is_terminal(file_descriptor: i64) -> Result<Value, String> {
    let stream = borrowed(file_descriptor)?;
    let terminal = isatty(stream).map_err(|error| error.to_string())?;
    Ok(Value::Bool(terminal))
}

/// Read all of standard input as text.
pub(crate) fn read_stdin() -> Result<Value, String> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|error| error.to_string())?;
    Ok(Value::Text(text.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A descriptor nobody holds reaches the kernel, and the kernel's `EBADF`
    /// is the failure; there is no list of accepted descriptors above it.
    #[test]
    fn an_unknown_descriptor_is_the_kernels_ebadf() {
        let error = write(9_999, "x").expect_err("the kernel rejects an unused descriptor");
        assert!(
            error.contains("EBADF") || error.to_lowercase().contains("bad file"),
            "write reported `{error}`"
        );
        let error = is_terminal(9_999).expect_err("the kernel rejects an unused descriptor");
        assert!(
            error.contains("EBADF") || error.to_lowercase().contains("bad file"),
            "isatty reported `{error}`"
        );
    }
}
