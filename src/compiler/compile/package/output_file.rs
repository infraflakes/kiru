//! The output file: deciding where a compiled binary goes and making it
//! writable.

use std::path::Path;

/// Give a file the mode of a normal executable, so the trailer can be
/// appended to a copy of a read-only `kc`.
pub(crate) fn make_writable_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}
