//! Checking the `use` paths a file writes.
//!
//! A `use` reaches a namespace, a type, or a plain function, and nothing else.
//! Every namespace and declaration is registered before this runs, so a path
//! that reaches a module value or a filed function is rejected here.

use crate::compiler::Diagnostic;
use crate::compiler::load_files::LoadedProgram;
use crate::compiler::model::{join_path, namespace_at};

use super::builder::Builder;

impl Builder {
    /// Reject a `use` whose path may not end where it does: a `use` reaches a
    /// namespace, a type, or a plain function, and nothing else. Every
    /// namespace and declaration is registered by the time this runs.
    pub(in crate::compiler::resolve_names) fn check_uses(
        &self,
        loaded: &LoadedProgram,
    ) -> Result<(), Diagnostic> {
        for used in &loaded.uses {
            if let Some(message) = self.use_rejection(&used.path) {
                return Err(Diagnostic::new(
                    &loaded.files[used.file].path,
                    used.span,
                    message,
                ));
            }
        }
        Ok(())
    }

    /// Why a `use` path is rejected, or `None` when it reaches a namespace, a
    /// type, or a plain function. A use path is absolute: its prefix names a
    /// namespace from the root.
    fn use_rejection(&self, path: &[String]) -> Option<String> {
        if namespace_at(&self.namespaces, path).is_some() {
            return None;
        }
        let unreached = || {
            format!(
                "`{}` names neither a namespace nor a declaration",
                join_path(path)
            )
        };
        let (name, prefix) = path.split_last()?;
        if prefix.is_empty() {
            return Some(unreached());
        }
        let Some(namespace) = namespace_at(&self.namespaces, prefix) else {
            return Some(unreached());
        };
        if self.namespaces[namespace.0].types.get(name).is_some() {
            return None;
        }
        if let Some(declaration) = self.namespaces[namespace.0].get(name).first().copied() {
            if self.skeletons[declaration.0].kind.useable() {
                return None;
            }
            return Some(format!(
                "`{}` is a module value; `use` reaches a namespace, a type, or a plain function",
                join_path(path)
            ));
        }
        if let Some(declaration) = self.filed_named(namespace, name) {
            return Some(format!(
                "`{}` belongs to the type `{}`; reach it through the type",
                join_path(path),
                self.bound_type_name(declaration),
            ));
        }
        Some(unreached())
    }
}
