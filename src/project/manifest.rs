//! Reading and validating one `kiru.toml`.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::compiler::WarningKind;

/// What the server publishes when it has checked the tree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DiagnosticScope {
    /// The open file only.
    #[default]
    File,
    /// Every file of the project tree.
    Tree,
}

/// How one unused fact is reported.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum WarningSetting {
    /// Report it as a warning.
    #[default]
    Warning,
    /// Do not report it.
    Off,
}

/// Which unused facts the project reports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WarningPolicy {
    pub(crate) unused_use: WarningSetting,
    pub(crate) unused_local: WarningSetting,
    pub(crate) unused_declaration: WarningSetting,
}

impl WarningPolicy {
    /// Whether a warning of this kind reaches the terminal or the editor.
    pub(crate) fn admits(self, kind: WarningKind) -> bool {
        let setting = match kind {
            WarningKind::UnusedUse => self.unused_use,
            WarningKind::UnusedLocal => self.unused_local,
            WarningKind::UnusedDeclaration => self.unused_declaration,
        };
        setting == WarningSetting::Warning
    }
}

/// One project: its manifest, the program tree it names, the binary it
/// builds, and the facts the server publishes.
#[derive(Clone, Debug)]
pub(crate) struct Project {
    pub(crate) manifest: PathBuf,
    pub(crate) root: PathBuf,
    pub(crate) output: PathBuf,
    pub(crate) scope: DiagnosticScope,
    pub(crate) warnings: WarningPolicy,
}

/// A manifest that cannot be a project: the file and what is wrong with it.
#[derive(Debug)]
pub(crate) struct ProjectError {
    pub(crate) manifest: PathBuf,
    pub(crate) message: String,
}

impl ProjectError {
    fn new(manifest: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self {
            manifest: manifest.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ProjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.manifest.display(), self.message)
    }
}

/// The manifest as written. Every table and key is known: a typo is an error,
/// not a silently ignored line.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    project: RawProject,
    #[serde(default)]
    lsp: RawLsp,
    #[serde(default)]
    diagnostics: RawDiagnostics,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProject {
    root: PathBuf,
    output: PathBuf,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLsp {
    scope: Option<DiagnosticScope>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDiagnostics {
    #[serde(rename = "unused-use")]
    unused_use: Option<WarningSetting>,
    #[serde(rename = "unused-local")]
    unused_local: Option<WarningSetting>,
    #[serde(rename = "unused-declaration")]
    unused_declaration: Option<WarningSetting>,
}

/// The one manifest name: a project is found by its file, not by a pattern.
const MANIFEST: &str = "kiru.toml";

impl Project {
    /// Read and validate one manifest. Every path it names is settled against
    /// the manifest's directory; the tree root is the manifest's tree.
    pub(crate) fn load(path: &Path) -> Result<Self, ProjectError> {
        if path.file_name().is_none_or(|name| name != MANIFEST) {
            return Err(ProjectError::new(
                path,
                format!("not a project manifest; name the `{MANIFEST}` file"),
            ));
        }
        let manifest = path.canonicalize().map_err(|error| {
            ProjectError::new(path, format!("cannot read the manifest: {error}"))
        })?;
        let text = std::fs::read_to_string(&manifest).map_err(|error| {
            ProjectError::new(&manifest, format!("cannot read the manifest: {error}"))
        })?;
        let raw: RawManifest = toml::from_str(&text)
            .map_err(|error| ProjectError::new(&manifest, error.to_string()))?;
        let directory = manifest.parent().expect("a canonical file has a directory");
        let root = directory.join(&raw.project.root);
        let root = root.canonicalize().map_err(|error| {
            ProjectError::new(
                &manifest,
                format!("cannot read the program tree `{}`: {error}", root.display()),
            )
        })?;
        if !root.is_dir() {
            return Err(ProjectError::new(
                &manifest,
                format!("the program tree `{}` is not a directory", root.display()),
            ));
        }
        let output = directory.join(&raw.project.output);
        if output == manifest {
            return Err(ProjectError::new(
                &manifest,
                "the output would overwrite the manifest; set `project.output`",
            ));
        }
        if output
            .extension()
            .is_some_and(|extension| extension == "kiru")
        {
            return Err(ProjectError::new(
                &manifest,
                "the output would overwrite a source file; set `project.output`",
            ));
        }
        Ok(Self {
            manifest,
            root,
            output,
            scope: raw.lsp.scope.unwrap_or_default(),
            warnings: WarningPolicy {
                unused_use: raw.diagnostics.unused_use.unwrap_or_default(),
                unused_local: raw.diagnostics.unused_local.unwrap_or_default(),
                unused_declaration: raw.diagnostics.unused_declaration.unwrap_or_default(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrote(manifest: &str) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join(MANIFEST);
        std::fs::write(&path, manifest).expect("the manifest");
        (directory, path)
    }

    #[test]
    fn output_is_settled_against_the_manifest_directory() {
        let (directory, manifest) = wrote("[project]\nroot = \".\"\noutput = \"build/app\"\n");
        let project = Project::load(&manifest).expect("the manifest is valid");
        assert_eq!(project.output, directory.path().join("build/app"));
        assert_eq!(project.root, directory.path().canonicalize().unwrap());
    }

    #[test]
    fn a_manifest_without_output_is_rejected() {
        let (_directory, manifest) = wrote("[project]\nroot = \".\"\n");
        let error = Project::load(&manifest).expect_err("the output is required");
        assert!(error.message.contains("missing field `output`"), "{error}");
    }
}
