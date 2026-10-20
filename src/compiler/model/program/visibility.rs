//! The one visibility relation: whether a declaration, member, or type in one
//! namespace may be reached from another.
//!
//! A name in its own namespace is always reachable. Across namespaces it is
//! reachable only when it is public. A declaration kind that takes no
//! visibility never crosses a namespace because its public flag is never set;
//! the kind fact decides whether the surface may write `pub`, and this relation
//! decides what `pub` reaches.

use super::super::ids::NamespaceId;

/// Whether something declared in `declaring` is reachable from `caller`.
pub(crate) fn reachable(public: bool, declaring: NamespaceId, caller: NamespaceId) -> bool {
    caller == declaring || public
}

/// Whether a caller may reach a bound function filed in `owner`: in its own
/// namespace always, and across namespaces only when the function is `pub`
/// and the type it is filed under is `pub`. A bound function is never gated
/// by a `use`. `type_public` is the visibility of the type the function is
/// filed under, when it is filed under a named one.
pub(crate) fn bound_reachable(
    public: bool,
    type_public: Option<bool>,
    owner: NamespaceId,
    caller: NamespaceId,
) -> bool {
    reachable(public, owner, caller)
        && type_public.is_none_or(|public| reachable(public, owner, caller))
}
