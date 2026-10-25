use std::collections::HashMap;

use super::{DeclarationId, Type};
use crate::syntax::Span;

/// One expression site: the declaration that owns the body and the span of
/// the expression written there. A body has at most one expression at a span.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Site {
    pub(crate) owner: DeclarationId,
    pub(crate) span: Span,
}

impl Site {
    pub(crate) fn new(owner: DeclarationId, span: Span) -> Self {
        Self { owner, span }
    }
}

/// The facts the check derives from the given graph. The store is disposable:
/// the rules are the source of truth, and a later pass reads a settled result
/// rather than writing the graph.
#[derive(Debug, Default)]
pub(crate) struct Facts {
    halts: HashMap<DeclarationId, bool>,
    method_targets: HashMap<Site, DeclarationId>,
    pattern_types: HashMap<DeclarationId, Type>,
    match_coverage: HashMap<Site, bool>,
}

impl Facts {
    /// Whether every path of a callable ends in `panic;` or a call to one that
    /// does. An unsettled callable does not halt.
    pub(crate) fn halts(&self, id: DeclarationId) -> bool {
        self.halts.get(&id).copied().unwrap_or(false)
    }

    pub(crate) fn set_halts(&mut self, id: DeclarationId, halts: bool) {
        self.halts.insert(id, halts);
    }

    /// The declaration a method-family call selected, once the check has
    /// settled its receiver's head.
    pub(crate) fn method_target(&self, site: Site) -> Option<DeclarationId> {
        self.method_targets.get(&site).copied()
    }

    pub(crate) fn set_method_target(&mut self, site: Site, target: DeclarationId) {
        self.method_targets.insert(site, target);
    }

    /// The instantiated type of a variant-pattern binding, once its arm
    /// substituted the pattern it came from.
    pub(crate) fn pattern_type(&self, binding: DeclarationId) -> Option<&Type> {
        self.pattern_types.get(&binding)
    }

    pub(crate) fn set_pattern_type(&mut self, binding: DeclarationId, ty: Type) {
        self.pattern_types.insert(binding, ty);
    }

    /// Whether a match covers every value its subject can take. `None` when
    /// no check has settled the site.
    pub(crate) fn match_is_exhaustive(&self, site: Site) -> Option<bool> {
        self.match_coverage.get(&site).copied()
    }

    pub(crate) fn set_match_exhaustive(&mut self, site: Site, exhaustive: bool) {
        self.match_coverage.insert(site, exhaustive);
    }
}
