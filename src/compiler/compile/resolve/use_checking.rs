//! Checking the `use` paths a file writes.
//!
//! A `use` reaches a namespace, a type, or a plain function, and nothing else.
//! Every namespace and declaration is registered before this runs, so a path
//! that reaches a module value or a filed function is rejected here.

use crate::compiler::Diagnostic;
use crate::compiler::compile::load::LoadedProgram;
use crate::compiler::model::join_path;

use super::builder::Builder;

impl Builder {
    /// The warnings for the `use`s a namespace wrote. A `use` is used when a
    /// body reads a delivery of its target; two items that deliver the same
    /// target under the same name are one namespace fact written twice, so the
    /// later ones are redundant rather than unused. The first item stays the
    /// one the unused fact names when no body reads the target.
    pub(in crate::compiler::compile::resolve) fn use_warnings(
        &self,
        loaded: &LoadedProgram,
    ) -> Vec<Diagnostic> {
        let used = self.used_uses.borrow();
        let mut firsts: Vec<(&[String], &str, crate::compiler::model::UseTarget, usize)> =
            Vec::new();
        let mut warnings = Vec::new();
        for (index, item) in loaded.uses.iter().enumerate() {
            let Some(target) = self.resolve_use(&item.path) else {
                continue;
            };
            let name = item
                .path
                .last()
                .expect("a `use` path is not empty")
                .as_str();
            if let Some((_, _, _, first)) =
                firsts.iter().find(|(namespace, delivered, first, _)| {
                    *namespace == item.namespace.as_slice()
                        && *delivered == name
                        && *first == target
                })
            {
                let first = *first;
                let first_item = &loaded.uses[first];
                warnings.push(
                    Diagnostic::warning(
                        &loaded.files[item.file].path,
                        item.span,
                        format!("redundant use `{}`", join_path(&item.path)),
                        crate::compiler::WarningKind::RedundantUse,
                    )
                    .with_related(
                        &loaded.files[first_item.file].path,
                        first_item.span,
                        "first use is here",
                    ),
                );
                continue;
            }
            firsts.push((&item.namespace, name, target, index));
            if !used.contains(&index) {
                warnings.push(Diagnostic::warning(
                    &loaded.files[item.file].path,
                    item.span,
                    format!("unused use `{}`", join_path(&item.path)),
                    crate::compiler::WarningKind::UnusedUse,
                ));
            }
        }
        warnings
    }

    /// Reject a `use` whose path may not end where it does: a `use` reaches a
    /// namespace, a type, or a plain function, and nothing else. Every
    /// namespace and declaration is registered by the time this runs.
    pub(in crate::compiler::compile::resolve) fn check_uses(
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
        self.resolve_use(path).is_none().then(|| {
            format!(
                "`{}` names no namespace, type, or function",
                join_path(path)
            )
        })
    }
}
