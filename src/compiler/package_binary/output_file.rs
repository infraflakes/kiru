//! The output file: deciding where a compiled binary goes and making it
//! writable.

use std::path::Path;

/// Give a file the mode of a normal executable, so the trailer can be
/// appended to a copy of a read-only `kc`.
pub(crate) fn make_writable_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

/// Whether `output` names the same file as `entry`, following symlinks and
/// `..` when the output already exists.
pub(crate) fn same_file(entry: &Path, output: &Path) -> bool {
    let entry = entry.canonicalize().unwrap_or_else(|_| entry.to_path_buf());
    let output = match output.canonicalize() {
        Ok(canonical) => canonical,
        Err(_) => match (output.parent(), output.file_name()) {
            (Some(parent), Some(name)) => parent
                .canonicalize()
                .map(|parent| parent.join(name))
                .unwrap_or_else(|_| output.to_path_buf()),
            _ => output.to_path_buf(),
        },
    };
    entry == output
}
