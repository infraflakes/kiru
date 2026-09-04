//! direnv integration: opt-in wrapping of `$(command)` execution in
//! `direnv exec <dir>` so project environments apply to shell commands.
//!
//! The whole rule: wrap a project's commands when (a) `direnv = true` in
//! `kiru.toml`, (b) the direnv binary is on `PATH`, and (c) the project's
//! directory contains a `.envrc`. Anything else runs plain, and a missing
//! binary is silently ignored. direnv itself decides the rest: an untrusted
//! `.envrc` makes wrapped commands fail with direnv's own error, and a
//! directory without an rc runs the command unchanged.

use std::ffi::OsStr;
use std::path::Path;

/// The direnv binary name, looked up on `PATH`. Single home: both the
/// binary check and the wrapped argv reference it.
pub(crate) const DIRENV_PROGRAM: &str = "direnv";

/// Whether an executable `name` exists on the given `PATH` value. Pure so
/// tests can pass a synthetic PATH.
pub(crate) fn binary_on_path(path_var: &OsStr, name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::env::split_paths(path_var).any(|dir| {
        let binary = dir.join(name);
        binary
            .metadata()
            .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    })
}

/// Whether the `direnv` binary is on the process `PATH`.
pub(crate) fn direnv_on_path() -> bool {
    std::env::var_os("PATH")
        .map(|path_var| binary_on_path(&path_var, DIRENV_PROGRAM))
        .unwrap_or(false)
}

/// Whether the project's starting directory contains a `.envrc`.
pub(crate) fn has_envrc(cwd: &Path) -> bool {
    cwd.join(".envrc").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn binary_on_path_finds_an_executable() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join(DIRENV_PROGRAM);
        std::fs::write(&binary, b"").unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(binary_on_path(dir.path().as_os_str(), DIRENV_PROGRAM));
    }

    #[test]
    fn binary_on_path_ignores_a_non_executable_file() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join(DIRENV_PROGRAM);
        std::fs::write(&binary, b"").unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert!(!binary_on_path(dir.path().as_os_str(), DIRENV_PROGRAM));
    }

    #[test]
    fn binary_on_path_ignores_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!binary_on_path(dir.path().as_os_str(), DIRENV_PROGRAM));
    }

    #[test]
    fn binary_on_path_ignores_an_empty_path() {
        assert!(!binary_on_path(std::ffi::OsStr::new(""), DIRENV_PROGRAM));
    }

    #[test]
    fn has_envrc_checks_the_directory_itself() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!has_envrc(dir.path()));
        std::fs::write(dir.path().join(".envrc"), b"").unwrap();
        assert!(has_envrc(dir.path()));
    }
}
