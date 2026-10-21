//! Checking the `use` paths a file writes.
//!
//! A `use` reaches a namespace, a type, or a plain function, and nothing else.
//! Every namespace and declaration is registered before this runs, so a path
//! that reaches a module value or a filed function is rejected here.

use crate::compiler::Diagnostic;
use crate::compiler::load_files::LoadedProgram;
use crate::compiler::model::join_path;

use super::builder::{Builder, UseMiss};

impl Builder {
    /// The warnings for the `use`s a namespace wrote but no body of the
    /// namespace ever read. A read is a resolution that took one delivery of
    /// the name: the reader records it where it reads, so a `use` absent from
    /// the recordings delivered nothing this namespace's bodies read.
    pub(in crate::compiler::resolve_names) fn unused_use_warnings(
        &self,
        loaded: &LoadedProgram,
    ) -> Vec<Diagnostic> {
        let used = self.used_uses.borrow();
        loaded
            .uses
            .iter()
            .enumerate()
            .filter(|(index, _)| !used.contains(index))
            .map(|(_, unused)| {
                Diagnostic::warning(
                    &loaded.files[unused.file].path,
                    unused.span,
                    format!("unused use `{}`", join_path(&unused.path)),
                )
            })
            .collect()
    }

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
    /// type, or a plain function. The target is resolved once, by
    /// `resolve_use`, for every pass that reads a `use`.
    fn use_rejection(&self, path: &[String]) -> Option<String> {
        match self.resolve_use(path) {
            Ok(_) => None,
            Err(UseMiss::Nothing) => Some(format!(
                "`{}` names neither a namespace nor a declaration",
                join_path(path)
            )),
            Err(UseMiss::Declaration) => Some(format!(
                "`{}` is a module value; `use` reaches a namespace, a type, or a plain function",
                join_path(path)
            )),
        }
    }
}
