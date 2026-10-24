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

    /// Rewrite every declaration id through `mapping`, keeping only the facts
    /// whose node is retained. A retained method site names a retained target,
    /// because reachability followed the target before compaction.
    pub(crate) fn remap(&mut self, mapping: &[Option<DeclarationId>]) {
        let halts = std::mem::take(&mut self.halts);
        self.halts = halts
            .into_iter()
            .filter_map(|(id, value)| retained(mapping, id).map(|id| (id, value)))
            .collect();

        let method_targets = std::mem::take(&mut self.method_targets);
        self.method_targets = method_targets
            .into_iter()
            .filter_map(|(site, target)| {
                let site = retained_site(mapping, site)?;
                let target = retained(mapping, target)
                    .expect("a retained method site names a retained target");
                Some((site, target))
            })
            .collect();

        let pattern_types = std::mem::take(&mut self.pattern_types);
        self.pattern_types = pattern_types
            .into_iter()
            .filter_map(|(binding, ty)| retained(mapping, binding).map(|binding| (binding, ty)))
            .collect();

        let match_coverage = std::mem::take(&mut self.match_coverage);
        self.match_coverage = match_coverage
            .into_iter()
            .filter_map(|(site, exhaustive)| {
                retained_site(mapping, site).map(|site| (site, exhaustive))
            })
            .collect();
    }
}

/// The new id of a retained declaration, or `None` when the id is dropped.
fn retained(mapping: &[Option<DeclarationId>], id: DeclarationId) -> Option<DeclarationId> {
    mapping.get(id.0).copied().flatten()
}

/// The remapped site, or `None` when the body that owns it is dropped.
fn retained_site(mapping: &[Option<DeclarationId>], site: Site) -> Option<Site> {
    retained(mapping, site.owner).map(|owner| Site::new(owner, site.span))
}
