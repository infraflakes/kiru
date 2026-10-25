//! Checking the `use` paths a file writes.
//!
//! A `use` reaches a namespace, a type, or a plain function, and nothing else.
//! Every namespace and declaration is registered before this runs, so a path
//! that reaches a module value or a filed function is rejected here.

use std::collections::HashMap;

use crate::compiler::Diagnostic;
use crate::compiler::compile::load::LoadedProgram;
use crate::compiler::model::{UseTarget, join_path};

use super::builder::Builder;

/// The status of one `use` item. `Taken` means a body read at least one target;
/// `Unused` means it read none; `Redundant` means every target was already
/// delivered by an earlier item under the same name. The status is derived
/// once from the delivery edges; no warning pass resolves a path again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UseStatus {
    Taken,
    Unused,
    Redundant,
}

impl Builder {
    /// The warnings for the `use`s a namespace wrote. The status of each item
    /// is settled from the delivery edges, then the warnings are emitted in
    /// source order. A type and a function may share a spelling, so one `use`
    /// can deliver both; it is redundant only when every target it delivers
    /// was already delivered by an earlier item under the same name.
    pub(in crate::compiler::compile::resolve) fn use_warnings(
        &self,
        loaded: &LoadedProgram,
    ) -> Vec<Diagnostic> {
        let statuses = self.settle_use_statuses(loaded);
        let mut warnings = Vec::new();
        for (index, item) in loaded.uses.iter().enumerate() {
            let Some((status, first)) = statuses[index] else {
                continue;
            };
            match status {
                UseStatus::Taken => {}
                UseStatus::Unused => warnings.push(Diagnostic::warning(
                    &loaded.files[item.file].path,
                    item.span,
                    format!("unused use `{}`", join_path(&item.path)),
                )),
                UseStatus::Redundant => {
                    let first = first.expect("a redundant use names its first delivery");
                    let first_item = &loaded.uses[first];
                    warnings.push(
                        Diagnostic::warning(
                            &loaded.files[item.file].path,
                            item.span,
                            format!("redundant use `{}`", join_path(&item.path)),
                        )
                        .with_related(
                            &loaded.files[first_item.file].path,
                            first_item.span,
                            "first use is here",
                        ),
                    );
                }
            }
        }
        warnings
    }

    /// Settle every `use` item's status from the delivery edges. A delivery
    /// carries the target and whether a resolution took it; the target set of
    /// one item is read back by its index. The first item that delivered a
    /// target keeps that fact, so a later item that delivers only targets
    /// already delivered is redundant, while a later item that adds a new
    /// target is not.
    fn settle_use_statuses(
        &self,
        loaded: &LoadedProgram,
    ) -> Vec<Option<(UseStatus, Option<usize>)>> {
        let mut targets_by_use: HashMap<usize, Vec<UseTarget>> = HashMap::new();
        let mut taken_by_use: HashMap<usize, bool> = HashMap::new();
        for namespace in &self.namespaces {
            for entries in namespace.brought.values() {
                for delivery in entries {
                    targets_by_use
                        .entry(delivery.use_index)
                        .or_default()
                        .push(delivery.target);
                    if delivery.taken.get() {
                        taken_by_use.insert(delivery.use_index, true);
                    }
                }
            }
        }
        let mut firsts: Vec<(&[String], &str, UseTarget, usize)> = Vec::new();
        let mut statuses = vec![None; loaded.uses.len()];
        for (index, item) in loaded.uses.iter().enumerate() {
            let Some(targets) = targets_by_use.get(&index) else {
                continue;
            };
            let name = item
                .path
                .last()
                .expect("a `use` path is not empty")
                .as_str();
            let covered = targets.iter().all(|target| {
                firsts.iter().any(|(namespace, delivered, first, _)| {
                    *namespace == item.namespace.as_slice()
                        && *delivered == name
                        && *first == *target
                })
            });
            if covered {
                let first = targets
                    .iter()
                    .filter_map(|target| {
                        firsts
                            .iter()
                            .find(|(namespace, delivered, first, _)| {
                                *namespace == item.namespace.as_slice()
                                    && *delivered == name
                                    && *first == *target
                            })
                            .map(|(_, _, _, index)| *index)
                    })
                    .min();
                statuses[index] = Some((UseStatus::Redundant, first));
                continue;
            }
            for target in targets {
                let known = firsts.iter().any(|(namespace, delivered, first, _)| {
                    *namespace == item.namespace.as_slice()
                        && *delivered == name
                        && *first == *target
                });
                if !known {
                    firsts.push((&item.namespace, name, *target, index));
                }
            }
            let status = if taken_by_use.get(&index).copied().unwrap_or(false) {
                UseStatus::Taken
            } else {
                UseStatus::Unused
            };
            statuses[index] = Some((status, None));
        }
        statuses
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
        self.resolve_uses(path).is_empty().then(|| {
            format!(
                "`{}` names no namespace, type, or function",
                join_path(path)
            )
        })
    }
}
